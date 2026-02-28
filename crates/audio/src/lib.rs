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

/// BGM 控制器：持有默认输出流与专用 Sink，用于背景音播放、暂停、恢复与音量。
pub struct BgmController {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    bgm_sink: Sink,
}

impl BgmController {
    /// 创建 BGM 控制器，使用系统默认输出设备。
    pub fn try_new() -> Result<Self, AudioError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;
        let bgm_sink =
            Sink::try_new(&stream_handle).map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;
        Ok(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            bgm_sink,
        })
    }

    /// 播放 BGM。若当前有在播，会先清空再播放。
    /// - `data`: 完整音频字节（WAV/MP3 等，由 Decoder 识别）。
    /// - `r#loop`: 是否循环播放。
    pub fn play_bgm(&self, data: Vec<u8>, r#loop: bool) -> Result<(), AudioError> {
        self.bgm_sink.clear();
        let cursor = Cursor::new(data);
        let decoder = Decoder::new(cursor).map_err(|e| AudioError::DecodeFailed(e.to_string()))?;
        if r#loop {
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

