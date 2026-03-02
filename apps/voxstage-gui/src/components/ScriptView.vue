<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ScriptItemType, ScriptItem } from "../types/script";
import type { RoleEntry, AppConfig } from "../types/config";
import { createItem, toVox } from "../types/script";
import ExprInput from "./ExprInput.vue";

const items = ref<ScriptItem[]>([]);
const roleNames = ref<string[]>([]);
const speakParamsEditingIndex = ref<number | null>(null);
const speakParamsDraft = ref<{ key: string; value: string }[]>([]);
const roleProviderMap = ref<Record<string, string>>({});
const currentSpeakRoleName = ref<string>("");
const currentSpeakProvider = ref<string>("");

/** 编辑 | Code 双模式 */
const mode = ref<"edit" | "code">("edit");
const codeText = ref("");
const parseError = ref<string | null>(null);

/** 当前运行到的静态语句索引（与 EngineCommand source_index 对齐），用于高亮当前步骤 */
const activeSourceIndex = ref<number | null>(null);
/** 当前高亮的列表行下标（0-based），用于在 UI 中展示更直观的进度 */
const activeRowIndex = ref<number | null>(null);

/** 可作为子步骤添加的类型及其标签 */
const childTypeOptions: { value: ScriptItemType; label: string }[] = [
  { value: "speak", label: "说话" },
  { value: "sleep", label: "等待" },
  { value: "let", label: "定义变量" },
  { value: "set", label: "设置变量" },
  { value: "if", label: "如果" },
  { value: "for", label: "循环" },
  { value: "while", label: "当" },
  { value: "bgm_play", label: "BGM 播放" },
  { value: "bgm_volume", label: "BGM 音量" },
  { value: "bgm_pause", label: "BGM 暂停" },
  { value: "bgm_resume", label: "BGM 恢复" },
  { value: "bgm_stop", label: "BGM 停止" },
];

/** 表达式编辑辅助：可用变量列表（来自当前脚本中的 let/set） */
const availableVars = computed(() => {
  const set = new Set<string>();
  for (const it of items.value) {
    if ((it.type === "let" || it.type === "set") && it.varName) {
      set.add(it.varName);
    }
  }
  return Array.from(set);
});

/** 表达式编辑辅助：内置函数模板 */
const builtinFunctions = [
  { name: "now()", snippet: "now()", desc: "当前 Unix 时间戳（秒）" },
  { name: "time_hour()", snippet: "time_hour()", desc: "当前小时 (0-23)" },
  { name: "time_minute()", snippet: "time_minute()", desc: "当前分钟 (0-59)" },
  { name: "time_second()", snippet: "time_second()", desc: "当前秒钟 (0-59)" },
  { name: "rand()", snippet: "rand()", desc: "随机整数 (0-999999999)" },
  { name: "rand_int(1, 10)", snippet: "rand_int(1, 10)", desc: "给定区间内随机整数" },
  { name: "rand_bool()", snippet: "rand_bool()", desc: "随机布尔值" },
  {
    name: 'rand_choice("a", "b")',
    snippet: 'rand_choice("a", "b")',
    desc: "从多个选项中随机选择一个",
  },
];

async function loadRoles() {
  try {
    const roles = await invoke<RoleEntry[]>("get_roles");
    roleNames.value = roles.map((r) => r.name);
  } catch (e) {
    console.error("load roles failed", e);
  }
}

async function loadRoleProviderMap() {
  try {
    const cfg = await invoke<AppConfig>("get_config");
    const modelProvider: Record<string, string> = {};
    for (const m of cfg.models) {
      if (m.name && m.provider) {
        modelProvider[m.name] = m.provider;
      }
    }
    const map: Record<string, string> = {};
    for (const r of cfg.roles) {
      const p = modelProvider[r.model];
      if (p) {
        map[r.name] = p;
      }
    }
    roleProviderMap.value = map;
  } catch (e) {
    console.error("load role provider map failed", e);
  }
}

/** 切换到 Code 模式：用当前 config + items 生成 .vox 文本 */
async function switchToCode() {
  parseError.value = null;
  try {
    const cfg = await invoke<AppConfig>("get_config");
    codeText.value = toVox(cfg, items.value);
    mode.value = "code";
  } catch (e) {
    parseError.value = String(e);
  }
}

/** 从 Code 模式切回编辑：解析 .vox 文本并更新 items */
async function applyCodeAndSwitchToEdit() {
  parseError.value = null;
  try {
    const list = await invoke<ScriptItem[]>("parse_vox_to_script", {
      voxText: codeText.value,
    });
    items.value = list;
    mode.value = "edit";
  } catch (e) {
    parseError.value = String(e);
  }
}

