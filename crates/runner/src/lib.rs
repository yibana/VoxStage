//! VoxStage 运行器（runner）。
//! 将执行引擎产生的命令与音频播放模块串联起来，对外提供简化的“脚本 + 模型管理 → 播放”接口。

use tokio::sync::mpsc;
use tokio::time::Duration;

use vox_audio::play_audio_blocking;
use vox_engine::{compile_script_to_channel, EngineCommand, EngineError, ModelManager};

/// 使用给定的模型管理器和 DSL 源码执行脚本，并在本地设备上播放音频。
///
/// - 内部会：
///   1. 创建一个 mpsc 队列；
///   2. 启动一个 producer 任务调用 `compile_script_to_channel` 将命令推入队列；
///   3. 在当前任务中消费命令，遇到 `SpeakAudio` 即播放，遇到 `Sleep` 则延迟。
pub async fn run_script_with_audio(
    manager: &ModelManager,
    src: &str,
) -> Result<(), EngineError> {
    // 创建命令通道：engine 作为生产者推送命令，本函数作为消费者依次处理。
    let (tx, mut rx) = mpsc::channel::<EngineCommand>(16);

    // 先在当前任务中以生产者身份将命令推入通道。
    // 注意：这里不会阻塞播放，因为真正的播放发生在后续消费环节，可以改造成并行生产/消费。
    compile_script_to_channel(manager, src, tx).await?;

    // 然后消费命令并播放音频。
    while let Some(cmd) = rx.recv().await {
        match cmd {
            EngineCommand::SpeakAudio { model_name, data } => {
                println!("模型 {} 合成完成，开始播放音频……", model_name);
                let res =
                    tokio::task::spawn_blocking(move || play_audio_blocking(&data)).await;
                match res {
                    Ok(Ok(())) => println!("音频播放完成。"),
                    Ok(Err(err)) => eprintln!("音频播放失败: {:?}", err),
                    Err(join_err) => eprintln!("音频播放任务崩溃: {:?}", join_err),
                }
            }
            EngineCommand::Sleep { duration_ms } => {
                println!("执行 sleep {} ms ……", duration_ms);
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
            }
        }
    }

    Ok(())
}

