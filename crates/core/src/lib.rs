//! 核心领域模型与公共 trait 定义。
//! 本模块与具体 HTTP 调用、音频播放实现解耦，只描述“文本转语音”的抽象接口。

use async_trait::async_trait;
use std::collections::HashMap;

/// 描述单个 TTS 模型在参数上的能力。
/// 执行引擎会根据这些能力信息裁剪不被支持的参数，避免向后端发送无效字段。
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    /// 是否支持语速调节（speed）。
    pub supports_speed: bool,
    /// 是否支持音量调节（volume）。
    pub supports_volume: bool,
    /// 是否支持音高调节（pitch）。
    pub supports_pitch: bool,
    /// 是否支持情感（emotion）控制。
    pub supports_emotion: bool,
    /// 是否支持流式输出音频。
    pub supports_streaming: bool,
    /// 模型自定义能力或配置键值对，供具体 Provider 自行解释。
    pub custom: HashMap<String, String>,
}

/// 一次合成请求的公共参数。
/// DSL、角色系统等最终会被归约为这样一份“干净”的请求结构，交由 TTS Provider 处理。
#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    /// 要合成的文本内容（已经完成字符串插值）。
    pub text: String,
    /// 角色名称提示，可用于模型选择说话人或音色。
    pub role: Option<String>,
    /// 语速倍率，1.0 为默认速度。
    pub speed: Option<f32>,
    /// 音量倍率，1.0 为默认音量。
    pub volume: Option<f32>,
    /// 音高倍率，1.0 为默认音高。
    pub pitch: Option<f32>,
    /// 情绪名称或标签，例如 "happy"、"sad"。
    pub emotion: Option<String>,
    /// 额外的自定义参数，具体含义由各个模型自己解释。
    pub extra: HashMap<String, String>,
}

/// 音频流的抽象表示。
/// MVP 阶段只返回一整块字节数据，后续可以扩展为真正的流式分片。
#[derive(Debug, Clone)]
pub enum AudioStream {
    /// 一次性返回的完整音频数据（例如编码后的 WAV / PCM 字节）。
    Full(Vec<u8>),
    // 未来可以在这里增加 Streaming 变体，例如携带 mpsc::Receiver<AudioChunk>。
}

/// TTS 相关错误的统一表示。
/// 为了最小实现，这里只区分几类大错误，后续可细化为带错误码的枚举。
#[derive(Debug)]
pub enum TtsError {
    /// 配置错误或必需参数缺失。
    InvalidConfig(String),
    /// 与远程服务交互失败（网络错误、状态码异常等）。
    RemoteError(String),
    /// 其他未分类错误。
    Other(String),
}

/// 所有具体 TTS 模型 Provider 必须实现的统一接口。
/// 通过这个 trait，执行引擎可以在不知道“背后是 Bert-VITS2 还是 GPT-SoVITS-v2”的情况下发起合成请求。
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// 返回该 Provider 的名称（通常对应一个逻辑模型 ID）。
    fn name(&self) -> &str;

    /// 返回该 Provider 的能力声明，用于参数裁剪与 UI 显示。
    fn capabilities(&self) -> &ModelCapabilities;

    /// 执行一次文本转语音合成。
    /// 这里使用 async fn，便于在内部执行 HTTP 调用或本地推理。
    async fn synthesize(
        &self,
        req: SynthesisRequest,
    ) -> Result<AudioStream, TtsError>;
}