/** 打开 .vox / .json 脚本文件 */
const fileError = ref<string | null>(null);
async function openScriptFile() {
  fileError.value = null;
  try {
    const result = await invoke<{ path: string; content: string }>("open_script_file");
    const path = result.path.toLowerCase();
    if (path.endsWith(".json")) {
      const list = JSON.parse(result.content) as ScriptItem[];
      items.value = list;
    } else {
      const list = await invoke<ScriptItem[]>("parse_vox_to_script", {
        voxText: result.content,
      });
      items.value = list;
    }
    mode.value = "edit";
  } catch (e) {
    fileError.value = String(e);
  }
}

/** 另存为 .vox 文件 */
async function saveScriptAs() {
  fileError.value = null;
  try {
    const cfg = await invoke<AppConfig>("get_config");
    const content = toVox(cfg, items.value);
    await invoke<string>("save_script_file", { content });
  } catch (e) {
    fileError.value = String(e);
  }
}

function addRoot(type: ScriptItemType) {
  items.value.push(createItem(type, 0));
}

/** 当前行代表的「移动单元」的结束索引：若是块则含整块，否则仅当前行 */
function moveRangeEnd(index: number): number {
  const arr = items.value;
  const cur = arr[index];
  if (!cur) return index;
  if (blockTypes.includes(cur.type)) return blockEndIndex(index);
  return index;
}

/** 从 index 往前找上一个同缩进的兄弟起始下标（含块则取块头） */
function prevSiblingStart(index: number): number {
  const arr = items.value;
  const cur = arr[index];
  if (!cur) return -1;
  for (let j = index - 1; j >= 0; j--) {
    if (arr[j].indent === cur.indent) return j;
  }
  return -1;
}

/** 从 rangeEnd+1 往后找下一个同缩进的兄弟起始下标（含块则取块头） */
function nextSiblingStart(rangeEnd: number): number {
  const arr = items.value;
  if (rangeEnd + 1 >= arr.length) return -1;
  const cur = arr[rangeEnd + 1];
  if (!cur) return -1;
  return rangeEnd + 1;
}

/** 上移：找上一个同层级兄弟（可能是块），整段与当前段交换；当前若是块则含子级 */
function moveUp(index: number) {
  const arr = items.value;
  if (index <= 0) return;
  const prevStart = prevSiblingStart(index);
  if (prevStart < 0) return;
  // 仅允许在同一父块（或同为顶层）内移动
  const parentCur = getBlockParentIndex(index);
  const parentPrev = getBlockParentIndex(prevStart);
  if (parentCur !== parentPrev || arr[prevStart].indent !== arr[index].indent) return;
  const rangeEnd = moveRangeEnd(index);
  const prevEnd = blockTypes.includes(arr[prevStart].type) ? blockEndIndex(prevStart) : prevStart;
  const chunkMe = arr.splice(index, rangeEnd - index + 1);
  const chunkPrev = arr.splice(prevStart, prevEnd - prevStart + 1);
  arr.splice(prevStart, 0, ...chunkMe);
  arr.splice(prevStart + chunkMe.length, 0, ...chunkPrev);
}

/** 下移：找下一个同层级兄弟（可能是块），整段与当前段交换；当前若是块则含子级 */
function moveDown(index: number) {
  const arr = items.value;
  const rangeEnd = moveRangeEnd(index);
  const nextStart = nextSiblingStart(rangeEnd);
  if (nextStart < 0) return;
  // 仅允许在同一父块（或同为顶层）内移动
  const parentCur = getBlockParentIndex(index);
  const parentNext = getBlockParentIndex(nextStart);
  if (parentCur !== parentNext || arr[nextStart].indent !== arr[index].indent) return;
  const nextRow = arr[nextStart];
  const nextEnd = blockTypes.includes(nextRow.type) ? blockEndIndex(nextStart) : nextStart;
  const count = rangeEnd - index + 1;
  const chunk = arr.splice(index, count);
  arr.splice(nextEnd - count + 1, 0, ...chunk);
}

function remove(index: number) {
  items.value.splice(index, 1);
}

/** 计算某块（if/for/while）当前 body 的结束位置 */
function blockEndIndex(parentIndex: number): number {
  const arr = items.value;
  const parent = arr[parentIndex];
  if (!parent) return parentIndex;
  const baseIndent = parent.indent;
  let last = parentIndex;
  for (let i = parentIndex + 1; i < arr.length; i++) {
    const ind = arr[i].indent;
    if (ind <= baseIndent) break;
    last = i;
  }
  return last;
}

const blockTypes: ScriptItemType[] = ["if", "for", "while"];

/** 当前行所在块的父步骤索引（若在 if/for/while 块内），否则 -1 */
function getBlockParentIndex(idx: number): number {
  const arr = items.value;
  const item = arr[idx];
  if (!item || item.indent === 0) return -1;
  const wantIndent = item.indent - 1;
  for (let j = idx - 1; j >= 0; j--) {
    const p = arr[j];
    if (p.indent === wantIndent && blockTypes.includes(p.type)) return j;
  }
  return -1;
}

