//! 音频播放模块（基础版）。
//! 这里使用 `rodio` 调用系统默认输出设备，将完整音频字节进行阻塞播放。
//! 后续可以在此之上扩展为带队列的 `AudioQueue`，支持多设备选择等。

use std::io::Cursor;

use rodio::{Decoder, OutputStream, Sink};

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
    // 1. 创建默认输出流和句柄。
    let (_stream, stream_handle) =
        OutputStream::try_default().map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;

    // 2. 构建一个音频“水槽”Sink，用于控制播放生命周期。
    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| AudioError::OutputStreamInitFailed(e.to_string()))?;

    // 3. 将字节数据包装成 `Cursor`，交给 `Decoder` 自动识别格式并解码为音频源。
    let cursor = Cursor::new(data.to_vec());
    let source = Decoder::new(cursor).map_err(|e| AudioError::DecodeFailed(e.to_string()))?;

    // 4. 将音频源附加到 Sink 上并开始播放。
    sink.append(source);

    // 5. 阻塞当前线程直到播放结束。
    sink.sleep_until_end();

    Ok(())
}

