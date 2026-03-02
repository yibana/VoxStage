# Phase 5：剧本运行 & 进度联动（engine/runner 对接）

目标：在 GUI 「剧本」Tab 中实现一键运行当前剧本，支持 TTS + BGM 播放、暂停/中断，以及执行进度高亮；并在编辑模式下提供更友好的 BGM 与表达式编辑体验。

---

## 已完成（步骤 1：基础运行）

### 1. Tauri 端（`apps/voxstage-gui/src-tauri`）

- **依赖**：增加 `vox-core`、`vox-engine`、`vox-runner`、`vox-tts-http`、`tokio`。
- **命令** `run_script(vox_text: String, app: AppHandle)`：
  - 根据脚本中的 `model` 块注册 TTS Provider（bert_vits2 / gpt_sovits_v2），逻辑与 CLI 一致；
  - 在后台线程中创建 Tokio runtime，调用 `run_script_with_audio` 解析、执行并播放，避免阻塞 GUI 主线程。

### 2. runner 端（`crates/runner`）

- 使用 `compile_script_to_channel` 将脚本编译为 `EngineCommandWithMeta { source_index, command }` 序列：
  - `source_index` 为静态语句索引（仅对 `speak` / `sleep` 分配），在循环中多次执行同一条语句会复用同一个索引；
  - `command` 为已合成的音频播放命令或 BGM 控制命令。
- 在消费循环中按顺序播放 TTS、执行 `sleep` 与 BGM 控制命令。

### 3. 前端 `ScriptView.vue`（基础运行）

- **运行按钮**：编辑模式与 Code 模式工具栏均有「运行」按钮；
  - 编辑模式：使用当前配置与列表 `items` 通过 `toVox(config, items)` 生成 `.vox` 文本，并回写解析结果以带上 `sourceIndex`；
  - Code 模式：直接使用 `codeText` 作为 `.vox` 文本。
- **状态**：`isRunning` 运行中时按钮显示「运行中…」并禁用；`runError` 显示运行失败信息。

---

## 已完成（步骤 2：暂停 / 中断）

### 1. Tauri 端

- 新增全局状态 `PlaybackControl { pause_flag, stop_flag }`，通过 `.manage()` 注入。
- 暂停 / 继续：
  - `pause_script()`：将 `pause_flag` 设为 `true`，runner 在每条命令之间检查并暂停；
  - `resume_script()`：将 `pause_flag` 设为 `false`，恢复执行。
- 中断：
  - `stop_script()`：将 `stop_flag` 设为 `true`，runner 在下一条命令前停止 BGM 并退出命令循环，同时清除暂停状态。

### 2. runner 端

- 在命令消费循环中优先检查 `stop_flag`，其后检查 `pause_flag`：
  - `stop_flag == true`：立即停止 BGM 并 `break`；
  - `pause_flag == true`：以 50ms 间隔轮询，阻塞在当前命令之前。

### 3. 前端 `ScriptView.vue`

- 在运行按钮旁增加：
  - 「暂停 / 继续」按钮：基于 `isPaused` 调用 `pause_script` / `resume_script`；
  - 「中断」按钮：调用 `stop_script`，并在前端重置 `isRunning` / `isPaused`。

---

## 已完成（步骤 3：执行进度同步 & 高亮）

### 1. engine / runner

- `vox-engine`：在构建执行上下文时为 AST 中的 `speak` / `sleep` 语句分配静态 `source_index`，并存入 `ExecContext.source_index_map`；
- 编译命令时，每条 `SpeakAudio` / `Sleep` 命令都会携带对应的 `source_index`，不随循环次数改变。
- `vox-runner`：在每次即将执行带有效 `source_index` 的命令前，通过可选回调 `progress_cb(source_index)` 通知外界。

### 2. Tauri 事件桥接

- 在 `run_script` 中构造进度回调：
  - 调用 `app.emit("script-progress", source_index)` 将当前静态语句索引广播给前端；
  - 在脚本正常结束或中断后，调用 `app.emit("script-finished", ())` 通知前端清理状态。

### 3. 前端高亮逻辑

- `ScriptItem` / `ScriptItemDto` 中增加 `sourceIndex?: number`，解析 `.vox` 时与 engine 中的 `source_index` 对齐；
- `ScriptView.vue` 中：
  - 监听 `script-progress` 事件，将 payload 写入 `activeSourceIndex`，并根据 `sourceIndex` 计算当前高亮的行号；
  - 使用 `script-row-active` 样式高亮当前执行行（含明显左侧高亮条）；
  - 监听 `script-finished` 事件，在运行结束或中断后清空高亮与运行状态。

---

## 已完成（步骤 4：BGM 与表达式编辑增强）

### 1. BGM 步骤可视化编辑

在编辑模式中，支持以下 BGM 步骤类型，并与 `.vox` DSL 自动互转：

- `bgm_play`：对应 `bgm "path_or_url" [loop]`
  - 输入：路径或 URL 文本框 + 「循环」复选框；
  - 左侧显示播放图标与彩色类型徽标。
- `bgm_volume`：对应 `bgm_volume 0.5`
  - 输入：0–1 的音量数值框；
  - 左侧显示音量图标。
- `bgm_pause` / `bgm_resume` / `bgm_stop`：
  - 显示对应暂停 / 恢复 / 停止图标与说明文字；
  - 底层映射为 `bgm_pause` / `bgm_resume` / `bgm_stop` 语句。

### 2. 表达式编辑辅助面板

为以下语句的表达式提供「插入」下拉面板：

- `let` / `set` 的右侧表达式；
- `if` / `while` 的条件表达式；
- `for` 的次数表达式；
- `speak` 文本中的 `${变量}` 占位符。

具体能力：

- 自动收集当前脚本中通过 `let` / `set` 定义的变量，组成变量下拉列表；
- 提供内置函数模板下拉：
  - `time_hour()` / `rand_int(1, 10)` / `rand_bool()` / `rand_choice("a", "b")`；
- 选择变量或内置函数后，会将文本安全地追加到对应表达式末尾；
- 在 `speak` 文本编辑区，可以从变量下拉中插入 `${varName}` 形式的文本占位符，配合引擎内的插值逻辑在运行时替换为变量值。

---

## 使用说明（更新版）

1. 在「配置」Tab 配置好模型（endpoint、provider 等）与角色；
2. 在「剧本」Tab：
   - 使用列表式编辑模式编排 `speak` / `sleep` / `if` / `for` / `while` / `let` / `set` / BGM 步骤；
   - 或在 Code 模式中直接编辑 `.vox` 文本，并通过「返回编辑」同步回列表视图；
   - 通过表达式「插入」下拉快速选择已有变量和内置函数、为 `speak` 文本插入 `${变量}` 占位。
3. 点击「运行」：
   - 可以在运行中使用「暂停 / 继续」与「中断」控制脚本执行；
   - 编辑列表中当前执行的 `speak` / `sleep` 行会自动高亮，并显示当前执行行号。
4. 若模型服务未启动或脚本有误，会在顶部显示「运行失败：…」详细错误信息。
