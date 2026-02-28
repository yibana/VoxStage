//! HTTP 类 TTS Provider 实现集合。
//! 这里以 Bert-VITS2 和 GPT-SoVITS-v2 为例，给出最小可用实现。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use tracing::{debug, info};

use vox_core::{AudioStream, ModelCapabilities, SynthesisRequest, TtsError, TtsProvider};

/// Bert-VITS2 模型 Provider 的配置结构。
/// 实际项目中可以从配置文件或 DSL 的 `model` 定义中构造该结构。
#[derive(Debug, Clone)]
pub struct BertVits2Config {
    /// HTTP 服务的基础地址，例如 `"http://127.0.0.1:5000"`.
    pub endpoint: String,
    /// 模型名称或 ID，用于区分不同说话人/音色。
    pub model_id: String,
}

/// GPT-SoVITS-v2 模型 Provider 的配置结构。
#[derive(Debug, Clone)]
pub struct GptSovitsV2Config {
    /// HTTP 服务的基础地址，例如 `"http://localhost:8002"`.
    pub endpoint: String,
    /// 模型名称或 ID。
    pub model_id: String,
}

/// Bert-VITS2 Provider，实现 `TtsProvider` trait。
/// 这里给出一个最小可用的 HTTP 接入版本，会向本地 Bert-VITS2 服务发送请求并返回音频字节。
pub struct BertVits2Provider {
    /// Provider 的人类可读名称。
    name: String,
    /// 当前模型的能力声明。
    capabilities: ModelCapabilities,
    /// 基础配置（例如 HTTP endpoint、模型 ID 等）。
    config: BertVits2Config,
    /// 可复用的 HTTP 客户端实例，内部带有连接池。
    client: Client,
}

impl BertVits2Provider {
    /// 使用给定配置创建一个新的 Bert-VITS2 Provider 实例。
    pub fn new(name: impl Into<String>, config: BertVits2Config) -> Self {
        // 对于大多数 VITS 系列模型，通常支持 speed/volume/pitch 等基本控制。
        let mut custom = HashMap::new();
        custom.insert("family".to_string(), "Bert-VITS2".to_string());

        let capabilities = ModelCapabilities {
            supports_speed: true,
            supports_volume: true,
            supports_pitch: true,
            supports_emotion: false,
            supports_streaming: false,
            custom,
        };

        Self {
            name: name.into(),
            capabilities,
            config,
            client: Client::new(),
        }
    }

    /// 将 Provider 包装进 `Arc` 中，便于注册到 `ModelManager`。
    pub fn into_shared(self) -> Arc<dyn TtsProvider> {
        Arc::new(self)
    }
}

#[async_trait]
impl TtsProvider for BertVits2Provider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    async fn synthesize(
        &self,
        req: SynthesisRequest,
    ) -> Result<AudioStream, TtsError> {
        // 为了对齐示例，这里假定 Bert-VITS2 HTTP 接口为：
        //   GET http://127.0.0.1:5000/voice
        //   query 参数包含合成控制字段，返回值为音频字节流（如 WAV/PCM）。

        // 1. 准备基础参数集合。
        let mut params: Vec<(&str, String)> = Vec::new();

        // 自动分句、自动翻译等开关，这里给出与示例一致的默认值。
        params.push(("auto_split", "false".to_string()));
        params.push(("auto_translate", "false".to_string()));

        // 情感、语言等，如果在请求中给出则覆盖默认值。
        let emotion = req
            .emotion
            .clone()
            .unwrap_or_else(|| "Neutral".to_string());
        params.push(("emotion", emotion));

        let language = req
            .extra
            .get("language")
            .cloned()
            .unwrap_or_else(|| "ZH".to_string());
        params.push(("language", language));

        // length（时长倍率）这里用 speed 近似映射，如果没有则使用 1.1 作为默认值。
        let length = req.speed.unwrap_or(1.1);
        params.push(("length", format!("{:.6}", length)));

        // 模型 ID 和说话人 ID，目前直接从配置和默认值中获取，后续可由 DSL/角色系统控制。
        params.push(("model_id", self.config.model_id.clone()));
        let speaker_id = req
            .extra
            .get("speaker_id")
            .cloned()
            .unwrap_or_else(|| "0".to_string());
        params.push(("speaker_id", speaker_id));

        // 噪声、噪声音长、SDP 比例、风格权重等高阶参数，先给出一组与示例接近的默认值。
        params.push(("noise", "0.100000".to_string()));
        params.push(("noisew", "0.800000".to_string()));
        params.push(("sdp_ratio", "0.600000".to_string()));
        params.push(("style_weight", "0.700000".to_string()));

        // 合成文本，已经在上层完成字符串插值。
        params.push(("text", req.text.clone()));

        // 2. 构造完整 URL。
        let url = format!("{}/voice", self.config.endpoint.trim_end_matches('/'));

        debug!(url = %url, "Bert-VITS2 request");

        // 3. 使用 GET + query 方式发送请求并获取音频字节。
        let resp = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| TtsError::RemoteError(format!("http request error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(TtsError::RemoteError(format!(
                "unexpected status code: {}",
                status
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::RemoteError(format!("read body error: {e}")))?;

        info!(bytes = bytes.len(), "Bert-VITS2 audio received");

        Ok(AudioStream::Full(bytes.to_vec()))
    }
}

