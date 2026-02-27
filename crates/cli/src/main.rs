//! VoxStage CLI 入口。
//! 本程序演示：
//! 1. 注册 Bert-VITS2 / GPT-SoVITS-v2 Provider（通过 HTTP 调用远程 TTS 服务）。
//! 2. 调用 Bert-VITS2 完成一次文本转语音。
//! 3. 使用系统默认音频设备播放生成的音频数据。

mod model_manager;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use clap::Parser;
use model_manager::ModelManager;
use vox_audio::play_audio_blocking;
use vox_core::{AudioStream, SynthesisRequest, TtsProvider};
use vox_tts_http::{BertVits2Config, BertVits2Provider, GptSovitsV2Config, GptSovitsV2Provider};
use vox_dsl::{parse_script, Item, Script, SpeakStmt};

/// CLI 命令行参数定义。
#[derive(Debug, Parser)]
#[command(name = "vox-cli", about = "VoxStage DSL 驱动 TTS 示例 CLI")]
struct CliArgs {
    /// DSL 脚本文件路径（例如 examples/demo.vox）。
    /// 如果未提供，将使用内置示例脚本。
    #[arg(value_name = "SCRIPT_PATH")]
    script: Option<PathBuf>,
}

/// Tokio 异步入口函数。
/// 在这里我们创建两个示例模型 Provider，并使用统一接口发起合成请求。
#[tokio::main]
async fn main() {
    // 0. 解析命令行参数。
    let args = CliArgs::parse();

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

    // 4. 确定要执行的 DSL 源码：优先使用命令行传入的脚本文件，其次回退到内置示例。
    let script_source = if let Some(path) = args.script {
        match fs::read_to_string(&path) {
            Ok(content) => {
                println!("从文件 `{}` 读取 DSL 脚本。", path.display());
                content
            }
            Err(err) => {
                eprintln!("读取脚本文件失败（{}），将使用内置示例脚本。错误: {err}", path.display());
                default_demo_script().to_string()
            }
        }
    } else {
        println!("未提供脚本路径，使用内置示例脚本。");
        default_demo_script().to_string()
    };

    if let Err(err) = run_script_with_dsl(&manager, &script_source).await {
        eprintln!("执行 DSL 脚本失败: {err}");
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

/// 返回一个内置的示例 DSL 脚本，用于未传入脚本路径或读取失败时的回退。
fn default_demo_script() -> &'static str {
    r#"
model bert_vits2 {
  type = "http"
  endpoint = "http://localhost:5000"
}

role Girl {
  model = bert_vits2
  speed = "1.1"
  language = "ZH"
  speaker_id = "0"
}

speak Girl "这是一个缺省的示例脚本"
"#
}

/// 运行一段 DSL 脚本：
/// 1. 使用 `vox-dsl` 解析为 AST。
/// 2. 从 AST 中提取角色配置。
/// 3. 顺序执行 `speak` 语句，使用角色绑定的模型与参数构造合成请求。
async fn run_script_with_dsl(manager: &ModelManager, src: &str) -> Result<(), String> {
    let script = parse_script(src).map_err(|e| e.to_string())?;

    // 1. 从 AST 收集角色配置。
    let mut roles: HashMap<String, RoleRuntimeConfig> = HashMap::new();
    for item in &script.items {
        if let Item::Role(role_def) = item {
            roles.insert(
                role_def.name.clone(),
                RoleRuntimeConfig {
                    model: role_def.model.clone(),
                    params: role_def.params.clone(),
                },
            );
        }
    }

    // 2. 顺序执行 speak 语句。
    for item in &script.items {
        if let Item::Speak(speak) = item {
            execute_speak(manager, &roles, &script, speak).await?;
        }
    }

    Ok(())
}

/// 用于运行时持有从 DSL `role` 定义中抽取的配置。
struct RoleRuntimeConfig {
    /// 绑定的模型名称（需与已注册 Provider 名称一致）。
    model: String,
    /// 默认参数表，例如 speed / language / speaker_id 等。
    params: HashMap<String, String>,
}

/// 执行单条 `speak` 语句：根据角色找到对应 Provider，合成文本并播放。
async fn execute_speak(
    manager: &ModelManager,
    roles: &HashMap<String, RoleRuntimeConfig>,
    _script: &Script,
    speak: &SpeakStmt,
) -> Result<(), String> {
    let role_cfg = roles
        .get(&speak.target)
        .ok_or_else(|| format!("未找到角色定义: {}", speak.target))?;

    let provider_name = &role_cfg.model;
    let provider = manager
        .get(provider_name)
        .ok_or_else(|| format!("未找到模型 Provider: {}", provider_name))?;

    // 从角色与 speak 参数中解析出最终合成参数（speak 参数将来可以覆盖角色参数）。
    let speed = get_param_f32(role_cfg, speak, "speed");
    let volume = get_param_f32(role_cfg, speak, "volume");
    let pitch = get_param_f32(role_cfg, speak, "pitch");
    let emotion = get_param_string(role_cfg, speak, "emotion");

    let mut extra = HashMap::new();
    if let Some(lang) = get_param_string(role_cfg, speak, "language") {
        extra.insert("language".to_string(), lang);
    }
    if let Some(speaker_id) = get_param_string(role_cfg, speak, "speaker_id") {
        extra.insert("speaker_id".to_string(), speaker_id);
    }

    let req = SynthesisRequest {
        text: speak.text.clone(),
        role: Some(speak.target.clone()),
        speed,
        volume,
        pitch,
        emotion,
        extra,
    };

    call_tts_and_play(provider_name, provider.as_ref(), req).await;
    Ok(())
}

/// 从角色和 speak 参数中获取某个浮点参数（speak 覆盖 role）。
fn get_param_f32(role: &RoleRuntimeConfig, speak: &SpeakStmt, key: &str) -> Option<f32> {
    // 未来 speak.params 可以实现参数覆盖，目前结构中已预留。
    if let Some(v) = speak.params.get(key) {
        v.parse().ok()
    } else if let Some(v) = role.params.get(key) {
        v.parse().ok()
    } else {
        None
    }
}

/// 从角色和 speak 参数中获取某个字符串参数（speak 覆盖 role）。
fn get_param_string(role: &RoleRuntimeConfig, speak: &SpeakStmt, key: &str) -> Option<String> {
    if let Some(v) = speak.params.get(key) {
        Some(v.clone())
    } else if let Some(v) = role.params.get(key) {
        Some(v.clone())
    } else {
        None
    }
}

