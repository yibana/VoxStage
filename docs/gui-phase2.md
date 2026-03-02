# Phase 2：全局配置（模型 + 角色）

目标：在应用内维护「模型」与「角色」的全局配置，存于 `app_data_dir/config.json`，配置页可增删改并保存。

---

## 已完成

### Rust（`apps/voxstage-gui/src-tauri/src/lib.rs`）

- **结构体**：`AppConfig`（`models: Vec<ModelEntry>`, `roles: Vec<RoleEntry>`）、`ModelEntry`（name, type, provider, endpoint, model_id, extra）、`RoleEntry`（name, model, params）。
- **存储**：`config_path(app)` 使用 `app.path().app_data_dir()` 下的 `config.json`；首次读写前 `create_dir_all`。
- **Commands**：
  - `get_config(app)` → 读取并反序列化，不存在则返回默认空配置。
  - `save_config(app, config)` → 序列化并写入。
  - `get_roles(app)` → 仅返回角色列表（供后续剧本页下拉等使用）。

### 前端

- **类型**：`src/types/config.ts` 中 `AppConfig` / `ModelEntry` / `RoleEntry` 与 Rust 对齐，并提供 `emptyModel()` / `emptyRole()`。
- **配置页**：`src/components/ConfigView.vue`
  - 模型列表：每项可编辑 name / type / provider / endpoint / model_id，支持添加、删除。
  - 角色列表：每项可编辑 name、模型（下拉，来源为当前配置中的模型名）、params（JSON 文本），支持添加、删除。
  - 顶部「保存配置」按钮调用 `save_config`，并显示成功/失败提示。
- **主界面**：`App.vue` 增加「配置 | 剧本」两个 Tab，默认显示配置页；剧本为占位，Phase 3 实现。

---

## 使用说明

1. 启动应用后点击「配置」Tab（默认即为此页）。
2. 在「模型」区添加模型，填写 name、type、provider、endpoint、model_id（与 DSL 的 model 块一致）。
3. 在「角色」区添加角色，选择已填写的模型，params 可填 JSON 如 `{"speed":"1.0","language":"zh"}`。
4. 点击「保存配置」写入 `app_data_dir/config.json`；下次打开会自动加载。

---

## 后续（Phase 3）

- 剧本列表编排（缩进子列表表示 if/for/while）。
- 剧本中的「说话」步骤从全局角色下拉选择，不再在剧本内定义 model/role。
