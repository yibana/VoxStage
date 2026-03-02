<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppConfig } from "../types/config";
import { emptyModel, emptyRole } from "../types/config";

const config = ref<AppConfig>({ models: [], roles: [] });
const saveStatus = ref<"" | "ok" | "err">("");
const saveMessage = ref("");

/** 模型 type / provider 选项（常用值） */
const modelTypeOptions = ["http"];
const providerOptions = ["gpt_sovits_v2", "bert_vits2"];

/** 每个角色 params 的编辑模式：键值对 或 JSON */
const roleParamsMode = ref<("kv" | "json")[]>([]);

function ensureRoleParamsMode() {
  while (roleParamsMode.value.length < config.value.roles.length) {
    roleParamsMode.value.push("kv");
  }
  roleParamsMode.value.splice(config.value.roles.length);
}

async function load() {
  try {
    config.value = await invoke<AppConfig>("get_config");
  } catch (e) {
    console.error("load config failed", e);
    config.value = { models: [], roles: [] };
  }
}

async function save() {
  saveStatus.value = "";
  saveMessage.value = "";
  try {
    await invoke("save_config", { config: config.value });
    saveStatus.value = "ok";
    saveMessage.value = "已保存";
  } catch (e) {
    saveStatus.value = "err";
    saveMessage.value = String(e);
  }
}

function addModel() {
  config.value.models.push(emptyModel());
}

function removeModel(i: number) {
  config.value.models.splice(i, 1);
}

function addRole() {
  config.value.roles.push(emptyRole());
  ensureRoleParamsMode();
}

function removeRole(i: number) {
  config.value.roles.splice(i, 1);
  ensureRoleParamsMode();
}

function setRoleParams(i: number, jsonStr: string) {
  try {
    const parsed = JSON.parse(jsonStr || "{}");
    if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
      const entries = Object.entries(parsed).filter(
        (e): e is [string, string] =>
          typeof e[0] === "string" && typeof e[1] === "string" && e[0].trim() !== ""
      );
      config.value.roles[i].params = Object.fromEntries(entries);
    }
  } catch {
    // 保持原样
  }
}

/** 键值对：获取某角色 params 的条目列表 */
function getParamsEntries(roleIndex: number): { key: string; value: string }[] {
  const r = config.value.roles[roleIndex];
  if (!r) return [];
  return Object.entries(r.params).map(([key, value]) => ({ key, value: String(value) }));
}

function updateParamKey(roleIndex: number, entryIndex: number, newKey: string) {
  const entries = getParamsEntries(roleIndex);
  if (entryIndex >= entries.length) return;
  entries[entryIndex].key = newKey;
  config.value.roles[roleIndex].params = Object.fromEntries(
    entries.filter((e) => e.key.trim() !== "").map((e) => [e.key.trim(), e.value])
  );
  // 保留空 key 的占位行，便于继续输入
  if (entries.some((e) => e.key === "")) {
    config.value.roles[roleIndex].params[""] = entries.find((e) => e.key === "")?.value ?? "";
  }
}

function updateParamValue(roleIndex: number, entryIndex: number, newValue: string) {
  const entries = getParamsEntries(roleIndex);
  if (entryIndex >= entries.length) return;
  entries[entryIndex].value = newValue;
  config.value.roles[roleIndex].params = Object.fromEntries(
    entries.filter((e) => e.key.trim() !== "").map((e) => [e.key.trim(), e.value])
  );
  const emptyEntry = entries.find((e) => e.key === "");
  if (emptyEntry) config.value.roles[roleIndex].params[""] = emptyEntry.value;
}

function addParamRow(roleIndex: number) {
  const r = config.value.roles[roleIndex];
  if (!r) return;
  r.params[""] = "";
}

function removeParamRow(roleIndex: number, entryIndex: number) {
  const entries = getParamsEntries(roleIndex);
  if (entryIndex >= entries.length) return;
  const keyToRemove = entries[entryIndex].key;
  const r = config.value.roles[roleIndex];
  if (keyToRemove in r.params) delete r.params[keyToRemove];
}