/** 当前行是否为某块的「块尾」：仅当该行是该块的真正最后一行（用 blockEndIndex 判断）时才显示「在此添加子步骤」 */
function isLastInBlock(idx: number): boolean {
  const arr = items.value;
  const item = arr[idx];

  if (blockTypes.includes(item.type)) {
    return blockEndIndex(idx) === idx;
  }
  const parentIdx = getBlockParentIndex(idx);
  if (parentIdx < 0) return false;
  return blockEndIndex(parentIdx) === idx;
}

/** 在块语句（if/for/while）下添加子步骤，自动放到 body 尾部，并设置缩进为 parent.indent + 1 */
function addChild(parentIndex: number, type: ScriptItemType) {
  const arr = items.value;
  const parent = arr[parentIndex];
  if (!parent) return;
  const insertPos = blockEndIndex(parentIndex) + 1;
  const child = createItem(type, parent.indent + 1);
  arr.splice(insertPos, 0, child);
}

function handleAddChild(parentIndex: number, raw: string) {
  if (!raw) return;
  const t = raw as ScriptItemType;
  addChild(parentIndex, t);
}

function handleAddRoot(raw: string) {
  if (!raw) return;
  addRoot(raw as ScriptItemType);
}

/** 在当前行后面插入一个与当前行（或块）匹配层级的新步骤 */
function handleInsertAfter(index: number, raw: string) {
  if (!raw) return;
  const arr = items.value;
  const cur = arr[index];
  if (!cur) return;

  let indent = cur.indent;
  // 若是块语句，则默认在块体内插入第一条子步骤
  if (blockTypes.includes(cur.type)) {
    indent = cur.indent + 1;
  }

  const item = createItem(raw as ScriptItemType, indent);
  arr.splice(index + 1, 0, item);
}

function clearScript() {
  items.value = [];
}

function openSpeakParams(idx: number) {
  const it = items.value[idx];
  if (!it || it.type !== "speak") return;
  const role = it.role ?? "";
  currentSpeakRoleName.value = role;
  currentSpeakProvider.value = role ? roleProviderMap.value[role] ?? "" : "";
  const params = it.speakParams ?? {};
  const entries = Object.entries(params).filter(
    ([k]) => k && k.trim().length > 0,
  );
  // 至少保留一行空输入，方便直接键入
  const list =
    entries.length > 0
      ? entries.map(([key, value]) => ({ key, value: String(value) }))
      : [{ key: "", value: "" }];
  speakParamsDraft.value = list;
  speakParamsEditingIndex.value = idx;
}

function addSpeakParamRow() {
  speakParamsDraft.value.push({ key: "", value: "" });
}

function removeSpeakParamRow(i: number) {
  speakParamsDraft.value.splice(i, 1);
  if (speakParamsDraft.value.length === 0) {
    speakParamsDraft.value.push({ key: "", value: "" });
  }
}

function ensureSpeakParamKey(key: string) {
  const k = key.trim();
  if (!k) return;
  const exists = speakParamsDraft.value.some((row) => row.key === k);
  if (!exists) {
    speakParamsDraft.value.push({ key: k, value: "" });
  }
}

function applySpeakParams() {
  const idx = speakParamsEditingIndex.value;
  if (idx == null) return;
  const it = items.value[idx];
  if (!it || it.type !== "speak") {
    speakParamsEditingIndex.value = null;
    return;
  }
  const obj: Record<string, string> = {};
  for (const { key, value } of speakParamsDraft.value) {
    const k = key.trim();
    if (!k) continue;
    obj[k] = String(value ?? "");
  }
  it.speakParams = obj;
  speakParamsEditingIndex.value = null;
}

function cancelSpeakParams() {
  speakParamsEditingIndex.value = null;
}

/** 运行剧本（编辑模式用 config+items 生成 .vox，Code 模式直接用 codeText） */
const runError = ref<string | null>(null);
const isRunning = ref(false);
const isPaused = ref(false);
/** 是否循环运行整个剧本（默认开启） */
const loopRun = ref(true);
async function runScript() {
  runError.value = null;
  isRunning.value = true;
  isPaused.value = false;
  activeSourceIndex.value = null;
  try {
    const loopFlag = loopRun.value;
    if (mode.value === "code") {
      const voxText = codeText.value;
      await invoke("run_script", { voxText, loopRun: loopFlag });
    } else {
      const cfg = await invoke<AppConfig>("get_config");
      const voxText = toVox(cfg, items.value);
      // 用解析结果规范化 items，并为 speak/sleep 等语句附加 sourceIndex，便于运行时高亮
      const parsed = await invoke<ScriptItem[]>("parse_vox_to_script", {
        voxText,
      });
      items.value = parsed;
      await invoke("run_script", { voxText, loopRun: loopFlag });
    }
  } catch (e) {
    runError.value = String(e);
  } finally {
    isRunning.value = false;
    isPaused.value = false;
  }
}

async function pauseScript() {
  try {
    await invoke("pause_script");
    isPaused.value = true;
  } catch (e) {
    runError.value = String(e);
  }
}

async function resumeScript() {
  try {
    await invoke("resume_script");
    isPaused.value = false;
  } catch (e) {
    runError.value = String(e);
  }
}

