## VoxStage 项目说明

VoxStage 是一个基于 Rust 的 DSL 驱动 AI 语音执行引擎，目标是通过自定义脚本语言调度多种 TTS 模型（远程 / 本地），实现 AI 主播、有声小说、多角色对白等场景下的高频模型切换与音频播放。

当前仓库处于 **核心骨架搭建阶段**，重点在于模型调用层与音频播放层的解耦设计。

---

## 项目架构概览

项目采用 **Cargo workspace + 多 crate 分层**，按职责拆分为：

- **`vox-core`（crates/core）**  
  - 定义跨层共享的领域模型与抽象：
    - `TtsProvider`：统一的 TTS Provider trait。
    - `ModelCapabilities`：模型能力声明（speed / volume / pitch / emotion / streaming / custom）。
    - `SynthesisRequest`：合成请求统一结构。
    - `AudioStream`：音频流抽象（当前为 `Full(Vec<u8>)`）。
    - `TtsError`：TTS 错误类型。
  - 不依赖任何具体 HTTP 客户端或音频库，是整个系统的基础层。

- **`vox-tts-http`（crates/tts-http）**  
  - 封装通过 HTTP 调用外部 TTS 服务的 Provider 实现：
    - `BertVits2Provider`（Bert-VITS2）
    - `GptSovitsV2Provider`（GPT-SoVITS-v2，占位实现）
  - 使用 `reqwest` 负责 HTTP 通信，将 `SynthesisRequest` 映射为 HTTP 请求，并返回 `AudioStream`。
  - 对上暴露 `TtsProvider` 接口，对下屏蔽具体 HTTP 细节。

- **`vox-dsl`（crates/dsl）**  
  - 脚本语言解析层：将 `.vox` 源码解析为 AST（`Script` / `Item` 等）。
  - 支持 `model` / `role` / `let` / `speak` / `sleep` / `if` / `for` / `while` 以及 **BGM 语句**（`bgm` / `bgm_volume` / `bgm_pause` / `bgm_resume` / `bgm_stop`）。

- **`vox-audio`（crates/audio）**  
  - 基于 `rodio` 的音频播放模块：
    - `play_audio_blocking(data: &[u8])`：使用系统默认输出设备播放一段完整音频数据（TTS），播放结束前阻塞。
    - `BgmController`：独立 BGM 轨道，支持：
      - `play_bgm(data, loop)`：播放/切换背景音，可选循环；
      - `pause_bgm()` / `resume_bgm()` / `stop_bgm()`；
      - `set_bgm_volume(volume)`。
    - `AudioError`：播放相关错误。
  - 未来会在此基础上扩展 `AudioQueue`、设备枚举与选择等能力。

- **`vox-engine`（crates/engine）**  
  - 执行引擎层，负责：
    - `ModelManager`：`HashMap<String, Arc<dyn TtsProvider>>` 管理所有已注册模型，支持 O(1) 切换。
    - 从 DSL AST 中收集 `role` / `let` 定义。
    - 解释执行控制流语句：`speak` / `sleep` / `if` / `for` / `while` 以及 **BGM 语句**。
    - 构造 `SynthesisRequest`，调用对应的 `TtsProvider`，返回 `AudioStream` 或执行命令流。
    - `EngineCommand`：除 `SpeakAudio` / `Sleep` 外，包含 `BgmPlay` / `BgmPause` / `BgmResume` / `BgmStop` / `BgmVolume`，由 runner 消费并转调 `vox-audio`。
  - 不依赖任何具体 HTTP 实现或音频播放，只做“脚本 → TTS/BGM 命令”的编排。

- **`vox-runner`（crates/runner）**  
  - 运行器层，用于将执行引擎产出的命令流与音频播放串联起来：
    - 调用 `vox-engine::compile_script_to_channel` 将脚本编译为顺序的 `EngineCommand`（`SpeakAudio` / `Sleep` / BGM 相关）。
    - 创建并持有 `BgmController`，根据路径加载 BGM 文件（当前仅支持本地路径），在本地设备上播放 TTS 与 BGM，并按 `Sleep` 控制间隔。