/** 用于角色下拉的模型名列表 */
const modelNames = () => config.value.models.map((m) => m.name);

onMounted(() => {
  load().then(() => ensureRoleParamsMode());
});
</script>

<template>
  <div class="config-view">
    <div class="config-toolbar">
      <button type="button" class="btn-save" @click="save">保存配置</button>
      <span v-if="saveStatus === 'ok'" class="status-ok">{{ saveMessage }}</span>
      <span v-else-if="saveStatus === 'err'" class="status-err">{{ saveMessage }}</span>
    </div>

    <section class="config-section">
      <h2 class="section-title">模型</h2>
      <button type="button" class="btn-add" @click="addModel">+ 添加模型</button>
      <div class="list">
        <div
          v-for="(m, i) in config.models"
          :key="i"
          class="list-item card"
        >
          <div class="item-row">
            <label>名称</label>
            <input v-model="m.name" placeholder="如 gpt_sovits_v2" />
          </div>
          <div class="item-row">
            <label>type</label>
            <select v-model="m.type">
              <option value="">请选择 type</option>
              <option v-for="t in modelTypeOptions" :key="t" :value="t">
                {{ t }}
              </option>
            </select>
          </div>
          <div class="item-row">
            <label>provider</label>
            <select v-model="m.provider">
              <option value="">请选择 provider</option>
              <option v-for="p in providerOptions" :key="p" :value="p">
                {{ p }}
              </option>
            </select>
          </div>
          <div class="item-row">
            <label>endpoint</label>
            <input v-model="m.endpoint" placeholder="http://127.0.0.1:9880" />
          </div>
          <div class="item-row">
            <label>model_id</label>
            <input v-model="m.model_id" placeholder="可选" />
          </div>
          <button type="button" class="btn-del" @click="removeModel(i)">删除</button>
        </div>
      </div>
    </section>

    <section class="config-section">
      <h2 class="section-title">角色</h2>
      <button type="button" class="btn-add" @click="addRole">+ 添加角色</button>
      <div class="list">
        <div
          v-for="(r, i) in config.roles"
          :key="i"
          class="list-item card"
        >
          <div class="item-row">
            <label>名称</label>
            <input v-model="r.name" placeholder="如 Narrator" />
          </div>
          <div class="item-row">
            <label>模型</label>
            <select v-model="r.model">
              <option value="">请选择模型</option>
              <option v-for="name in modelNames()" :key="name" :value="name">{{ name }}</option>
            </select>
          </div>
          <div class="item-row item-row-full params-block">
            <div class="params-label-row">
              <label>params</label>
              <span class="params-mode-tabs">
                <button
                  type="button"
                  class="mode-tab"
                  :class="{ active: (roleParamsMode[i] ?? 'kv') === 'kv' }"
                  @click="ensureRoleParamsMode(); roleParamsMode[i] = 'kv'"
                >
                  键值对
                </button>
                <button
                  type="button"
                  class="mode-tab"
                  :class="{ active: (roleParamsMode[i] ?? 'kv') === 'json' }"
                  @click="ensureRoleParamsMode(); roleParamsMode[i] = 'json'"
                >
                  JSON
                </button>
              </span>
            </div>
            <template v-if="(roleParamsMode[i] ?? 'kv') === 'kv'">
              <div class="kv-list">
                <div
                  v-for="(entry, pIdx) in getParamsEntries(i)"
                  :key="pIdx"
                  class="kv-row"
                >
                  <input
                    :value="entry.key"
                    @input="updateParamKey(i, pIdx, (($event.target as HTMLInputElement).value))"
                    placeholder="参数名"
                    class="kv-key"
                  />
                  <input
                    :value="entry.value"
                    @input="updateParamValue(i, pIdx, (($event.target as HTMLInputElement).value))"
                    placeholder="值"
                    class="kv-value"
                  />
                  <button type="button" class="btn-del-small" @click="removeParamRow(i, pIdx)">删</button>
                </div>
              </div>
              <button type="button" class="btn-add-small" @click="addParamRow(i)">+ 添加参数</button>
            </template>
            <template v-else>
              <textarea
                :value="JSON.stringify(r.params, null, 2)"
                @input="(e) => setRoleParams(i, (e.target as HTMLTextAreaElement).value)"
                placeholder='{"speed":"1.0","language":"zh"}'
                class="input-params"
                rows="4"
              />
            </template>
          </div>
          <button type="button" class="btn-del" @click="removeRole(i)">删除</button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.config-view {
  padding: 0 0 2rem 0;
}