async function stopScript() {
  try {
    await invoke("stop_script");
  } catch (e) {
    runError.value = String(e);
  } finally {
    isRunning.value = false;
    isPaused.value = false;
  }
}

function labelOfType(t: ScriptItemType): string {
  switch (t) {
    case "speak":
      return "说话";
    case "sleep":
      return "等待";
    case "if":
      return "如果";
    case "for":
      return "循环";
    case "while":
      return "当";
    case "let":
      return "定义";
    case "set":
      return "赋值";
    case "bgm_play":
      return "BGM 播放";
    case "bgm_volume":
      return "BGM 音量";
    case "bgm_pause":
      return "BGM 暂停";
    case "bgm_resume":
      return "BGM 恢复";
    case "bgm_stop":
      return "BGM 停止";
  }
}

/** 自动保存草稿：防抖 800ms */
let saveDraftTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleSaveDraft() {
  if (saveDraftTimer) clearTimeout(saveDraftTimer);
  saveDraftTimer = setTimeout(async () => {
    saveDraftTimer = null;
    try {
      await invoke("save_script_draft", { json: JSON.stringify(items.value) });
    } catch (e) {
      console.error("auto-save draft failed", e);
    }
  }, 800);
}

async function loadScriptDraft() {
  try {
    const json = await invoke<string>("get_script_draft");
    const list = JSON.parse(json || "[]") as ScriptItem[];
    if (Array.isArray(list) && list.length > 0) {
      items.value = list;
    }
  } catch (e) {
    console.error("load draft failed", e);
  }
}

let unlistenProgress: (() => void) | null = null;
let unlistenFinished: (() => void) | null = null;

onMounted(async () => {
  // 初始加载：角色列表 + 剧本草稿
  loadRoles();
  loadRoleProviderMap();
  loadScriptDraft();

  try {
    // 运行进度事件
    unlistenProgress = await listen<number>("script-progress", (event) => {
      console.log("script-progress event", event.payload);
      if (typeof event.payload === "number") {
        activeSourceIndex.value = event.payload;
        const rowIdx = items.value.findIndex(
          (it) => it.sourceIndex === event.payload,
        );
        activeRowIndex.value = rowIdx >= 0 ? rowIdx : null;
      } else {
        activeSourceIndex.value = null;
        activeRowIndex.value = null;
      }
    });

    // 运行结束事件
    unlistenFinished = await listen("script-finished", () => {
      console.log("script-finished event");
      activeSourceIndex.value = null;
      activeRowIndex.value = null;
      isRunning.value = false;
      isPaused.value = false;
    });

    // 配置变更事件（例如在配置页新增/删除角色）
    await listen("config-changed", () => {
      console.log("config-changed event, reload roles");
      loadRoles();
      loadRoleProviderMap();
    });
  } catch (e) {
    console.error(
      "listen script-progress/script-finished/config-changed failed",
      e,
    );
  }
});

onBeforeUnmount(() => {
  if (unlistenProgress) unlistenProgress();
  if (unlistenFinished) unlistenFinished();
});

watch(items, () => scheduleSaveDraft(), { deep: true });
</script>