- **`vox-cli`（crates/cli）**  
  - 命令行入口示例程序，用于将各层能力串联起来：
    - 创建具体的 Provider 实例（如 `BertVits2Provider`）并注册到 `vox-engine::ModelManager`。
    - 从 `.vox` 脚本文件读取 DSL 源码。
    - 调用 `vox-runner::run_script_with_audio`，完成“脚本 → 执行 → 播放”的完整流程。

依赖方向（自下而上）为：

```text
vox-core
  ├─> vox-tts-http
  ├─> vox-audio
  ├─> vox-dsl
  ├─> vox-engine
  ├─> vox-runner
  └─> vox-cli  (组合以上各层)
```

---

## 当前实现进度

### 1. 模型调用层（TTS Provider）

- ✅ 定义统一抽象 `TtsProvider` / `SynthesisRequest` / `AudioStream`（`vox-core`）。
- ✅ 实现 **Bert-VITS2 HTTP Provider**（`BertVits2Provider`，位于 `vox-tts-http`）：
  - 通过 **GET `http://localhost:5000/voice`** 调用本地 Bert-VITS2 服务。
  - 使用 query 参数传递：
    - `text`、`model_id`、`speaker_id`
    - `auto_split`、`auto_translate`
    - `emotion`、`language`、`length`
    - `noise`、`noisew`、`sdp_ratio`、`style_weight`
  - 将 HTTP 响应体作为音频字节读取并包装为 `AudioStream::Full(Vec<u8>)`。
- ✅ 实现 **GPT-SoVITS-v2 Provider 占位版本**（`GptSovitsV2Provider`）：
  - 目前仅打印参数并返回一段伪造音频数据，后续可按 Bert-VITS2 的方式补齐 HTTP 实现。

### 2. 音频系统

- ✅ 基于 `rodio` 的播放实现（`vox-audio`）：
  - 使用系统默认输出设备。
  - TTS：`play_audio_blocking` 对一整段音频数据进行阻塞播放。
  - **BGM**：`BgmController` 独立 Sink，支持播放/循环/暂停/恢复/停止/音量；与 TTS 双轨并存，由 rodio 混音输出。
- ⏳ 计划中：
  - `AudioQueue`：维护播放队列，支持多个合成请求按顺序输出。
  - `AudioOutputManager`：音频设备枚举与选择。
  - 为流式音频 (`AudioStream::Streaming`) 预留接口。

### 3. DSL / 执行引擎

- ✅ `vox-dsl`：
  - 支持 `model` / `role` / `let` / `speak` / `sleep` / `if` / `for` / `while` 以及 **BGM** 语法：
    - `model` 块：声明模型配置（目前主要用作文档与将来扩展的配置源）。
    - `role` 块：绑定模型与默认参数（如 `speed` / `language` / `speaker_id`）。
    - `let` 语句：定义简单变量（字符串或数字，以字符串形式存储），可用于后续插值：
      - `let user_name = "小明"`
      - `let speed_fast = 1.3`
      - 当前实现中，`let` 是“全局赋值/覆盖”，尚未实现块级作用域（即块内 `let` 会影响后续所有语句）。
    - `speak` 语句：触发一次 TTS 调用，支持在括号中写覆盖参数：
      - `speak Girl "一句话"`
      - `speak Girl(speed = 1.3, language = "EN") "另一句话"`
      - 支持在文本中使用 `${var}` 字符串插值，例如：
        - `speak Girl "你好，${user_name}"`
    - `sleep` 语句：在执行过程插入延迟（毫秒）：
      - `sleep 1000  # 延迟 1 秒`
    - `if` 条件语句：
      - 语法：`if lang == "ZH" { ... }` 或 `if flag != "off" { ... }`
      - 当前条件只支持变量与字符串字面量的 `==` / `!=` 比较。
    - `for` 次数循环：
      - 语法：`for 3 { ... }`，表示将块内语句重复执行 3 次。
    - `while` 条件循环：
      - 语法：`while keep_running { ... }`，当变量值为 `"true"`（忽略大小写）时继续循环。
    - **BGM 语句**：
      - `bgm "path_or_url"` 或 `bgm "path" loop` / `bgm "path" once`：播放背景音（当前 runner 仅支持本地路径）。
      - `bgm_volume 0.5`：设置 BGM 音量（1.0 为原始音量）。
      - `bgm_pause` / `bgm_resume` / `bgm_stop`：暂停、恢复、停止 BGM。
  - 输出独立于执行的 AST 结构（含 `BgmPlayStmt` / `BgmVolumeStmt` 等）。

