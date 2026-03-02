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

## 已完成（步骤 5：窗口与运行体验增强）

### 1. GUI 窗口大小 / 位置记忆

- 在 Tauri 端的全局配置 `AppConfig` 中新增 `window` 字段（`WindowState`）：
  - 记录窗口的宽度 / 高度（逻辑像素）、是否最大化、左上角坐标 `x/y`。
- 启动时：
  - 从 `app_data_dir/config.json` 读取上次窗口状态；
  - 若记录为最大化，则直接最大化主窗口；
  - 否则按记录的宽高设置窗口大小，并根据当前显示器尺寸做边界检查：
    - 如果记录的尺寸超过屏幕，则自动缩放到屏幕宽高的 90% 以内；
    - 如果记录的位置超出当前屏幕可视范围，则自动重新居中。
- 运行过程中：
  - 监听 `WindowEvent::Resized` / `WindowEvent::Moved` 事件；
  - 在窗口大小或位置变化时，实时把最新的大小与位置写回 `config.json`。

### 2. 角色配置与剧本角色下拉同步

- `save_config` 在成功写入配置文件后，会向前端广播 `config-changed` 事件；
- `ScriptView.vue` 在挂载时：
  - 初次加载角色列表 `loadRoles()`；
  - 订阅 `config-changed` 事件，在事件到来时重新调用 `loadRoles()`；
- 效果：在「配置」Tab 中新增或修改角色后，无需重启应用，「剧本」Tab 中的角色下拉框会自动刷新。

### 3. 剧本循环运行模式

- 在前端「剧本」工具栏中：
  - 在「运行」按钮左侧增加「循环」复选框；
  - 勾选后会让当前剧本从头到尾连续循环运行，直到点击「中断」或关闭应用；
  - 运行中（`isRunning = true`）时复选框会被禁用，避免中途切换模式。
- Tauri 端：
  - `run_script` 新增可选参数 `loop_run: Option<bool>`，由前端通过 `invoke("run_script", { voxText, loopRun })` 传入；
  - 内部在构建好 `ModelManager` 与 Tokio runtime 后，用 `loop` 包裹 `run_script_with_audio` 调用：
    - 每轮开始前检查 `stop_flag`，若已请求停止则退出循环；
    - 每轮运行时，将新的进度回调 `progress_cb` 传给 runner，用于高亮当前执行行；
    - 当本轮执行出错或 `loop_run` 为 `false` 时，立刻发出 `script-finished` 事件并返回；
    - 在循环模式下，只要本轮成功且未收到停止请求，就自动进入下一轮。

### 4. 表达式输入与 speak 参数覆写增强

- 表达式输入框（`ExprInput.vue`）：
  - 支持基于简单规则的即时语法校验：
    - 检查字符串引号是否闭合（支持 `\"` / `\\` 转义）；
    - 检查圆括号配对情况；
    - 检查明显错误（以运算符开头/结尾等），在出错时以红色提示文案标出。
  - 内置自动补全：
    - 在表达式中输入变量或函数名前缀时，会在下方弹出联想列表；
    - 支持上下方向键选择候选，`Enter` / `Tab` 接受补全，`Esc` 关闭候选。
    - 候选列表中展示变量/函数类型、名称以及内置函数的简要说明。
- 表达式辅助下拉功能：
  - 下拉中展示内置时间与随机函数（`now` / `time_hour` / `time_minute` / `time_second` / `rand` / `rand_int` / `rand_bool` / `rand_choice`），并附带文案说明。
- `speak` 语句的 per-speak 参数覆写：
  - 在每条 `speak` 行添加「参数」按钮，点击后弹出参数覆写弹窗；
  - 弹窗内以键值对列表的形式编辑本句的参数，例如 `language` / `speaker_id` / `ref_audio_path` 等；
  - 仅对当前 `speak` 行生效，角色默认参数仍由 `role` 块控制；
  - 对应生成的 DSL 语法形如：`speak RoleName(key1 = "v1", key2 = "v2") "文本"`；
  - 从 `.vox` 解析回来的 `speak` 语句会将参数映射回前端的 `speakParams`，在弹窗中可视化编辑。

---

## 使用说明（更新版）

1. 在「配置」Tab 配置好模型（endpoint、provider 等）与角色；
2. 在「剧本」Tab：
   - 使用列表式编辑模式编排 `speak` / `sleep` / `if` / `for` / `while` / `let` / `set` / BGM 步骤；
   - 或在 Code 模式中直接编辑 `.vox` 文本，并通过「返回编辑」同步回列表视图；
   - 通过表达式「插入」下拉快速选择已有变量和内置函数、为 `speak` 文本插入 `${变量}` 占位。
3. 点击「运行」：
   - 可以在运行中使用「暂停 / 继续」与「中断」控制脚本执行；
   - 如勾选「循环」，脚本会从头到尾反复执行，直到点击「中断」；
   - 编辑列表中当前执行的 `speak` / `sleep` 行会自动高亮，并显示当前执行行号。
4. 若模型服务未启动或脚本有误，会在顶部显示「运行失败：…」详细错误信息。
