//! VoxStage CLI 入口。
//! 本程序演示：
//! 1. 注册 Bert-VITS2 / GPT-SoVITS-v2 Provider（通过 HTTP 调用远程 TTS 服务）。
//! 2. 调用 Bert-VITS2 完成一次文本转语音。
//! 3. 使用系统默认音频设备播放生成的音频数据。

mod model_manager;

use std::collections::HashMap;

use model_manager::ModelManager;
use vox_audio::play_audio_blocking;
use vox_core::{AudioStream, SynthesisRequest, TtsProvider};
use vox_tts_http::{BertVits2Config, BertVits2Provider, GptSovitsV2Config, GptSovitsV2Provider};

/// Tokio 异步入口函数。
/// 在这里我们创建两个示例模型 Provider，并使用统一接口发起合成请求。
#[tokio::main]
async fn main() {
    // 1. 创建模型管理器，用于统一管理和切换 TTS Provider。
    let mut manager = ModelManager::new();

    // 2. 构建 Bert-VITS2 Provider 并注册。
    //    请将 endpoint 修改为你本地 Bert-VITS2 API 实际监听的地址。
    let bert_config = BertVits2Config {
        endpoint: "http://localhost:5000".to_string(),
        model_id: "0".to_string(),
    };
    let bert_provider = BertVits2Provider::new("bert_vits2", bert_config).into_shared();
    manager.register("bert_vits2", bert_provider);

    // 3. 构建 GPT-SoVITS-v2 Provider 并注册（当前仍为占位实现）。
    let gpt_config = GptSovitsV2Config {
        endpoint: "http://localhost:8002".to_string(),
        model_id: "gpt-sovits-v2-zh".to_string(),
    };
    let gpt_provider = GptSovitsV2Provider::new("gpt_sovits_v2", gpt_config).into_shared();
    manager.register("gpt_sovits_v2", gpt_provider);

    println!("已注册的模型数量: {}", manager.len());

    // 4. 构造一份通用的合成请求。
    let mut extra = HashMap::new();
    // 对 Bert-VITS2 而言，我们将 language、speaker_id 存入 extra，由 Provider 映射为 query 参数。
    extra.insert("language".to_string(), "ZH".to_string());
    extra.insert("speaker_id".to_string(), "0".to_string());

    let req = SynthesisRequest {
        text: "你好，这是来自 VoxStage 的 Bert-VITS2 测试。".to_string(),
        role: Some("Girl".to_string()),
        speed: Some(1.1),
        volume: Some(1.0),
        pitch: Some(1.0),
        emotion: Some("Neutral".to_string()),
        extra,
    };

    // 5. 使用 Bert-VITS2 模型进行一次合成，并在本地扬声器上播放结果。
    if let Some(provider) = manager.get("bert_vits2") {
        call_tts_and_play("bert_vits2", provider.as_ref(), req).await;
    } else {
        eprintln!("未找到名为 bert_vits2 的模型 Provider。");
    }
}

/// 通用的 TTS 调用封装函数。
/// 通过 `dyn TtsProvider` 接口接受任意模型，实现调用逻辑与具体模型解耦，
/// 并在成功时调用本地音频播放函数进行阻塞播放。
async fn call_tts_and_play(
    model_name: &str,
    provider: &dyn TtsProvider,
    req: SynthesisRequest,
) {
    println!("开始调用模型: {}", model_name);
    match provider.synthesize(req).await {
        Ok(AudioStream::Full(data)) => {
            println!("模型 {} 合成成功，开始播放音频……", model_name);
            if let Err(err) = play_audio_blocking(&data) {
                eprintln!("音频播放失败: {:?}", err);
            } else {
                println!("音频播放完成。");
            }
        }
        Ok(_) => {
            eprintln!("当前示例仅实现对完整音频数据的播放。");
        }
        Err(err) => {
            eprintln!("模型 {} 合成失败: {:?}", model_name, err);
        }
    }
}

