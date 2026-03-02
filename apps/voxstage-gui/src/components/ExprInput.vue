<script setup lang="ts">
import { ref, watch, nextTick, computed } from "vue";

interface BuiltinFn {
  name: string;
  snippet: string;
  desc?: string;
}

const props = defineProps<{
  modelValue: string;
  variables?: string[];
  builtins?: BuiltinFn[];
  placeholder?: string;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

const inputRef = ref<HTMLInputElement | null>(null);
const error = ref<string | null>(null);

type SuggestItem =
  | { kind: "var"; label: string; insertText: string }
  | { kind: "fn"; label: string; insertText: string; desc?: string };

const suggestions = ref<SuggestItem[]>([]);
const showSuggestions = ref(false);
const activeSuggestIndex = ref(0);
const currentTokenStart = ref<number | null>(null);

const displaySize = computed(() => {
  // 根据当前内容长度自适应字符宽度，但限制在一个合理区间内
  const base =
    (props.modelValue && props.modelValue.length) ||
    (props.placeholder && props.placeholder.length) ||
    0;
  const min = 18;
  const max = 60;
  return Math.min(max, Math.max(min, base + 2));
});

function updateValue(value: string) {
  emit("update:modelValue", value);
}

function validateExpression(raw: string): string | null {
  const s = (raw ?? "").trim();
  if (!s) return null;

  // 1. 检查字符串引号是否闭合（支持 \" 与 \\ 转义）
  let inString = false;
  let escape = false;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!;
    if (escape) {
      escape = false;
      continue;
    }
    if (ch === "\\") {
      escape = true;
      continue;
    }
    if (ch === '"') {
      inString = !inString;
    }
  }
  if (inString) {
    return "字符串引号未闭合";
  }

  // 2. 检查圆括号配对
  let depth = 0;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!;
    if (ch === "(") depth++;
    else if (ch === ")") {
      depth--;
      if (depth < 0) return "右括号多余";
    }
  }
  if (depth > 0) {
    return "左括号未闭合";
  }

  // 3. 明显的运算符错误（结尾是运算符等）
  if (/(\&\&|\|\||[+\-*/%])\s*$/.test(s)) {
    return "表达式结尾不能是运算符";
  }
  if (/^\s*(\&\&|\|\||[*/%])/.test(s)) {
    return "表达式不能以该运算符开头";
  }

  return null;
}

function insertAtCursor(snippet: string) {
  const el = inputRef.value;
  if (!el) {
    updateValue((props.modelValue || "") + snippet);
    return;
  }
  const start = el.selectionStart ?? props.modelValue.length;
  const end = el.selectionEnd ?? props.modelValue.length;
  const current = props.modelValue || "";
  const next = current.slice(0, start) + snippet + current.slice(end);
  updateValue(next);
  nextTick(() => {
    const pos = start + snippet.length;
    el.setSelectionRange(pos, pos);
    el.focus();
  });
}

function onInput(e: Event) {
  const el = e.target as HTMLInputElement;
  updateValue(el.value);
  updateSuggestions(el);
}

function updateSuggestions(el: HTMLInputElement) {
  const value = el.value || "";
  const cursor = el.selectionStart ?? value.length;
  // 仅在没有选区且光标在行内时做简单前缀联想
  if (el.selectionStart !== el.selectionEnd) {
    showSuggestions.value = false;
    suggestions.value = [];
    return;
  }

  // 向前回溯获取当前 token（字母/数字/下划线）
  let start = cursor;
  const isWordChar = (ch: string) => /[A-Za-z0-9_]/.test(ch);
  while (start > 0 && isWordChar(value[start - 1]!)) {
    start--;
  }
  const token = value.slice(start, cursor);
  currentTokenStart.value = start;

  if (!token) {
    showSuggestions.value = false;
    suggestions.value = [];
    return;
  }

  const lower = token.toLowerCase();
  const list: SuggestItem[] = [];

  if (props.variables) {
    for (const v of props.variables) {
      if (v.toLowerCase().startsWith(lower)) {
        list.push({ kind: "var", label: v, insertText: v });
      }
    }
  }

  if (props.builtins) {
    for (const fn of props.builtins) {
      if (fn.name.toLowerCase().startsWith(lower)) {
        list.push({
          kind: "fn",
          label: fn.name,
          insertText: fn.snippet,
          desc: fn.desc,
        });
      }
    }
  }

  if (list.length === 0) {
    showSuggestions.value = false;
    suggestions.value = [];
    return;
  }

  suggestions.value = list.slice(0, 10);
  activeSuggestIndex.value = 0;
  showSuggestions.value = true;
}

function applySuggestion(item: SuggestItem) {
  const el = inputRef.value;
  const value = props.modelValue || "";
  if (!el || currentTokenStart.value === null) {
    updateValue(value + item.insertText);
    return;
  }
  const cursor = el.selectionStart ?? value.length;
  const start = currentTokenStart.value;
  const end = cursor;
  const next =
    value.slice(0, start) + item.insertText + value.slice(end);
  updateValue(next);
  nextTick(() => {
    const pos = start + item.insertText.length;
    el.setSelectionRange(pos, pos);
    el.focus();
  });
  showSuggestions.value = false;
}