pub struct GptSovitsV2Provider {
    /// Provider 的人类可读名称。
    name: String,
    /// 当前模型的能力声明。
    capabilities: ModelCapabilities,
    /// 基础配置（例如 HTTP endpoint、模型 ID 等）。
    config: GptSovitsV2Config,
    /// 可复用的 HTTP 客户端实例。
    client: Client,
}

impl GptSovitsV2Provider {
    /// 使用给定配置创建一个新的 GPT-SoVITS-v2 Provider 实例。
    pub fn new(name: impl Into<String>, config: GptSovitsV2Config) -> Self {
        let mut custom = HashMap::new();
        custom.insert("family".to_string(), "GPT-SoVITS-v2".to_string());

        // 假定 GPT-SoVITS-v2 也支持 speed/volume/pitch，
        // 并且具有一定的情感表达能力。
        let capabilities = ModelCapabilities {
            supports_speed: true,
            supports_volume: true,
            supports_pitch: true,
            supports_emotion: true,
            supports_streaming: false,
            custom,
        };

        Self {
            name: name.into(),
            capabilities,
            config,
            client: Client::new(),
        }
    }

    /// 将 Provider 包装进 `Arc` 中，便于注册到 `ModelManager`。
    pub fn into_shared(self) -> Arc<dyn TtsProvider> {
        Arc::new(self)
    }
}

#[async_trait]
impl TtsProvider for GptSovitsV2Provider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    async fn synthesize(
        &self,
        req: SynthesisRequest,
    ) -> Result<AudioStream, TtsError> {
        // 参考示例：
        // GET http://127.0.0.1:9880/tts
        //   ?text=...
        //   &text_lang=zh
        //   &ref_audio_path=laugh1
        //   &prompt_lang=zh
        //   &prompt_text=...
        //   &text_split_method=cut5
        //   &batch_size=1
        //   &media_type=wav
        //   &streaming_mode=true

        // 1. 语言相关：优先使用 extra.text_lang，其次 extra.language，默认 zh。
        let raw_lang = req
            .extra
            .get("text_lang")
            .cloned()
            .or_else(|| req.extra.get("language").cloned())
            .unwrap_or_else(|| "zh".to_string());
        let text_lang = match raw_lang.to_ascii_lowercase().as_str() {
            "zh_cn" | "zh-cn" => "zh".to_string(),
            "en_us" | "en-us" => "en".to_string(),
            other => other.to_string(),
        };

        // prompt_lang 默认跟随 text_lang，也可以通过 extra.prompt_lang 覆盖。
        let prompt_lang = req
            .extra
            .get("prompt_lang")
            .cloned()
            .unwrap_or_else(|| text_lang.clone());

        // 2. 参考音频路径：优先使用 extra.ref_audio_path，其次 extra.speaker_id，再次使用 config.model_id。
        let ref_audio_path = req
            .extra
            .get("ref_audio_path")
            .cloned()
            .or_else(|| req.extra.get("speaker_id").cloned())
            .unwrap_or_else(|| self.config.model_id.clone());

        // 3. 提示文本：用于控制风格，来自 extra.prompt_text，默认为空字符串。
        let prompt_text = req
            .extra
            .get("prompt_text")
            .cloned()
            .unwrap_or_default();

        // 4. 其余参数：可通过 DSL 传入，提供合理默认值。
        let text_split_method = req
            .extra
            .get("text_split_method")
            .cloned()
            .unwrap_or_else(|| "cut5".to_string());

        let batch_size = req
            .extra
            .get("batch_size")
            .cloned()
            .unwrap_or_else(|| "1".to_string());

        let media_type = req
            .extra
            .get("media_type")
            .cloned()
            .unwrap_or_else(|| "wav".to_string());

        // 当前系统内部仍按一次性完整音频处理，streaming_mode 缺省为 false，
        // 如需让后端按流式返回，可在 DSL 中通过 extra.streaming_mode = "true" 显式设置。
        let streaming_mode = req
            .extra
            .get("streaming_mode")
            .cloned()
            .unwrap_or_else(|| "false".to_string());

        let mut params: Vec<(&str, String)> = Vec::new();
        params.push(("text", req.text.clone()));
        params.push(("text_lang", text_lang));
        params.push(("ref_audio_path", ref_audio_path));
        params.push(("prompt_lang", prompt_lang));
        params.push(("prompt_text", prompt_text));
        params.push(("text_split_method", text_split_method));
        params.push(("batch_size", batch_size));
        params.push(("media_type", media_type));
        params.push(("streaming_mode", streaming_mode));

        let url = format!("{}/tts", self.config.endpoint.trim_end_matches('/'));

        debug!(url = %url, "GPT-SoVITS-v2 request");

        let resp = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| TtsError::RemoteError(format!("http request error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(TtsError::RemoteError(format!(
                "unexpected status code: {}",
                status
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::RemoteError(format!("read body error: {e}")))?;

        info!(bytes = bytes.len(), "GPT-SoVITS-v2 audio received");

        Ok(AudioStream::Full(bytes.to_vec()))
    }
}