.config-toolbar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 1.5rem;
}

.btn-save {
  padding: 0.4rem 0.8rem;
  background: #1a1a2e;
  color: #eee;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.875rem;
}

.btn-save:hover {
  background: #2a2a4e;
}

.status-ok { color: #0d7377; font-size: 0.875rem; }
.status-err { color: #c94c4c; font-size: 0.875rem; }

.config-section {
  margin-bottom: 2rem;
}

.section-title {
  font-size: 1rem;
  margin: 0 0 0.5rem 0;
  color: #333;
}

.btn-add {
  margin-bottom: 0.75rem;
  padding: 0.35rem 0.7rem;
  background: #e8e8e8;
  border: 1px solid #ccc;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.8rem;
}

.btn-add:hover {
  background: #ddd;
}

.list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.list-item.card {
  padding: 0.75rem 1rem;
  background: #fff;
  border: 1px solid #e0e0e0;
  border-radius: 8px;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem 1rem;
}

.item-row {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.item-row label {
  min-width: 4.5rem;
  font-size: 0.8rem;
  color: #555;
}

.item-row input,
.item-row select {
  padding: 0.25rem 0.4rem;
  border: 1px solid #ccc;
  border-radius: 4px;
  font-size: 0.875rem;
  min-width: 10rem;
}

.item-row-full { width: 100%; }
.item-row-full textarea.input-params {
  min-width: 100%;
  resize: vertical;
}

.input-params {
  min-width: 14rem;
}

.params-block {
  flex-direction: column;
  align-items: stretch;
  gap: 0.35rem;
}

.params-label-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.params-label-row label {
  margin: 0;
}

.params-mode-tabs {
  display: flex;
  gap: 0.2rem;
}

.mode-tab {
  padding: 0.2rem 0.5rem;
  font-size: 0.75rem;
  background: #e8e8e8;
  border: 1px solid #ccc;
  border-radius: 4px;
  cursor: pointer;
  color: #555;
}

.mode-tab:hover {
  background: #ddd;
}

.mode-tab.active {
  background: #1a1a2e;
  color: #eee;
  border-color: #1a1a2e;
}

.kv-list {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.kv-row {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.kv-key {
  width: 8rem;
  padding: 0.25rem 0.4rem;
  border: 1px solid #ccc;
  border-radius: 4px;
  font-size: 0.8rem;
}

.kv-value {
  flex: 1;
  min-width: 6rem;
  padding: 0.25rem 0.4rem;
  border: 1px solid #ccc;
  border-radius: 4px;
  font-size: 0.8rem;
}

.btn-add-small {
  margin-top: 0.25rem;
  padding: 0.2rem 0.5rem;
  font-size: 0.75rem;
  background: #e8e8e8;
  border: 1px solid #ccc;
  border-radius: 4px;
  cursor: pointer;
  align-self: flex-start;
}

.btn-add-small:hover {
  background: #ddd;
}

.btn-del-small {
  padding: 0.15rem 0.35rem;
  font-size: 0.7rem;
  color: #c94c4c;
  background: transparent;
  border: 1px solid #c94c4c;
  border-radius: 3px;
  cursor: pointer;
  flex-shrink: 0;
}

.btn-del-small:hover {
  background: #ffe0e0;
}

.btn-del {
  padding: 0.2rem 0.5rem;
  font-size: 0.75rem;
  color: #c94c4c;
  background: transparent;
  border: 1px solid #c94c4c;
  border-radius: 4px;
  cursor: pointer;
  margin-left: auto;
}

.btn-del:hover {
  background: #ffe0e0;
}
</style>
