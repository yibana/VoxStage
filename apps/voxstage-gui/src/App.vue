<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const bridgeMsg = ref("");

onMounted(async () => {
  try {
    bridgeMsg.value = await invoke<string>("greet", { name: "VoxStage" });
  } catch (e) {
    bridgeMsg.value = `调用失败: ${e}`;
  }
});
</script>

<template>
  <div class="app">
    <header class="topbar">
      <h1 class="topbar-title">VoxStage</h1>
      <div class="topbar-actions">
        <span class="placeholder">运行</span>
        <span class="placeholder">保存</span>
      </div>
    </header>
    <main class="main">
      <p class="placeholder-text">配置 | 剧本（Phase 2 实现）</p>
      <p class="bridge-ok" v-if="bridgeMsg">
        Rust 桥接正常：{{ bridgeMsg }}
      </p>
    </main>
  </div>
</template>

<style scoped>
.app {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 1rem;
  height: 2.5rem;
  background: #1a1a2e;
  color: #eee;
  flex-shrink: 0;
}

.topbar-title {
  margin: 0;
  font-size: 1rem;
  font-weight: 600;
}

.topbar-actions {
  display: flex;
  gap: 0.75rem;
}

.topbar-actions .placeholder {
  font-size: 0.875rem;
  opacity: 0.8;
  cursor: default;
}

.main {
  flex: 1;
  padding: 1.5rem;
  background: #f6f6f6;
}

.placeholder-text {
  color: #666;
  margin: 0 0 1rem 0;
}

.bridge-ok {
  font-size: 0.875rem;
  color: #0d7377;
  margin: 0;
}
</style>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;
  color: #0f0f0f;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

@media (prefers-color-scheme: dark) {
  .main {
    background: #2f2f2f;
  }
  .placeholder-text {
    color: #aaa;
  }
}
</style>
