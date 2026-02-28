//! 音频播放模块（基础版）。
//! 这里使用 `rodio` 调用系统默认输出设备，将完整音频字节进行阻塞播放。
//! 支持 TTS 单次播放与 BGM 独立 Sink（循环/暂停/恢复/音量）。

use std::io::Cursor;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

/// 音频播放相关的错误类型。
#[derive(Debug)]
pub enum AudioError {
    /// 创建输出流失败（例如系统没有可用音频设备）。
    OutputStreamInitFailed(String),
    /// 无法解析音频数据（格式不被支持或数据损坏）。
    DecodeFailed(String),
}

/// 使用系统默认输出设备播放一段完整的音频数据，并在播放结束前阻塞当前线程。
///
/// - `data` 通常是从 TTS 服务返回的一段完整音频二进制数据（如 WAV）。
/// - 本函数会尝试自动识别音频格式（由 `rodio::Decoder` 完成）。
pub fn play_audio_blocking(data: &[u8]) -> Result<(), AudioError> {
    let (_stream, stream_handle) =
        OutputStream::try_default().map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;

    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;

    let cursor = Cursor::new(data.to_vec());
    let source = Decoder::new(cursor).map_err(|e| AudioError::DecodeFailed(e.to_string()))?;

    sink.append(source);
    sink.sleep_until_end();

    Ok(())
}

// ---------------------------------------------------------------------------
// BGM 控制：独立 Sink，支持循环、暂停、恢复、音量、停止
// ---------------------------------------------------------------------------

/// BGM 控制器：持有专用 Sink，用于背景音播放、暂停、恢复与音量。
/// 可与 TTS 共用同一 OutputStream（通过 `PlaybackContext` 创建），避免多路输出导致无声。
pub struct BgmController {
    _stream_handle: OutputStreamHandle,
    bgm_sink: Sink,
}

impl BgmController {
    /// 使用已有的输出句柄创建 BGM 控制器（与 TTS 共用同一设备时使用）。
    pub fn new(stream_handle: OutputStreamHandle) -> Result<Self, AudioError> {
        let bgm_sink = Sink::try_new(&stream_handle)
            .map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;
        Ok(Self {
            _stream_handle: stream_handle,
            bgm_sink,
        })
    }

    /// 创建 BGM 控制器，并单独占用系统默认输出设备（不推荐与 TTS 混用时使用）。
    pub fn try_new() -> Result<Self, AudioError> {
        let (_stream, stream_handle) =
            OutputStream::try_default().map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;
        Self::new(stream_handle)
    }

    /// 判断是否为 MP3 魔数（ID3 或帧同步），用于决定是否使用 repeat_infinite。
    fn is_mp3(data: &[u8]) -> bool {
        data.starts_with(b"ID3")
            || (data.len() >= 2 && data[0] == 0xFF && (data[1] & 0xE0) == 0xE0)
    }

    /// 播放 BGM。若当前有在播，会先清空再播放。
    /// - `data`: 完整音频字节（WAV/MP3 等，由 Decoder 识别）。
    /// - `r#loop`: 是否循环播放。MP3 格式当前不使用 repeat_infinite，仅播放一遍，避免 rodio 下无声。
    pub fn play_bgm(&self, data: Vec<u8>, r#loop: bool) -> Result<(), AudioError> {
        let is_mp3 = Self::is_mp3(&data);
        self.bgm_sink.clear();
        let cursor = Cursor::new(data);
        let decoder = Decoder::new(cursor).map_err(|e| AudioError::DecodeFailed(e.to_string()))?;
        if r#loop && !is_mp3 {
            self.bgm_sink.append(decoder.repeat_infinite());
        } else {
            self.bgm_sink.append(decoder);
        }
        self.bgm_sink.play();
        Ok(())
    }

    /// 暂停 BGM（可随后用 `resume_bgm` 恢复）。
    pub fn pause_bgm(&self) {
        self.bgm_sink.pause();
    }

    /// 恢复已暂停的 BGM。
    pub fn resume_bgm(&self) {
        self.bgm_sink.play();
    }

    /// 停止 BGM：清空队列并暂停。
    pub fn stop_bgm(&self) {
        self.bgm_sink.clear();
    }

    /// 设置 BGM 音量，1.0 为原始音量。
    pub fn set_bgm_volume(&self, volume: f32) {
        self.bgm_sink.set_volume(volume);
    }
}

// ---------------------------------------------------------------------------
// 统一播放上下文：BGM 与 TTS 共用同一 OutputStream，避免多路输出导致 BGM 无声
// ---------------------------------------------------------------------------

/// 统一播放上下文：持有一个默认输出流，BGM 与 TTS 共用该设备，避免部分系统上 BGM 无声。
pub struct PlaybackContext {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    pub bgm: BgmController,
}

impl PlaybackContext {
    /// 创建播放上下文（BGM + TTS 共用同一输出设备）。
    pub fn try_new() -> Result<Self, AudioError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;
        let bgm = BgmController::new(stream_handle.clone())?;
        Ok(Self {
            _stream: stream,
            stream_handle,
            bgm,
        })
    }

    /// 使用当前输出设备阻塞播放一段 TTS 音频（与 BGM 同设备）。
    pub fn play_tts_blocking(&self, data: &[u8]) -> Result<(), AudioError> {
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;
        let cursor = Cursor::new(data.to_vec());
        let source = Decoder::new(cursor).map_err(|e| AudioError::DecodeFailed(e.to_string()))?;
        sink.append(source);
        sink.sleep_until_end();
        Ok(())
    }
}

