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
    - `BertVits2Provider`（Bert-VITS2，GET `http://localhost:5000/voice`）。
    - `GptSovitsV2Provider`（GPT-SoVITS v2，GET `http://127.0.0.1:9880/tts`），支持：
      - `text` / `text_lang` / `prompt_text` / `prompt_lang`；
      - `ref_audio_path` / `batch_size` / `media_type` / `streaming_mode` 等参数（由 DSL 中的字段透传）。
  - 使用 `reqwest` 负责 HTTP 通信，将 `SynthesisRequest` 映射为 HTTP 请求，并返回 `AudioStream`。
  - 对上暴露 `TtsProvider` 接口，对下屏蔽具体 HTTP 细节。

- **`vox-dsl`（crates/dsl）**  
  - 脚本语言解析层：将 `.vox` 源码解析为 AST（`Script` / `Item` / `Expr` 等）。
  - 支持 `model` / `role` / `let` / `speak` / `sleep` / `if` / `for` / `while` 以及 **BGM 语句**（`bgm` / `bgm_volume` / `bgm_pause` / `bgm_resume` / `bgm_stop`），其中：
    - `let` / `if` / `for` / `while` 的右侧条件与次数均为统一的表达式（支持字面量、变量、算术、比较、逻辑与括号）。

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
    - 内置简单的表达式求值器：将 `Expr` 解析为运行时值（`Int` / `Bool` / `Str`），支撑 `let/if/for/while` 的基础运算（`+ - * / %`、比较、`&& || !`、括号）。
    - 支持根据脚本中的 `model` 块自动注册 Provider：`register_providers_from_script` 读取 `ModelDef`，由调用方工厂将 `type` / `provider` / `endpoint` / `model_id` 等字段映射到具体 `TtsProvider` 实例。
    - 构造 `SynthesisRequest`，调用对应的 `TtsProvider`，返回 `AudioStream` 或执行命令流。
    - `EngineCommand`：除 `SpeakAudio` / `Sleep` 外，包含 `BgmPlay` / `BgmPause` / `BgmResume` / `BgmStop` / `BgmVolume`，由 runner 消费并转调 `vox-audio`。
  - 不依赖任何具体 HTTP 实现或音频播放，只做“脚本 → 表达式求值 → TTS/BGM 命令”的编排。

- **`vox-runner`（crates/runner）**  
  - 运行器层，用于将执行引擎产出的命令流与音频播放串联起来：
    - 调用 `vox-engine::compile_script_to_channel` 将脚本编译为顺序的 `EngineCommand`（`SpeakAudio` / `Sleep` / BGM 相关）。
    - 使用 `tokio::mpsc` 将 engine（producer）与 runner（consumer）解耦，实现「一边合成一边播放」的流式执行。
    - 创建并持有 BGM 控制器，根据路径加载 BGM 文件（当前仅支持本地路径），在本地设备上播放 TTS 与 BGM，并按 `Sleep` 控制间隔。

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
- ✅ 实现 **GPT-SoVITS-v2 HTTP Provider**（`GptSovitsV2Provider`）：
  - 通过 **GET `http://127.0.0.1:9880/tts`** 调用本地 GPT-SoVITS v2 服务；
  - 支持 DSL 中通过 `role` / `speak` 的参数透传 `text_lang` / `prompt_text` / `prompt_lang` / `ref_audio_path` / `text_split_method` / `batch_size` / `media_type` / `streaming_mode` 等控制项；
  - 将返回体读为完整音频字节，封装为 `AudioStream::Full(Vec<u8>)`。

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
    - `model` 块：声明模型配置（`type` / `provider` / `endpoint` / `model_id` 等），由 CLI 通过 `register_providers_from_script` 自动注册 Provider。
    - `role` 块：绑定模型与默认参数（如 `speed` / `language` / `speaker_id` / GPT-SoVITS 的 `ref_audio_path` / `prompt_*` 等）。
    - `let` 语句：右侧是表达式，支持：
      - 字面量：数字、布尔（`true/false`）、字符串；
      - 变量引用：`foo`；
      - 基础运算：`+ - * / %`、比较（`== != < <= > >=`）、逻辑（`&& || !`）、括号。
      - 示例：`let score = base_score + bonus * 2`。
    - `speak` 语句：触发一次 TTS 调用，支持在括号中写覆盖参数：
      - `speak Girl "一句话"`
      - `speak Girl(speed = 1.3, language = "EN") "另一句话"`
      - 支持在文本中使用 `${var}` 字符串插值，例如：`speak Girl "你好，${user_name}"`。
    - `sleep` 语句：在执行过程插入延迟（毫秒），如 `sleep 1000`。
    - `if` 条件语句：条件为通用表达式，例如：
      - `if score >= 90 && lang == "ZH" { ... }`
      - `if !(flag == "off") { ... }`
    - `for` 次数循环：次数为表达式，例如：
      - `for 3 { ... }`
      - `for base_loop + extra { ... }`
    - `while` 条件循环：条件为表达式，例如：
      - `while keep_running { ... }`
      - `while i < max_loop && keep_running { ... }`
    - **BGM 语句**：
      - `bgm "path_or_url"` 或 `bgm "path" loop` / `bgm "path" once`：播放背景音（当前 runner 仅支持本地路径），支持在路径中使用 `${var}` 变量插值。
      - `bgm_volume 0.5`：设置 BGM 音量（1.0 为原始音量）。
      - `bgm_pause` / `bgm_resume` / `bgm_stop`：暂停、恢复、停止 BGM。
  - 输出独立于执行的 AST 结构（含 `Expr` / `BgmPlayStmt` / `BgmVolumeStmt` 等）。

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

