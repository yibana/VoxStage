//! VoxStage 运行器（runner）。
//! 将执行引擎产生的命令与音频播放模块串联起来，对外提供简化的“脚本 + 模型管理 → 播放”接口。

use std::path::Path;

use tokio::sync::mpsc;
use tokio::time::Duration;

use tracing::{error, info};
use vox_audio::{play_audio_blocking, BgmController};
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
///   2. 调用 `compile_script_to_channel` 将命令推入队列；
///   3. 消费命令：`SpeakAudio` 播放 TTS，`Sleep` 延迟，BGM 命令转调 `BgmController`。
pub async fn run_script_with_audio(
    manager: &ModelManager,
    src: &str,
) -> Result<(), EngineError> {
    let (tx, mut rx) = mpsc::channel::<EngineCommand>(16);
    let bgm = BgmController::try_new()
        .map_err(|e| EngineError::Audio(format!("BGM 控制器初始化失败: {:?}", e)))?;

    compile_script_to_channel(manager, src, tx).await?;

    while let Some(cmd) = rx.recv().await {
        match cmd {
            EngineCommand::SpeakAudio { model_name, data } => {
                info!("模型 {} 合成完成，开始播放音频……", model_name);
                let res =
                    tokio::task::spawn_blocking(move || play_audio_blocking(&data)).await;
                match res {
                    Ok(Ok(())) => info!("音频播放完成。"),
                    Ok(Err(err)) => error!("音频播放失败: {:?}", err),
                    Err(join_err) => error!("音频播放任务崩溃: {:?}", join_err),
                }
            }
            EngineCommand::Sleep { duration_ms } => {
                info!("执行 sleep {} ms ……", duration_ms);
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
            }
            EngineCommand::BgmPlay { path_or_url, r#loop } => {
                match load_audio_bytes(&path_or_url) {
                    Ok(data) => {
                        if let Err(e) = bgm.play_bgm(data, r#loop) {
                            error!("BGM 播放失败: {:?}", e);
                        } else {
                            info!("BGM 开始播放 (loop={})", r#loop);
                        }
                    }
                    Err(e) => error!("BGM 加载失败: {}", e),
                }
            }
            EngineCommand::BgmPause => {
                bgm.pause_bgm();
                info!("BGM 已暂停");
            }
            EngineCommand::BgmResume => {
                bgm.resume_bgm();
                info!("BGM 已恢复");
            }
            EngineCommand::BgmStop => {
                bgm.stop_bgm();
                info!("BGM 已停止");
            }
            EngineCommand::BgmVolume { volume } => {
                bgm.set_bgm_volume(volume);
                info!("BGM 音量设为 {}", volume);
            }
        }
    }

    Ok(())
}