<template>
  <div class="script-view">
    <div class="script-toolbar">
      <span class="toolbar-label">模式：</span>
      <button
        type="button"
        class="btn-mode"
        :class="{ active: mode === 'edit' }"
        @click="mode = 'edit'"
      >
        编辑
      </button>
      <button
        type="button"
        class="btn-mode"
        :class="{ active: mode === 'code' }"
        @click="switchToCode"
      >
        Code
      </button>
      <span v-if="activeRowIndex !== null" class="run-progress">
        当前执行行：第 {{ activeRowIndex + 1 }} 行
      </span>
      <span class="toolbar-spacer"></span>
      <template v-if="mode === 'edit'">
        <label class="run-loop-toggle">
          <input
            type="checkbox"
            v-model="loopRun"
            :disabled="isRunning"
          />
          <span>循环</span>
        </label>
        <button
          type="button"
          class="btn-run"
          :disabled="isRunning"
          @click="runScript"
        >
          {{ isRunning ? "运行中…" : "运行" }}
        </button>
        <button
          v-if="isRunning"
          type="button"
          class="btn-run-secondary"
          @click="isPaused ? resumeScript() : pauseScript()"
        >
          {{ isPaused ? "继续" : "暂停" }}
        </button>
        <button
          v-if="isRunning"
          type="button"
          class="btn-run-secondary btn-run-stop"
          @click="stopScript"
        >
          中断
        </button>
        <span class="toolbar-spacer"></span>
        <button type="button" class="btn-file" @click="openScriptFile">打开</button>
        <button type="button" class="btn-file" @click="saveScriptAs">另存为</button>
        <button type="button" class="btn-clear" @click="clearScript">清空脚本</button>
      </template>
      <template v-else>
        <button
          type="button"
          class="btn-run"
          :disabled="isRunning"
          @click="runScript"
        >
          {{ isRunning ? "运行中…" : "运行" }}
        </button>
      </template>
    </div>

    <!-- 文件操作与运行错误提示 -->
    <p v-if="fileError" class="file-error">{{ fileError }}</p>
    <p v-if="runError" class="file-error">运行失败：{{ runError }}</p>

    <!-- Code 模式：全屏编辑框，返回编辑在右下角 -->
    <div v-if="mode === 'code'" class="code-mode-full">
      <p v-if="parseError" class="parse-error">解析失败：{{ parseError }}</p>
      <textarea
        v-model="codeText"
        class="code-textarea"
        placeholder="在此编辑 .vox 脚本…"
        spellcheck="false"
      ></textarea>
      <button type="button" class="btn-return-edit" @click="applyCodeAndSwitchToEdit">
        返回编辑
      </button>
    </div>

    <!-- 编辑模式：列表（空时也显示“在剧本末尾添加顶层步骤”） -->
    <div v-else class="script-list">
      <div v-if="items.length === 0" class="script-empty">
        还没有任何步骤，可使用下方下拉添加顶层步骤。
      </div>

      <template v-else v-for="(item, idx) in items" :key="item.id">
        <div
          class="script-row"
          :class="{
            'script-row-active':
              item.sourceIndex != null && item.sourceIndex === activeSourceIndex,
          }"
          :style="{ marginLeft: `${item.indent * 24}px` }"
        >
          <div class="script-row-main">
          <span class="badge" :class="'badge-' + item.type">{{ labelOfType(item.type) }}</span>

          <template v-if="item.type === 'speak'">
            <select v-model="item.role" class="input role-select">
              <option value="">选择角色</option>
              <option v-for="r in roleNames" :key="r" :value="r">{{ r }}</option>
            </select>
            <button
              type="button"
              class="btn-speak-params"
              @click="openSpeakParams(idx)"
            >
              参数
              <span
                v-if="
                  item.speakParams &&
                  Object.keys(item.speakParams).length
                "
              >
                *
              </span>
            </button>
            <input
              v-model="item.text"
              class="input text-input"
              placeholder="要说的内容…"
            />
            <div class="expr-helper">
              <span class="expr-helper-label">插入文本变量：</span>
              <select
                class="expr-helper-select"
                @change="(e) => {
                  const el = e.target as HTMLSelectElement;
                  if (el.value) {
                    item.text = (item.text || '') + '${' + el.value + '}';
                    el.value = '';
                  }
                }"
              >
                <option value="">选择变量</option>
                <option v-for="v in availableVars" :key="`speak-var-${v}`" :value="v">
                  {{ v }}
                </option>
              </select>
            </div>
          </template>

          <template v-else-if="item.type === 'sleep'">
            <input
              v-model.number="item.ms"
              type="number"
              min="0"
              class="input number-input"
            />
            <span class="label">ms</span>
          </template>

          <template v-else-if="item.type === 'if'">
            <ExprInput
              v-model="item.condition"
              :variables="availableVars"
              :builtins="builtinFunctions"
              placeholder="条件表达式，如 score >= 60"
            />
          </template>

          <template v-else-if="item.type === 'for'">
            <ExprInput
              v-model="item.times"
              :variables="availableVars"
              :builtins="builtinFunctions"
              placeholder="次数表达式，如 3 或 n + 1"
            />
            <span class="label">次</span>
          </template>

          <template v-else-if="item.type === 'while'">
            <ExprInput
              v-model="item.condition"
              :variables="availableVars"
              :builtins="builtinFunctions"
              placeholder="条件表达式，如 running"
            />
            <span class="label">时</span>
          </template>

          <template v-else-if="item.type === 'let'">
            <input
              v-model="item.varName"
              class="input var-input"
              placeholder="变量名"
            />
            <span class="label">=</span>
            <ExprInput
              v-model="item.expr"
              :variables="availableVars"
              :builtins="builtinFunctions"
              placeholder="表达式，如 1 或 score + 1"
            />
          </template>

          <template v-else-if="item.type === 'set'">
            <input
              v-model="item.varName"
              class="input var-input"
              placeholder="变量名"
            />
            <span class="label">=</span>
            <ExprInput
              v-model="item.expr"
              :variables="availableVars"
              :builtins="builtinFunctions"
              placeholder="表达式，如 x + 1"
            />
          </template>

          <template v-else-if="item.type === 'bgm_play'">
            <span class="icon-bgm icon-bgm-play">▶</span>
            <input
              v-model="item.bgmPath"
              class="input text-input"
              placeholder="BGM 文件路径或 URL"
            />
            <label class="label checkbox-label">
              <input v-model="item.bgmLoop" type="checkbox" />
              循环
            </label>
          </template>

          <template v-else-if="item.type === 'bgm_volume'">
            <span class="icon-bgm icon-bgm-volume">🔊</span>
            <input
              v-model.number="item.bgmVolume"
              type="number"
              min="0"
              max="1"
              step="0.05"
              class="input number-input"
            />
            <span class="label">音量 (0-1)</span>
          </template>

          <template v-else-if="item.type === 'bgm_pause'">
            <span class="icon-bgm icon-bgm-pause">⏸</span>
            <span class="label">暂停当前 BGM</span>
          </template>

          <template v-else-if="item.type === 'bgm_resume'">
            <span class="icon-bgm icon-bgm-resume">⏯</span>
            <span class="label">恢复当前 BGM</span>
          </template>

          <template v-else-if="item.type === 'bgm_stop'">
            <span class="icon-bgm icon-bgm-stop">⏹</span>
            <span class="label">停止当前 BGM</span>
          </template>
        </div>

        <div class="script-row-actions">
          <button type="button" class="btn-row" @click="moveUp(idx)">↑</button>
          <button type="button" class="btn-row" @click="moveDown(idx)">↓</button>
          <button type="button" class="btn-row btn-del" @click="remove(idx)">删</button>
          <select
            class="child-add-inline"
            @change="(e) => { const el = e.target as HTMLSelectElement; handleInsertAfter(idx, el.value); el.value = ''; }"
          >
            <option value="">+ 在此行后插入步骤</option>
            <option
              v-for="opt in childTypeOptions"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </option>
          </select>
        </div>
        </div>

        <!-- 每个块只在真正最后一行下方显示一个：+ 在此块末尾添加子步骤 -->
        <div
          v-if="isLastInBlock(idx)"
          class="block-add-row"
          :style="{ marginLeft: `${(blockTypes.includes(item.type) ? item.indent + 1 : item.indent) * 24}px` }"
        >
          <select
            class="child-add-inline"
            @change="(e) => { const el = e.target as HTMLSelectElement; handleAddChild(blockTypes.includes(item.type) ? idx : getBlockParentIndex(idx), el.value); el.value = ''; }"
          >
            <option value="">+ 在此块末尾添加子步骤</option>
            <option
              v-for="opt in childTypeOptions"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </option>
          </select>
        </div>
      </template>

      <!-- 最顶级：在整份剧本最后添加顶层步骤（空剧本时也显示） -->
      <div class="block-add-row root-add-row">
        <select
          class="child-add-inline"
          @change="(e) => { const el = e.target as HTMLSelectElement; handleAddRoot(el.value); el.value = ''; }"
        >
          <option value="">+ 在剧本末尾添加顶层步骤</option>
          <option
            v-for="opt in childTypeOptions"
            :key="opt.value"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </select>
      </div>
    </div>

    <!-- speak 行参数覆写弹窗 -->
    <div
      v-if="speakParamsEditingIndex !== null"
      class="dialog-backdrop"
    >
      <div class="dialog">
        <h3 class="dialog-title">本句角色参数覆写</h3>
        <p class="dialog-desc">
          仅影响当前 speak 行；留空的字段会继续使用角色默认参数。
        </p>
        <p v-if="currentSpeakRoleName" class="dialog-provider">
          角色：{{ currentSpeakRoleName }}
          <span v-if="currentSpeakProvider">
            （provider: {{ currentSpeakProvider }}）
          </span>
        </p>

        <div
          v-if="currentSpeakProvider === 'gpt_sovits_v2'"
          class="preset-params-row"
        >
          <span class="preset-label">GPT-SoVITS 常用字段：</span>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('language')"
          >
            language
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('text_lang')"
          >
            text_lang
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('ref_audio_path')"
          >
            ref_audio_path
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('prompt_text')"
          >
            prompt_text
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('speaker_id')"
          >
            speaker_id
          </button>
        </div>

        <div
          v-else-if="currentSpeakProvider === 'bert_vits2'"
          class="preset-params-row"
        >
          <span class="preset-label">Bert-VITS2 常用字段：</span>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('language')"
          >
            language
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('speaker_id')"
          >
            speaker_id
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('emotion')"
          >
            emotion
          </button>
          <button
            type="button"
            class="preset-chip"
            @click="ensureSpeakParamKey('length')"
          >
            length
          </button>
        </div>
        <div class="kv-list">
          <div
            v-for="(row, i) in speakParamsDraft"
            :key="i"
            class="kv-row"
          >
            <input
              v-model="row.key"
              class="kv-key"
              placeholder="参数名，如 language"
            />
            <input
              v-model="row.value"
              class="kv-value"
              placeholder='值，如 "zh" / "1.0"'
            />
            <button
              type="button"
              class="btn-del-small"
              @click="removeSpeakParamRow(i)"
            >
              删
            </button>
          </div>
        </div>
        <button
          type="button"
          class="btn-add-small"
          @click="addSpeakParamRow"
        >
          + 添加参数
        </button>
        <div class="dialog-actions">
          <button
            type="button"
            class="btn-primary"
            @click="applySpeakParams"
          >
            确定
          </button>
          <button
            type="button"
            class="btn-secondary"
            @click="cancelSpeakParams"
          >
            取消
          </button>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
