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
</script>

<template>
  <div class="expr-input">
    <input
      ref="inputRef"
      :value="modelValue"
      type="text"
      class="expr-input-field"
      :placeholder="placeholder"
      :size="displaySize"
      @input="onInput"
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
        <option v-for="fn in builtins" :key="fn.name" :value="fn.name">
          {{ fn.name }}
        </option>
      </select>
    </div>
  </div>
</template>

<style scoped>
.expr-input {
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 0.35rem;
}

.expr-input-field {
  /* 通过 size 属性控制字符宽度，这里只做最小样式约束 */
  flex: 0 0 auto;
  box-sizing: border-box;
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
</style>
*** End Patch```}"/>
```json to=functions.ApplyPatchייערuser to=functions.ApplyPatch  环宇assistant to=functions.ApplyPatchонадകassistant to=functions.ApplyPatch ***!
