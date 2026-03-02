//! 核心领域模型与公共 trait 定义。
//! 本模块与具体 HTTP 调用、音频播放实现解耦，只描述“文本转语音”的抽象接口。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

/// 一个简单的基于内存的 TTS 结果缓存包装器。
/// 用于在同一进程生命周期内，对相同的合成请求复用音频字节，避免重复 HTTP 调用或本地推理。
#[derive(Clone)]
struct CacheKey {
    text: String,
    role: Option<String>,
    speed: Option<f32>,
    volume: Option<f32>,
    pitch: Option<f32>,
    emotion: Option<String>,
    /// extra 中的键值对，按 key/value 排序后存入，保证哈希稳定。
    extra: Vec<(String, String)>,
}

impl CacheKey {
    fn from_request(req: &SynthesisRequest) -> Self {
        let mut extra: Vec<(String, String)> = req
            .extra
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        extra.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        Self {
            text: req.text.clone(),
            role: req.role.clone(),
            speed: req.speed,
            volume: req.volume,
            pitch: req.pitch,
            emotion: req.emotion.clone(),
            extra,
        }
    }
}

impl std::hash::Hash for CacheKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.role.hash(state);
        // 对于可选的浮点字段，使用 to_bits 后再哈希，并区分 None / Some。
        self.speed
            .map(|v| v.to_bits())
            .unwrap_or(0)
            .hash(state);
        self.volume
            .map(|v| v.to_bits())
            .unwrap_or(0)
            .hash(state);
        self.pitch
            .map(|v| v.to_bits())
            .unwrap_or(0)
            .hash(state);
        self.emotion.hash(state);
        self.extra.hash(state);
    }
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.role == other.role
            && self
                .speed
                .map(|v| v.to_bits())
                .unwrap_or(0)
                == other.speed.map(|v| v.to_bits()).unwrap_or(0)
            && self
                .volume
                .map(|v| v.to_bits())
                .unwrap_or(0)
                == other.volume.map(|v| v.to_bits()).unwrap_or(0)
            && self
                .pitch
                .map(|v| v.to_bits())
                .unwrap_or(0)
                == other.pitch.map(|v| v.to_bits()).unwrap_or(0)
            && self.emotion == other.emotion
            && self.extra == other.extra
    }
}

impl Eq for CacheKey {}

/// 通用的缓存包装器，向外仍然暴露为 `Arc<dyn TtsProvider>`。
/// 内部使用一个简单的基于计数器的 LRU：每次命中会刷新“最近使用”时间，
/// 当缓存条目数超过上限时，淘汰最久未被访问的条目。
pub struct CachedTtsProvider {
    inner: Arc<dyn TtsProvider>,
    cache: Mutex<CacheInner>,
}

struct CacheInner {
    /// key -> (audio_bytes, last_used_counter)
    map: HashMap<CacheKey, (Vec<u8>, u64)>,
    next_counter: u64,
}

/// 单个 Provider 的最大缓存条目数。
/// 这是一个安全上限，用于防止缓存无限增长；可根据需要调整。
const MAX_CACHE_ENTRIES: usize = 256;

impl CachedTtsProvider {
    /// 用一个已存在的 Provider 创建带缓存的 Provider。
    /// 返回值直接是 `Arc<dyn TtsProvider>`，方便在现有代码中无缝替换。
    pub fn new(inner: Arc<dyn TtsProvider>) -> Arc<dyn TtsProvider> {
        Arc::new(Self {
            inner,
            cache: Mutex::new(CacheInner {
                map: HashMap::new(),
                next_counter: 0,
            }),
        })
    }
}

#[async_trait]
impl TtsProvider for CachedTtsProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn capabilities(&self) -> &ModelCapabilities {
        self.inner.capabilities()
    }

    async fn synthesize(
        &self,
        req: SynthesisRequest,
    ) -> Result<AudioStream, TtsError> {
        let key = CacheKey::from_request(&req);

        // 1. 先尝试从缓存命中（只缓存 Full 模式的字节流）。
        // 命中时更新最近使用计数，符合 LRU 语义。
        {
            let mut cache = self
                .cache
                .lock()
                .expect("CachedTtsProvider cache mutex poisoned");
            // 先生成新的时间戳，再尝试命中并刷新 last_used。
            let ts = cache.next_counter;
            cache.next_counter = cache.next_counter.wrapping_add(1);
            if let Some((bytes, last_used)) = cache.map.get_mut(&key) {
                *last_used = ts;
                return Ok(AudioStream::Full(bytes.clone()));
            }
        }

        // 2. 未命中则调用底层 Provider（不持有锁，避免长时间阻塞）。
        let result = self.inner.synthesize(req).await?;

        // 3. 仅当返回的是完整字节流时才写入缓存。
        if let AudioStream::Full(data) = &result {
            let mut cache = self
                .cache
                .lock()
                .expect("CachedTtsProvider cache mutex poisoned");

            // 如果已达到容量上限，先淘汰最久未使用的条目。
            if cache.map.len() >= MAX_CACHE_ENTRIES {
                // 找到 last_used 最小的条目并淘汰
                let evict_key_opt = {
                    let mut oldest: Option<(CacheKey, u64)> = None;
                    for (k, (_, ts)) in cache.map.iter() {
                        match oldest {
                            Some((_, old_ts)) if *ts >= old_ts => {}
                            _ => {
                                oldest = Some((k.clone(), *ts));
                            }
                        }
                    }
                    oldest.map(|(k, _)| k)
                };
                if let Some(evict_key) = evict_key_opt {
                    cache.map.remove(&evict_key);
                }
            }

            let ts = cache.next_counter;
            cache.next_counter = cache.next_counter.wrapping_add(1);
            cache.map.insert(key, (data.clone(), ts));
        }

        Ok(result)
    }
}