# 控制流示例：if / for / while（旧版控制流）
cargo run -p vox-cli -- examples/control_flow.vox

# BGM 示例：背景音播放、音量、暂停/恢复/停止
cargo run -p vox-cli -- examples/bgm.vox

# GPT-SoVITS v2 全能力示例（含 BGM）
cargo run -p vox-cli -- examples/gpt_sovits_full.vox

# 混合使用 gpt_sovits_v2 与 bert_vits2 的示例
cargo run -p vox-cli -- examples/mix_gpt_bert.vox

# 表达式能力示例：let/if/for/while 使用 + - * /、比较、逻辑与括号
cargo run -p vox-cli -- examples/expr_demo.vox
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
  - `examples/control_flow.vox`：早期控制流示例，展示 `if` / `for` / `while` 的基本结构。
  - `examples/bgm.vox`：演示 BGM 播放（`bgm "path"`）、音量（`bgm_volume`）、暂停/恢复/停止（`bgm_pause` / `bgm_resume` / `bgm_stop`）与 TTS 的配合。
  - `examples/gpt_sovits_full.vox`：仅使用 `gpt_sovits_v2` 的完整示例，覆盖 BGM + GPT-SoVITS 常用参数。
  - `examples/mix_gpt_bert.vox`：同一脚本内混合使用 `bert_vits2` 与 `gpt_sovits_v2` 的示例。
  - `examples/expr_demo.vox`：演示表达式能力（`let/if/for/while` 中使用算术/比较/逻辑/括号），验证表达式求值行为。

---

## 后续规划（MVP → 完整架构）

- [ ] 在 `vox-engine` 中引入更完整的变量作用域（块级作用域）、赋值语句（如 `set`）与更丰富的表达式语法（当前已支持基础算术/比较/逻辑）。
- [ ] 为 `AudioStream` 增加流式模式，并在 `vox-audio` 中实现流式播放。
- [ ] 提供统一的模型预加载与健康检查机制（`preload()`）。
- [ ] 设计缓存层接口，用于复用常见文本/短句的合成结果。
- [ ] 预留插件式 Provider 接口，支持本地模型或外部脚本接入。

在当前阶段，项目已经完成了 **“单模型 + 单句文本 → HTTP 调用 → 播放”** 的主干闭环，并支持 **BGM 与 TTS 双轨播放、暂停/恢复/音量控制**；后续可以在此基础上不断向 DSL、多角色、多模型切换方向演进。 

