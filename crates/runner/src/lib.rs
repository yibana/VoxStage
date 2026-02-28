//! VoxStage 运行器（runner）。
//! 将执行引擎产生的命令与音频播放模块串联起来，对外提供简化的“脚本 + 模型管理 → 播放”接口。

use std::path::Path;

use tokio::sync::mpsc;
use tokio::time::Duration;

use tracing::{error, info};
use vox_audio::PlaybackContext;
use vox_engine::{compile_script_to_channel, EngineCommand, EngineError, ModelManager};

/// 根据 path_or_url 加载音频字节。当前仅支持本地文件路径；以 `http://` / `https://` 开头的 URL 暂不支持。
fn load_audio_bytes(path_or_url: &str) -> Result<Vec<u8>, String> {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        return Err("BGM URL 暂不支持，请使用本地文件路径".to_string());
    }
    let path = Path::new(path_or_url);
    std::fs::read(path).map_err(|e| format!("读取 BGM 文件失败 {}: {}", path_or_url, e))
}

/// 使用给定的模型管理器和 DSL 源码执行脚本，并在本地设备上播放音频。
///
/// - 内部会：
///   1. 创建 BGM 控制器与命令通道；
///   2. 并发运行两个异步任务：
///      - producer：`compile_script_to_channel` 按顺序推入命令；
///      - consumer：即时消费命令，`SpeakAudio` 立刻开始播放、`Sleep` 控制间隔、BGM 命令转调 `BgmController`。
pub async fn run_script_with_audio(
    manager: &ModelManager,
    src: &str,
) -> Result<(), EngineError> {
    let (tx, mut rx) = mpsc::channel::<EngineCommand>(16);
    let ctx = PlaybackContext::try_new()
        .map_err(|e| EngineError::Audio(format!("播放上下文初始化失败: {:?}", e)))?;

    // producer：编译脚本为命令并推入通道。
    let producer = compile_script_to_channel(manager, src, tx);

    // consumer：消费命令并即时播放/控制（BGM 与 TTS 共用 ctx 的同一输出设备）。
    let consumer = async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                EngineCommand::SpeakAudio { model_name, data } => {
                    info!("模型 {} 合成完成，开始播放音频……", model_name);
                    match ctx.play_tts_blocking(&data) {
                        Ok(()) => info!("音频播放完成。"),
                        Err(err) => error!("音频播放失败: {:?}", err),
                    }
                }
                EngineCommand::Sleep { duration_ms } => {
                    info!("执行 sleep {} ms ……", duration_ms);
                    tokio::time::sleep(Duration::from_millis(duration_ms)).await;
                }
                EngineCommand::BgmPlay { path_or_url, r#loop } => {
                    match load_audio_bytes(&path_or_url) {
                        Ok(data) => {
                            if let Err(e) = ctx.bgm.play_bgm(data, r#loop) {
                                error!("BGM 播放失败: {:?}", e);
                            } else {
                                info!("BGM 开始播放 (loop={})", r#loop);
                            }
                        }
                        Err(e) => error!("BGM 加载失败: {}", e),
                    }
                }
                EngineCommand::BgmPause => {
                    ctx.bgm.pause_bgm();
                    info!("BGM 已暂停");
                }
                EngineCommand::BgmResume => {
                    ctx.bgm.resume_bgm();
                    info!("BGM 已恢复");
                }
                EngineCommand::BgmStop => {
                    ctx.bgm.stop_bgm();
                    info!("BGM 已停止");
                }
                EngineCommand::BgmVolume { volume } => {
                    ctx.bgm.set_bgm_volume(volume);
                    info!("BGM 音量设为 {}", volume);
                }
            }
        }

        Ok::<(), EngineError>(())
    };

    // 并发生产和消费命令：一边合成一边播放。
    tokio::try_join!(producer, consumer)?;

    Ok(())
}

