# VoxStage GUI（apps/voxstage-gui）

VoxStage GUI 是 VoxStage 项目的桌面前端，基于 **Tauri 2 + Vue 3 + TypeScript** 构建，用于：

- 可视化维护 TTS 模型与角色配置；
- 以列表形式编辑 `.vox` 剧本（支持缩进块结构与 BGM 步骤）；
- 一键调用 `vox-engine` / `vox-runner` 在本地播放 TTS 与 BGM；
- 在运行时高亮当前执行步骤，并提供暂停 / 继续 / 中断控制与「循环运行」模式；
- 记住窗口大小 / 位置与最大化状态，下次启动时恢复到上次使用状态。

---

## 架构概览

GUI 分为 **前端 Vue 应用** 与 **Tauri Rust 后端** 两层：

- 前端：`apps/voxstage-gui/src`
  - `App.vue`：顶层布局与 Tab 切换（「配置」/「剧本」）。
  - `components/ConfigView.vue`：模型与角色配置界面。
  - `components/ScriptView.vue`：剧本编辑与运行界面（编辑 / Code 双模式、BGM、表达式辅助、进度高亮等）。
  - `types/config.ts`：前端配置类型定义，与 Tauri 端 `AppConfig` 对应。
  - `types/script.ts`：前端剧本步骤类型定义（`ScriptItem`），以及 `toVox(config, items)`，负责将列表式剧本导出为 `.vox` 文本。

- 后端：`apps/voxstage-gui/src-tauri`
  - `src/lib.rs`：Tauri 入口与命令注册：
    - 配置相关：`get_config` / `save_config` / `get_roles`；
    - 剧本相关：`parse_vox_to_script`（`.vox` → 列表）、`open_script_file` / `save_script_file`、`get_script_draft` / `save_script_draft`；
    - 运行相关：`run_script(vox_text, loop_run?)` / `pause_script` / `resume_script` / `stop_script`。
  - 运行时：在 `run_script` 中根据 `.vox` 脚本：
    - 使用 `vox-dsl` 解析 AST；
    - 调用 `vox-engine::register_providers_from_script` 注册 TTS Provider；
    - 调用 `vox-runner::run_script_with_audio` 执行脚本并播放 TTS 与 BGM。
  - 事件桥接：通过 `app.emit("script-progress", source_index)` / `app.emit("script-finished", ())` 将当前执行步骤索引广播给前端，用于高亮当前行。
  - 窗口状态：在启动时根据 `config.json` 中记录的 `window` 字段恢复窗口大小 / 位置 / 最大化状态，并在窗口移动或缩放时自动更新配置。

依赖关系与整体架构详见仓库根目录的 `README.md` 与 `docs/gui-phase5.md`。

---

## 开发与调试

```bash
cd apps/voxstage-gui
pnpm install

# 开发模式（前端 + Tauri dev）
pnpm run tauri dev

# 仅前端构建（生成 dist，用于 Tauri build）
pnpm build

# 打包桌面应用
pnpm tauri build
```

推荐 IDE 与插件：

- VS Code + Vue (Volar) + Tauri VS Code 插件 + rust-analyzer。