.script-view {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  height: 100%;
  min-height: 0;
}

.script-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.toolbar-label {
  font-size: 0.875rem;
  color: #555;
}

.btn-add {
  padding: 0.25rem 0.6rem;
  font-size: 0.8rem;
  background: #e8e8e8;
  border: 1px solid #ccc;
  border-radius: 4px;
  cursor: pointer;
}

.btn-add:hover {
  background: #ddd;
}

.toolbar-spacer {
  flex: 1;
}

.btn-export {
  padding: 0.25rem 0.8rem;
  font-size: 0.8rem;
  background: #1a1a2e;
  color: #eee;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.btn-export:hover {
  background: #2a2a4e;
}

.btn-mode {
  padding: 0.25rem 0.6rem;
  font-size: 0.8rem;
  background: #f0f0f0;
  border: 1px solid #ccc;
  border-radius: 4px;
  cursor: pointer;
}

.btn-mode.active {
  background: #1a1a2e;
  color: #eee;
  border-color: #1a1a2e;
}

.btn-mode:hover:not(.active) {
  background: #e0e0e0;
}

.file-error {
  margin: 0 0 0.5rem;
  font-size: 0.85rem;
  color: #c94c4c;
}

.parse-error {
  margin: 0 0 0.5rem;
  font-size: 0.85rem;
  color: #c94c4c;
}