- ✅ `vox-engine`：
  - 提供 `ModelManager` 持有 `TtsProvider` 实例。
  - `run_script_streaming`：按顺序解释执行所有语句：
    - `let` 更新变量表。
    - `if/for/while` 通过递归执行子块实现控制流。
    - `speak` 构造 `SynthesisRequest` 并调用 Provider，将 `AudioStream` 交给回调处理。
    - `sleep` 通过 `tokio::time::sleep` 控制后续语句的时间。
  - `compile_script_to_commands` / `compile_script_to_channel`：
    - 将脚本“预编译”为一串 `EngineCommand`（`SpeakAudio` / `Sleep` / `BgmPlay` / `BgmPause` / `BgmResume` / `BgmStop` / `BgmVolume`），由上层（如 `vox-runner`）负责具体播放与 BGM 加载。

---

## 本地运行说明（当前阶段）

1. **准备环境**
   - 安装 Rust（推荐使用 `rustup`，稳定版即可）。
   - 在本地启动 Bert-VITS2 服务，确保接口：
     - 地址：`http://localhost:5000/voice`
     - 支持 GET 请求并接受当前 Provider 构造的参数。

2. **克隆 & 构建**

```bash
git clone <your-repo-url> VoxStage
cd VoxStage

# 构建 workspace
cargo build
```

3. **运行 CLI 示例（从 `.vox` 脚本执行）**

- 推荐使用仓库自带的示例脚本：

```bash
# 基础示例：角色 + speak + sleep + 参数覆盖
cargo run -p vox-cli -- examples/hello.vox

# 变量与字符串插值示例
cargo run -p vox-cli -- examples/dialog.vox

# 控制流示例：if / for / while
cargo run -p vox-cli -- examples/control_flow.vox

# BGM 示例：背景音播放、音量、暂停/恢复/停止
cargo run -p vox-cli -- examples/bgm.vox
```

### 日志输出

CLI 支持通过 `--log-level` 控制日志级别：

```bash
cargo run -p vox-cli -- --log-level info  examples/hello.vox
cargo run -p vox-cli -- --log-level debug examples/hello.vox
cargo run -p vox-cli -- --log-level trace examples/hello.vox
```

可选值：`error` / `warn` / `info` / `debug` / `trace`。也可以通过环境变量 `RUST_LOG` 覆盖默认过滤规则。

  - 示例脚本说明：
  - `examples/hello.vox`：演示基础的 `model/role/speak/sleep`，以及不同语速的连续合成与播放。
  - `examples/dialog.vox`：演示 `let` 变量定义与 `${var}` 字符串插值构造简单对话。
  - `examples/control_flow.vox`：演示 `if` 条件分支、`for` 次数循环和 `while` 条件循环，包含在循环体内更新变量的逻辑。
  - `examples/bgm.vox`：演示 BGM 播放（`bgm "path"`）、音量（`bgm_volume`）、暂停/恢复/停止（`bgm_pause` / `bgm_resume` / `bgm_stop`）与 TTS 的配合。

---

## 后续规划（MVP → 完整架构）

- [ ] 在 `vox-engine` 中引入更完整的变量作用域（块级作用域）、赋值语句（如 `set`）与表达式求值。
- [ ] 为 `AudioStream` 增加流式模式，并在 `vox-audio` 中实现流式播放。
- [ ] 提供统一的模型预加载与健康检查机制（`preload()`）。
- [ ] 设计缓存层接口，用于复用常见文本/短句的合成结果。
- [ ] 预留插件式 Provider 接口，支持本地模型或外部脚本接入。

在当前阶段，项目已经完成了 **“单模型 + 单句文本 → HTTP 调用 → 播放”** 的主干闭环，并支持 **BGM 与 TTS 双轨播放、暂停/恢复/音量控制**；后续可以在此基础上不断向 DSL、多角色、多模型切换方向演进。 

