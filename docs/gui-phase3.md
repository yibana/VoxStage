# Phase 3：剧本列表编排（缩进列表）

目标：在 GUI 中实现「剧本」Tab 的列表式编排，用缩进表示 if/for/while 等控制结构的层级，先实现纯前端结构与编辑，后续再映射到 DSL AST 与执行引擎。

---

## 已完成

### 1. 前端数据结构（`src/types/script.ts`）

- 定义 `ScriptItemType`：
  - `'speak' | 'sleep' | 'if' | 'for' | 'while' | 'let' | 'set'`。
- 定义 `ScriptItem`：
  - 字段：
    - `id: string`：前端唯一标识；
    - `type: ScriptItemType`：步骤类型；
    - `indent: number`：缩进层级，0 为顶层，数值越大缩进越深；
    - `role?: string` / `text?: string`：用于 `speak`；
    - `ms?: number`：用于 `sleep`；
    - `condition?: string`：用于 `if` / `while`；
    - `times?: string`：用于 `for`（允许表达式字符串，后续由引擎解释）。
- 工具函数：
  - `createItem(type: ScriptItemType, indent = 0): ScriptItem`：根据类型创建带默认值的新步骤。

> 说明：当前 Phase 仅在前端维护 `indent`，表示「视觉/结构上的层级」。后续 Phase 可以根据 `indent` 解析成真正的嵌套 AST 结构。

### 2. 剧本视图（`src/components/ScriptView.vue`）

- **编辑 | Code 双模式**：工具栏可切换「编辑」（列表）与「Code」（全屏 .vox 文本编辑）；Code 模式右下角有「返回编辑」按钮，返回时解析 .vox 并更新列表。
- **添加步骤**：无顶部按钮组；每个块在**块内最后一行下方**显示「+ 在此块末尾添加子步骤」下拉；剧本**最下方**有「+ 在剧本末尾添加顶层步骤」下拉；空剧本时同样显示顶层添加下拉。
- **列表区域**：
  - 所有步骤按顺序显示，每行一个 `ScriptItem`，通过 `item.indent` 控制缩进。
  - 每行左侧为**类型徽标（badge）**，按类型使用不同颜色（说话/等待/如果/循环/当/定义/赋值）；徽标右侧仅保留该类型所需控件，不再重复类型文字。
  - 类型与控件：`speak`（角色下拉 + 台词）、`sleep`（毫秒数 + ms）、`if`/`while`（条件输入）、`for`（次数 + 次）、`let`/`set`（变量名 + = + 表达式）。
  - 每行右侧：`↑` / `↓` 上移/下移（若为块则整块含子级一起移动；可跨块与相邻兄弟交换）、`删` 删除。
- **脚本持久化**：见 Phase 4（打开/另存为、自动保存草稿）。

### 3. 与主界面集成（`src/App.vue`）

- 在原有的「配置 / 剧本」Tab 结构基础上：
  - `配置` Tab：仍挂载 `ConfigView`（Phase 2 全局配置）。
  - `剧本` Tab：现在挂载 `ScriptView`，替代之前的占位文本。
- Tab 切换逻辑保持不变，仅切换显示哪一个视图。

> 剧本草稿自动保存到 `app_data_dir/script_draft.json`，启动时自动加载；打开/另存为见 Phase 4。尚未与引擎连接。

---

## 使用说明

1. 打开 GUI 应用，切换到「剧本」Tab。
2. 通过「在剧本末尾添加顶层步骤」或各块下方的「在此块末尾添加子步骤」下拉添加步骤（说话/等待/如果/循环/当/定义变量/设置变量）。
3. 说话：选角色、填台词；等待：填毫秒；if/while：填条件；for：填次数；let/set：填变量名与表达式。
4. 使用行右侧 `↑` / `↓` 调整顺序（块会连同子级一起移动），`删` 删除。
5. 可切换「Code」模式直接编辑 .vox 文本，再「返回编辑」解析回列表。

当前阶段的设计重点在于：

- 以**列表 + 缩进**的形式建立「剧本结构」的直观编辑体验；
- 保持与 DSL 的核心语义相近（speak/sleep/if/for/while + 表达式）。

---

## 后续

- 将 GUI 中的剧本与 `vox-engine` / `vox-runner` 对接，实现一键「运行剧本」。

