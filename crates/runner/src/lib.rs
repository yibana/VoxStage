//! VoxStage 运行器（runner）。
//! 将执行引擎产生的命令与音频播放模块串联起来，对外提供简化的“脚本 + 模型管理 → 播放”接口。

use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::mpsc;
use tokio::time::Duration;

use tracing::{error, info};
use vox_audio::{play_audio_blocking, PlaybackContext};
use vox_engine::{
    compile_script_to_channel, EngineCommand, EngineCommandWithMeta, EngineError, ModelManager,
};

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
///   2. 并发运行 producer 与 consumer：producer 推命令，consumer 消费并播放。
///   TTS 在 `spawn_blocking` 中调用 `play_audio_blocking`，避免阻塞 consumer，从而 producer 可持续推命令。
pub async fn run_script_with_audio(
    manager: &ModelManager,
    src: &str,
    pause_flag: Option<Arc<AtomicBool>>,
    stop_flag: Option<Arc<AtomicBool>>,
    progress_cb: Option<Arc<dyn Fn(u32) + Send + Sync>>,
) -> Result<(), EngineError> {
    let (tx, mut rx) = mpsc::channel::<EngineCommandWithMeta>(16);
    let ctx = PlaybackContext::try_new()
        .map_err(|e| EngineError::Audio(format!("播放上下文初始化失败: {:?}", e)))?;

    // producer：编译脚本为命令并推入通道。
    let producer = compile_script_to_channel(manager, src, tx);

    // 预先克隆控制标志与进度回调供 consumer 使用。
    let pause_flag_consumer = pause_flag.clone();
    let stop_flag_consumer = stop_flag.clone();
    let progress_cb_consumer = progress_cb.clone();

    // consumer：消费命令并即时播放/控制（BGM 与 TTS 共用 ctx 的同一输出设备）。
    let consumer = async move {
        while let Some(enveloped) = rx.recv().await {
            let source_index = enveloped.source_index;
            let cmd = enveloped.command;

            // 若存在中断标志，优先检查：一旦被设置则立刻停止 BGM 并退出循环。
            if let Some(flag) = &stop_flag_consumer {
                if flag.load(Ordering::SeqCst) {
                    ctx.bgm.stop_bgm();
                    info!("收到停止标志，中断剩余命令执行。");
                    break;
                }
            }

            // 若存在暂停标志，则在每条命令前检查，处于暂停状态时阻塞在此。
            if let Some(flag) = &pause_flag_consumer {
                while flag.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }

            // 通知外界当前即将执行的命令对应的源索引（仅当不是占位值时）。
            if let Some(cb) = &progress_cb_consumer {
                if source_index != u32::MAX {
                    cb(source_index);
                }
            }

            match cmd {
                EngineCommand::SpeakAudio { model_name, data } => {
                    info!("模型 {} 合成完成，开始播放音频……", model_name);
                    // 在 spawn_blocking 中播放，避免阻塞 consumer 任务，否则 producer 无法继续推命令。
                    let data = data.clone();
                    let res = tokio::task::spawn_blocking(move || play_audio_blocking(&data)).await;
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