.btn-file {
  padding: 0.25rem 0.6rem;
  font-size: 0.8rem;
  background: #e8e8e8;
  border: 1px solid #ccc;
  border-radius: 4px;
  cursor: pointer;
}

.btn-file:hover {
  background: #ddd;
}

.btn-clear {
  padding: 0.25rem 0.6rem;
  font-size: 0.8rem;
  background: #fff;
  border: 1px solid #c94c4c;
  color: #c94c4c;
  border-radius: 4px;
  cursor: pointer;
}

.btn-clear:hover {
  background: #ffe0e0;
}

.btn-run {
  padding: 0.25rem 0.8rem;
  font-size: 0.8rem;
  background: #059669;
  color: #fff;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.btn-run:hover:not(:disabled) {
  background: #047857;
}

.btn-run:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.btn-run-secondary {
  padding: 0.25rem 0.6rem;
  font-size: 0.8rem;
  margin-left: 0.5rem;
  background: #fbbf24;
  color: #1f2933;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.btn-run-secondary:hover {
  background: #f59e0b;
}

.btn-run-stop {
  background: #ef4444;
  color: #fff;
}

.btn-run-stop:hover {
  background: #dc2626;
}

.run-loop-toggle {
  display: inline-flex;
  align-items: center;
  font-size: 0.8rem;
  margin-right: 0.5rem;
  gap: 0.25rem;
}

.run-loop-toggle input[type="checkbox"] {
  width: 14px;
  height: 14px;
}

.btn-speak-params {
  margin-left: 0.25rem;
  padding: 0.1rem 0.4rem;
  font-size: 0.7rem;
  border-radius: 999px;
  border: 1px solid #d4d4d8;
  background: #f9fafb;
  cursor: pointer;
}

.btn-speak-params span {
  color: #f97316;
}

.dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.35);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
}

.dialog {
  background: #ffffff;
  padding: 1rem 1.25rem;
  border-radius: 0.5rem;
  max-width: 520px;
  width: 100%;
  box-shadow: 0 20px 40px rgba(15, 23, 42, 0.25);
}

.dialog-title {
  margin: 0 0 0.35rem;
  font-size: 1rem;
}

.dialog-desc {
  margin: 0 0 0.75rem;
  font-size: 0.8rem;
  color: #6b7280;
}

.dialog-provider {
  margin: 0 0 0.5rem;
  font-size: 0.8rem;
  color: #374151;
}

.preset-params-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.25rem;
  margin: 0 0 0.75rem;
  font-size: 0.75rem;
}

.preset-label {
  color: #6b7280;
}

.preset-chip {
  padding: 0.1rem 0.5rem;
  border-radius: 999px;
  border: 1px solid #d1d5db;
  background: #f9fafb;
  font-size: 0.75rem;
  cursor: pointer;
}

.preset-chip:hover {
  background: #e5e7eb;
}

.kv-list {
  margin: 0 0 0.5rem;
}

.kv-row {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  margin-bottom: 0.25rem;
}

.kv-key,
.kv-value {
  flex: 1;
  padding: 0.2rem 0.4rem;
  font-size: 0.8rem;
  border-radius: 0.25rem;
  border: 1px solid #d4d4d8;
}

.btn-add-small {
  margin-top: 0.25rem;
  padding: 0.15rem 0.5rem;
  font-size: 0.75rem;
  border-radius: 999px;
  border: 1px solid #d1d5db;
  background: #f3f4f6;
  cursor: pointer;
}

.btn-add-small:hover {
  background: #e5e7eb;
}