function onKeyDown(e: KeyboardEvent) {
  if (!showSuggestions.value || suggestions.value.length === 0) return;

  if (e.key === "ArrowDown") {
    e.preventDefault();
    activeSuggestIndex.value =
      (activeSuggestIndex.value + 1) % suggestions.value.length;
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    activeSuggestIndex.value =
      (activeSuggestIndex.value - 1 + suggestions.value.length) %
      suggestions.value.length;
  } else if (e.key === "Enter" || e.key === "Tab") {
    e.preventDefault();
    const item = suggestions.value[activeSuggestIndex.value];
    if (item) applySuggestion(item);
  } else if (e.key === "Escape") {
    e.preventDefault();
    showSuggestions.value = false;
  }
}

function onVarChange(e: Event) {
  const el = e.target as HTMLSelectElement;
  if (el.value) {
    insertAtCursor(el.value);
    el.value = "";
  }
}

function onBuiltinChange(e: Event) {
  const el = e.target as HTMLSelectElement;
  if (el.value && props.builtins) {
    const fn = props.builtins.find((b) => b.name === el.value);
    if (fn) {
      insertAtCursor(fn.snippet);
    }
    el.value = "";
  }
}

watch(
  () => props.modelValue,
  (val) => {
    error.value = validateExpression(val || "");
  },
  { immediate: true },
);
</script>

<template>
  <div class="expr-input">
    <div class="expr-main">
      <input
        ref="inputRef"
        :value="modelValue"
        type="text"
        class="expr-input-field"
        :class="{ 'expr-input-field-error': error }"
        :placeholder="placeholder"
        :size="displaySize"
        @input="onInput"
        @keydown="onKeyDown"
      />
      <div
        v-if="(variables && variables.length) || (builtins && builtins.length)"
        class="expr-helper"
      >
        <span class="expr-helper-label">插入：</span>
        <select
          v-if="variables && variables.length"
          class="expr-helper-select"
          @change="onVarChange"
        >
          <option value="">变量</option>
          <option v-for="v in variables" :key="v" :value="v">
            {{ v }}
          </option>
        </select>
        <select
          v-if="builtins && builtins.length"
          class="expr-helper-select"
          @change="onBuiltinChange"
        >
          <option value="">内置函数</option>
          <option
            v-for="fn in builtins"
            :key="fn.name"
            :value="fn.name"
            :title="fn.desc || fn.snippet"
          >
            {{ fn.name }}{{ fn.desc ? ` – ${fn.desc}` : "" }}
          </option>
        </select>
      </div>
      <ul
        v-if="showSuggestions && suggestions.length"
        class="expr-suggest-list"
      >
        <li
          v-for="(s, idx) in suggestions"
          :key="`${s.kind}-${s.label}-${idx}`"
          :class="['expr-suggest-item', { active: idx === activeSuggestIndex }]"
        >
          <span class="expr-suggest-kind">
            {{ s.kind === "var" ? "变量" : "函数" }}
          </span>
          <span class="expr-suggest-label">{{ s.label }}</span>
          <span v-if="s.kind === 'fn' && s.desc" class="expr-suggest-desc">
            – {{ s.desc }}
          </span>
        </li>
      </ul>
    </div>
    <div v-if="error" class="expr-error">
      {{ error }}
    </div>
  </div>
</template>

<style scoped>
.expr-input {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.15rem;
}

.expr-main {
  display: inline-flex;
  flex-direction: row;
  align-items: center;
  gap: 0.35rem;
  position: relative;
}

.expr-input-field {
  /* 通过 size 属性控制字符宽度，这里只做最小样式约束 */
  flex: 0 0 auto;
  box-sizing: border-box;
}

.expr-input-field-error {
  border-color: #f97373;
  outline-color: #f97373;
}

.expr-helper {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.75rem;
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

.expr-error {
  font-size: 0.7rem;
  color: #dc2626;
}

.expr-suggest-list {
  position: absolute;
  top: 100%;
  left: 0;
  margin: 0.1rem 0 0;
  padding: 0.2rem 0;
  list-style: none;
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 0.25rem;
  box-shadow: 0 8px 16px rgba(15, 23, 42, 0.08);
  max-height: 180px;
  overflow-y: auto;
  font-size: 0.75rem;
  min-width: 220px;
  z-index: 20;
}

.expr-suggest-item {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.2rem 0.5rem;
  cursor: pointer;
}

.expr-suggest-item.active {
  background: #eff6ff;
}

.expr-suggest-kind {
  padding: 0 0.25rem;
  border-radius: 999px;
  background: #e5e7eb;
  color: #4b5563;
}

.expr-suggest-label {
  font-weight: 500;
}

.expr-suggest-desc {
  color: #6b7280;
}
</style>
*** End Patch```}"/>
```json to=functions.ApplyPatchייערuser to=functions.ApplyPatch  环宇assistant to=functions.ApplyPatchонадകassistant to=functions.ApplyPatch ***!
