# Phase 1：Tauri 壳 + 项目结构

目标：在仓库内搭好 Tauri 应用骨架，窗口能打开，前端能调到一个 Rust command，为 Phase 2（全局配置）做准备。

---

## 1. 目录与创建方式

- **位置**：`apps/voxstage-gui/`，与现有 `crates/` 平级。
- **内容**：
  - `apps/voxstage-gui/src-tauri/`：Tauri 后端（Rust）
  - `apps/voxstage-gui/` 下前端：Vite 工程（推荐 **Vue 3 + TypeScript** 或 **React + TypeScript**，便于后续做配置页和列表）

**方式 A（推荐）：用 create-tauri-app 一步生成**

在仓库根目录执行（按你本机有的包管理器选一个）：

```bash
# 进入 apps 并创建（若无 apps 目录先 mkdir apps）
mkdir -p apps
cd apps

# 使用 create-tauri-app（会提示选名字、框架等）
# 项目名填 voxstage-gui，前端选 Vue 或 React，语言选 TypeScript
pnpm create tauri-app
# 或: npm create tauri-app@latest
# 或: bunx create-tauri-app
```

交互时建议：

- **Project name**：`voxstage-gui`
- **Frontend**：TypeScript + Vue 或 React
- **Package manager**：pnpm / npm 任选
- **Identifier**：如 `com.voxstage.gui`

生成后目录大致为：

```
apps/voxstage-gui/
  src-tauri/          # Rust
  src/                # 前端源码（Vue/React）
  package.json
  ...
```

**方式 B：手动**

```bash
mkdir -p apps/voxstage-gui
cd apps/voxstage-gui
pnpm create vite . --template vue-ts   # 或 react-ts
pnpm add -D @tauri-apps/cli@latest
pnpm tauri init
```

按提示填：App name、Window title、Web assets 路径、Dev server URL（Vite 默认 `http://localhost:5173`）、build 命令等。

---

## 2. 接入现有 Cargo workspace

- 把 Tauri 的 Rust 包加入 workspace，便于以后依赖 `vox-engine`、`vox-runner` 等。

**修改根目录 `Cargo.toml`**，在 `members` 里增加一行：

```toml
[workspace]
members = [
    "crates/core",
    "crates/tts-http",
    "crates/audio",
    "crates/dsl",
    "crates/engine",
    "crates/runner",
    "crates/cli",
    "apps/voxstage-gui/src-tauri",   # 新增
]
```

若 `tauri init` 生成的 `src-tauri/Cargo.toml` 里 `edition` 与 workspace 不一致，可改为与现有 crates 一致（如 `edition = "2021"`，注意当前仓库写的是 2024，按实际可编译为准）。

**可选：避免 workspace 与 Tauri 的 target 路径冲突**

在 `apps/voxstage-gui/package.json` 的 `scripts` 里，把 `tauri` 相关命令的 target 固定到 `src-tauri` 下，例如：

```json
"scripts": {
  "tauri": "tauri",
  "dev": "tauri dev",
  "build": "tauri build"
}
```

若出现「找不到 target / 编译到错误目录」时，再改为：

```json
"tauri": "cross-env CARGO_BUILD_TARGET_DIR=src-tauri/target tauri",
"dev": "cross-env CARGO_BUILD_TARGET_DIR=src-tauri/target tauri dev",
"build": "cross-env CARGO_BUILD_TARGET_DIR=src-tauri/target tauri build"
```

（需要时可 `pnpm add -D cross-env`。）

---

## 3. 一个简单的 Tauri command（验证前后端打通）

- 在 `src-tauri/src/main.rs`（或 `lib.rs` + `main.rs` 分工，视模板而定）里注册一个 command，例如：

```rust
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! (from Rust)", name)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- 在前端入口页（如 Vue 的 `App.vue` 或 React 的 `App.tsx`）里调用一次：

  - 使用 `@tauri-apps/api` 的 `invoke('greet', { name: 'VoxStage' })`，把返回的字符串显示在页面上（或 `console.log`）。

- 运行 `pnpm tauri dev`（或 `npm run tauri dev`），确认窗口打开且页面上能看到 Rust 返回的字符串，即表示 Phase 1 的「桥」已通。

---

## 4. 基础布局（为后续配置 + 剧本列表留位）

- **顶部栏**：占一行，左侧标题「VoxStage」或「剧本编辑」，右侧预留位置（Phase 2 放 Run / Save 等不实现逻辑）。
- **主内容区**：下方一大块空白或占位文案（如「配置 | 剧本」两个 Tab 的占位，Phase 2 再做真实 Tab）。

前端实现方式不限：在现有 Vite 模板里加一个简单布局组件即可（例如顶部 `header` + 下方 `main`，用 CSS 或 UI 库均可）。

---

## 5. 验收标准

- [ ] 在 `apps/voxstage-gui` 下能执行 `pnpm install` 与 `pnpm tauri dev`，窗口正常打开。
- [ ] 页面能调用 `invoke('greet', …)` 并显示 Rust 返回字符串。
- [ ] 根 workspace 已加入 `apps/voxstage-gui/src-tauri`，在仓库根执行 `cargo build -p <src-tauri 的 package name>` 能通过（包名见 `src-tauri/Cargo.toml` 的 `[package].name`）。
- [ ] 界面有顶部栏 + 主内容区占位。

---

## 6. Phase 1 已完成项（按方式 A 生成后）

- [x] 根 `Cargo.toml` 已加入 `apps/voxstage-gui/src-tauri` 为 workspace member。
- [x] `App.vue` 已改为：顶部栏（标题 VoxStage + 运行/保存占位）+ 主内容区；`onMounted` 时调用 `greet("VoxStage")` 并显示返回值，用于验证 Rust 桥接。
- 在 `apps/voxstage-gui` 下执行 `pnpm tauri dev` 可启动窗口并确认桥接正常。

---

## 7. 后续（Phase 2 前）

- Phase 2 会在「主内容区」做「配置」页（模型 / 角色），并增加 `get_config`、`save_config` 等 command。
- Phase 1 的 `src-tauri` 暂不依赖 `vox-engine` / `vox-runner`，等 Phase 4 再接。

---

## 8. 参考

- [Tauri 2 - Create a Project](https://v2.tauri.app/start/create-project/)
- [Tauri 2 - Project Structure](https://v2.tauri.app/start/project-structure/)
- [Tauri - Invoke (Commands)](https://v2.tauri.app/develop/calling-rust/)
