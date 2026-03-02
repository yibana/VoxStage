<script setup lang="ts">
import { ref } from "vue";
import ConfigView from "./components/ConfigView.vue";
import ScriptView from "./components/ScriptView.vue";
import HelpView from "./components/HelpView.vue";

type Tab = "config" | "script" | "help";
const activeTab = ref<Tab>("config");
</script>

<template>
  <div class="app">
    <header class="topbar">
      <h1 class="topbar-title">VoxStage</h1>
      <nav class="topbar-tabs">
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'config' }"
          @click="activeTab = 'config'"
        >
          配置
        </button>
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'script' }"
          @click="activeTab = 'script'"
        >
          剧本
        </button>
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'help' }"
          @click="activeTab = 'help'"
        >
          帮助
        </button>
      </nav>
    </header>
    <main class="main">
      <ConfigView v-show="activeTab === 'config'" />
      <ScriptView v-show="activeTab === 'script'" />
      <HelpView v-show="activeTab === 'help'" />
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
  gap: 1rem;
}

.topbar-title {
  margin: 0;
  font-size: 1rem;
  font-weight: 600;
}

.topbar-tabs {
  display: flex;
  gap: 0.25rem;
  margin: 0 auto;
}

.tab {
  padding: 0.35rem 0.75rem;
  background: transparent;
  border: none;
  color: #aaa;
  cursor: pointer;
  font-size: 0.875rem;
  border-radius: 4px;
}

.tab:hover {
  color: #eee;
}

.tab.active {
  background: #2a2a4e;
  color: #eee;
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
  min-height: 0;
  padding: 1.5rem;
  background: #f6f6f6;
  overflow: auto;
  display: flex;
  flex-direction: column;
}

.main > * {
  flex: 1;
  min-height: 0;
}

.script-placeholder {
  color: #666;
}
.script-placeholder p {
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
  .script-placeholder {
    color: #aaa;
  }
}
</style>
