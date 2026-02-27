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

- **`vox-audio`（crates/audio）**  
  - 基于 `rodio` 的音频播放模块：
    - `play_audio_blocking(data: &[u8])`：使用系统默认输出设备播放一段完整音频数据，播放结束前阻塞。
    - `AudioError`：播放相关错误。
  - 未来会在此基础上扩展 `AudioQueue`、设备枚举与选择、多轨混音等能力。

- **`vox-cli`（crates/cli）**  
  - 命令行入口示例程序，用于将各层能力串联起来：
    - `ModelManager`：`HashMap<String, Arc<dyn TtsProvider>>` 管理所有已注册模型，支持 O(1) 切换。
    - `main.rs`：演示从文本 → TTS 模型（Bert-VITS2）→ 音频字节 → 本地播放 的完整流程。
  - 后续可以演化为：
    - 读取 DSL 脚本文件并交给执行引擎。
    - 提供命令行参数（选择角色/模型、指定输出设备等）。

依赖方向（自下而上）为：

```text
vox-core
  ├─> vox-tts-http
  ├─> vox-audio
  └─> vox-cli  (组合以上各层)
```

未来会在 `crates/` 下继续增加：

- `vox-dsl`：DSL 语法、AST 与解析器。
- `vox-engine`：执行引擎、Role Resolver、控制流解释器等。

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

- ✅ 基于 `rodio` 的最小播放实现（`vox-audio`）：
  - 使用系统默认输出设备。
  - 支持对一整段音频数据进行阻塞播放。
- ⏳ 计划中：
  - `AudioQueue`：维护播放队列，支持多个合成请求按顺序输出。
  - `AudioOutputManager`：音频设备枚举与选择。
  - 为流式音频 (`AudioStream::Streaming`) 预留接口。

### 3. 应用层 / 演示 CLI

- ✅ `vox-cli` 示例流程：
  1. 创建 `ModelManager`，注册：
     - `"bert_vits2"` → `BertVits2Provider`
     - `"gpt_sovits_v2"` → `GptSovitsV2Provider`（占位）
  2. 构造一份 `SynthesisRequest`：
     - 文本：示例中文句子。
     - 角色名：`"Girl"`（目前仅作为 hint）。
     - 参数：`speed` / `volume` / `pitch` / `emotion`。
     - `extra`：包括 `language="ZH"`、`speaker_id="0"` 等。
  3. 调用 `BertVits2Provider::synthesize` 获取 `AudioStream::Full`。
  4. 使用 `vox_audio::play_audio_blocking` 在本机扬声器播放结果。

- ⏳ 计划中：
  - 从配置文件（TOML/YAML）加载模型列表与参数。
  - 提供命令行参数选择模型、文本与输出设备。
  - 承接 DSL 与执行引擎，支持直接跑 `.vox` 或类似脚本。

### 4. DSL / 执行引擎（规划中）

当前还未创建对应 crate，设计目标已明确：

- `vox-dsl`：
  - 支持 `model` / `role` / `preset` / `let` / `speak` / 控制流 (`if/for/while`) / 字符串插值等语法。
  - 输出与执行无关的 AST。
- `vox-engine`：
  - 解释执行 DSL AST。
  - 管理角色与模型绑定、默认参数与覆盖参数。
  - 自动根据 `ModelCapabilities` 裁剪不支持的参数。
  - 将合成请求统一派发给 `TtsProvider`，并与音频队列对接。

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

3. **运行 CLI 示例**

```bash
cargo run -p vox-cli
```

如果一切正常，你将看到类似输出：

```text
已注册的模型数量: 2
开始调用模型: bert_vits2
[Bert-VITS2] GET http://localhost:5000/voice
[Bert-VITS2] received XXXXX bytes of audio data
模型 bert_vits2 合成成功，开始播放音频……
音频播放完成。
```

此时可以从系统默认音频输出设备听到合成语音。

---

## 后续规划（MVP → 完整架构）

- [ ] 新增 `vox-dsl` crate，完成 DSL 语法与 AST 设计。
- [ ] 新增 `vox-engine` crate，实现执行引擎、Role Resolver 与控制流解释。
- [ ] 为 `AudioStream` 增加流式模式，并在 `vox-audio` 中实现流式播放。
- [ ] 提供统一的模型预加载与健康检查机制（`preload()`）。
- [ ] 设计缓存层接口，用于复用常见文本/短句的合成结果。
- [ ] 预留插件式 Provider 接口，支持本地模型或外部脚本接入。

在当前阶段，项目已经完成了 **“单模型 + 单句文本 → HTTP 调用 → 播放”** 的主干闭环，后续可以在此基础上不断向 DSL、多角色、多模型切换方向演进。 

