<script setup lang="ts">
import { ref, onMounted, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { ScriptItemType, ScriptItem } from "../types/script";
import type { RoleEntry, AppConfig } from "../types/config";
import { createItem, toVox } from "../types/script";

const items = ref<ScriptItem[]>([]);
const roleNames = ref<string[]>([]);

/** 编辑 | Code 双模式 */
const mode = ref<"edit" | "code">("edit");
const codeText = ref("");
const parseError = ref<string | null>(null);

/** 可作为子步骤添加的类型及其标签 */
const childTypeOptions: { value: ScriptItemType; label: string }[] = [
  { value: "speak", label: "说话" },
  { value: "sleep", label: "等待" },
  { value: "let", label: "定义变量" },
  { value: "set", label: "设置变量" },
  { value: "if", label: "如果" },
  { value: "for", label: "循环" },
  { value: "while", label: "当" },
];

async function loadRoles() {
  try {
    const roles = await invoke<RoleEntry[]>("get_roles");
    roleNames.value = roles.map((r) => r.name);
  } catch (e) {
    console.error("load roles failed", e);
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

/** 从 index 往前找上一个同层级兄弟的起始下标（含块则取块头） */
function prevSiblingStart(index: number): number {
  const arr = items.value;
  const cur = arr[index];
  if (!cur) return -1;
  for (let j = index - 1; j >= 0; j--) {
    if (arr[j].indent === cur.indent) return j;
  }
  return -1;
}

/** 从 rangeEnd+1 往后找下一个同层级兄弟的起始下标（含块则取块头） */
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
  const cur = arr[index];
  const prevStart = prevSiblingStart(index);
  if (prevStart < 0) return;
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
  const cur = arr[index];
  const rangeEnd = moveRangeEnd(index);
  const nextStart = nextSiblingStart(rangeEnd);
  if (nextStart < 0) return;
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

function clearScript() {
  items.value = [];
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

onMounted(() => {
  loadRoles();
  loadScriptDraft();
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
      <span class="toolbar-spacer"></span>
      <template v-if="mode === 'edit'">
        <span class="toolbar-spacer"></span>
        <button type="button" class="btn-file" @click="openScriptFile">打开</button>
        <button type="button" class="btn-file" @click="saveScriptAs">另存为</button>
        <button type="button" class="btn-clear" @click="clearScript">清空脚本</button>
      </template>
    </div>

    <!-- 文件操作错误提示（打开/另存为） -->
    <p v-if="fileError" class="file-error">{{ fileError }}</p>

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
          :style="{ marginLeft: `${item.indent * 24}px` }"
        >
          <div class="script-row-main">
          <span class="badge" :class="'badge-' + item.type">{{ labelOfType(item.type) }}</span>

          <template v-if="item.type === 'speak'">
            <select v-model="item.role" class="input role-select">
              <option value="">选择角色</option>
              <option v-for="r in roleNames" :key="r" :value="r">{{ r }}</option>
            </select>
            <input
              v-model="item.text"
              class="input text-input"
              placeholder="要说的内容…"
            />
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
            <input
              v-model="item.condition"
              class="input text-input"
              placeholder="条件表达式，如 score &gt;= 60"
            />
          </template>

          <template v-else-if="item.type === 'for'">
            <input
              v-model="item.times"
              class="input number-input"
              placeholder="次数表达式，如 3 或 n + 1"
            />
            <span class="label">次</span>
          </template>

          <template v-else-if="item.type === 'while'">
            <input
              v-model="item.condition"
              class="input text-input"
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
            <input
              v-model="item.expr"
              class="input text-input"
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
            <input
              v-model="item.expr"
              class="input text-input"
              placeholder="表达式，如 x + 1"
            />
          </template>
        </div>

        <div class="script-row-actions">
          <button type="button" class="btn-row" @click="moveUp(idx)">↑</button>
          <button type="button" class="btn-row" @click="moveDown(idx)">↓</button>
          <button type="button" class="btn-row btn-del" @click="remove(idx)">删</button>
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