.btn-del-small {
  padding: 0.15rem 0.4rem;
  font-size: 0.7rem;
  border-radius: 999px;
  border: 1px solid #fecaca;
  background: #fee2e2;
  color: #b91c1c;
  cursor: pointer;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  margin-top: 0.75rem;
}

.btn-primary {
  padding: 0.25rem 0.8rem;
  font-size: 0.8rem;
  border-radius: 0.375rem;
  border: none;
  background: #1d4ed8;
  color: #ffffff;
  cursor: pointer;
}

.btn-primary:hover {
  background: #1e40af;
}

.btn-secondary {
  padding: 0.25rem 0.8rem;
  font-size: 0.8rem;
  border-radius: 0.375rem;
  border: 1px solid #d1d5db;
  background: #ffffff;
  color: #374151;
  cursor: pointer;
}

.btn-secondary:hover {
  background: #f3f4f6;
}

.script-row-active {
  background: rgba(59, 130, 246, 0.16);
  box-shadow: inset 3px 0 0 #3b82f6;
}

.run-progress {
  margin-left: 1rem;
  font-size: 0.8rem;
  color: #2563eb;
}

.code-mode-full {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  position: relative;
}

.code-mode-full .code-textarea {
  flex: 1;
  min-height: 0;
  width: 100%;
  resize: none;
  box-sizing: border-box;
}

.code-textarea {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
    "Courier New", monospace;
  font-size: 0.8rem;
  padding: 0.4rem 0.5rem;
  border-radius: 4px;
  border: 1px solid #ccc;
}

.btn-return-edit {
  position: absolute;
  right: 1rem;
  bottom: 1rem;
  padding: 0.4rem 1rem;
  font-size: 0.875rem;
  background: #1a1a2e;
  color: #eee;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.btn-return-edit:hover {
  background: #2a2a4e;
}

.script-empty {
  font-size: 0.875rem;
  color: #777;
}

.script-list {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.script-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.35rem 0.5rem;
  border-radius: 4px;
  background: #ffffff;
  border: 1px solid #e0e0e0;
}

.script-row-main {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex: 1;
  min-width: 0;
}

.badge {
  padding: 0.1rem 0.4rem;
  border-radius: 999px;
  font-size: 0.75rem;
  color: #fff;
  flex-shrink: 0;
}

.badge-speak {
  background: #2563eb;
}

.badge-sleep {
  background: #64748b;
}

.badge-if {
  background: #059669;
}

.badge-for {
  background: #d97706;
}

.badge-while {
  background: #7c3aed;
}

.badge-let {
  background: #0d9488;
}

.badge-set {
  background: #dc2626;
}

.badge-bgm_play,
.badge-bgm_volume,
.badge-bgm_pause,
.badge-bgm_resume,
.badge-bgm_stop {
  background: #0f766e;
}

.label {
  font-size: 0.8rem;
  color: #555;
}

.input {
  padding: 0.2rem 0.4rem;
  border-radius: 4px;
  border: 1px solid #ccc;
  font-size: 0.8rem;
}

.role-select {
  min-width: 7rem;
}

.text-input {
  min-width: 12rem;
  flex: 1;
}

.var-input {
  width: 6rem;
}

.number-input {
  width: 5rem;
}

.expr-helper {
  margin-top: 0.2rem;
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  align-items: center;
}

.expr-helper-label {
  font-size: 0.75rem;
  color: #6b7280;
}

.expr-helper-select {
  padding: 0.1rem 0.35rem;
  font-size: 0.7rem;
  border-radius: 999px;
  border: 1px solid #d4d4d8;
  background: #f9fafb;
}

.icon-bgm {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.25rem;
  height: 1.25rem;
  margin-right: 0.35rem;
  font-size: 0.75rem;
  border-radius: 999px;
  background: #e0f2fe;
  color: #0369a1;
}

.child-add {
  margin-left: 0.5rem;
  padding: 0.2rem 0.4rem;
  font-size: 0.75rem;
}

.block-add-row {
  margin-top: 0.25rem;
  margin-bottom: 0.15rem;
  padding: 0.2rem 0;
}

.root-add-row {
  margin-top: 0.5rem;
  padding-top: 0.35rem;
  border-top: 1px dashed #ddd;
}

.child-add-inline {
  padding: 0.2rem 0.5rem;
  font-size: 0.75rem;
  color: #666;
  border: 1px dashed #bbb;
  border-radius: 4px;
  background: #fafafa;
  cursor: pointer;
}

.child-add-inline:hover {
  border-color: #1a1a2e;
  color: #1a1a2e;
}

.script-row-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.btn-row {
  padding: 0.15rem 0.4rem;
  font-size: 0.7rem;
  border-radius: 4px;
  border: 1px solid #ccc;
  background: #f5f5f5;
  cursor: pointer;
}

.btn-row:hover {
  background: #e5e5e5;
}

.btn-del {
  color: #c94c4c;
  border-color: #c94c4c;
}

.btn-del:hover {
  background: #ffe0e0;
}

</style>

