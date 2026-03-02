# Phase 4：剧本持久化（打开 / 另存为）

目标：在 GUI 剧本 Tab 中支持通过系统对话框「打开」与「另存为」脚本文件，实现剧本的持久化与交换。

---

## 已完成

### 1. Tauri 端（`apps/voxstage-gui/src-tauri`）

- **依赖**：增加 `tauri-plugin-dialog`，用于弹出系统「打开文件」「另存为」对话框。
- **命令**：
  - `open_script_file()`：弹出打开对话框，筛选 `.vox` / `.json`；用户选择文件后读取内容，返回 `{ path: string, content: string }`；用户取消则返回错误。
  - `save_script_file(content: string)`：弹出另存为对话框，默认文件名 `script.vox`，筛选 `.vox`；用户选择路径后写入内容，返回保存路径；用户取消则返回错误。

### 2. 前端 ScriptView.vue

- **打开**：点击「打开」后调用 `open_script_file`；若扩展名为 `.json` 则 `JSON.parse(content)` 得到 `ScriptItem[]` 并赋给 `items`；否则将 `content` 交给 `parse_vox_to_script` 得到 `ScriptItem[]` 并赋给 `items`；成功后切回编辑模式；失败时在界面显示 `fileError`。
- **另存为**：点击「另存为」后取当前 `get_config` + `toVox(config, items)` 得到 .vox 文本，再调用 `save_script_file(content)` 写入用户选择的路径；失败时显示 `fileError`。
- **UI**：编辑模式工具栏有「打开」「另存为」「清空脚本」；文件操作错误使用 `.file-error` 展示。
- **自动保存草稿**：编辑中的剧本以 JSON 形式自动保存到 `app_data_dir/script_draft.json`（防抖约 800ms）；启动时自动加载草稿，若有则恢复列表。

### 3. Tauri 端草稿命令

- `get_script_draft()`：读取 `script_draft.json` 内容，不存在则返回 `"[]"`。
- `save_script_draft(json: string)`：将 JSON 写入 `script_draft.json`。

---

## 使用说明

1. **打开**：在剧本 Tab 编辑模式下点击「打开」，在对话框中选择 `.vox` 或 `.json` 文件；.vox 会按 DSL 解析为剧本列表，.json 会按 `ScriptItem[]` 解析；解析成功后列表会更新并处于编辑模式。
2. **另存为**：在编辑模式下点击「另存为」，当前剧本（含配置生成的 model/role + 步骤）会生成为 .vox 文本并弹出保存对话框，选择路径后写入该文件。

---

## 后续（Phase 5 及以后）

- 可选：记住「当前文件路径」，支持「保存」直接覆盖当前文件。
- 将 GUI 剧本与 vox-engine / vox-runner 对接，实现一键「运行剧本」。
