//! VoxStage CLI 入口。
//! 本程序：
//! 1. 读取 .vox 脚本，根据脚本中的 `model` 块注册 TTS Provider（type=http 时支持 bert_vits2 / gpt_sovits_v2）。
//! 2. 使用 vox-engine 执行脚本，通过 vox-runner 在本地设备上播放 TTS 与 BGM。

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use vox_dsl::ModelDef;
use vox_engine::{register_providers_from_script, ModelManager};
use vox_runner::run_script_with_audio;
use vox_tts_http::{BertVits2Config, BertVits2Provider, GptSovitsV2Config, GptSovitsV2Provider};

/// CLI 命令行参数定义。
#[derive(Debug, Parser)]
#[command(name = "vox-cli", about = "VoxStage DSL 驱动 TTS 示例 CLI")]
struct CliArgs {
    /// DSL 脚本文件路径（例如 examples/demo.vox）。
    /// 如果未提供，将使用内置示例脚本。
    #[arg(value_name = "SCRIPT_PATH")]
    script: Option<PathBuf>,

    /// 日志级别：error / warn / info / debug / trace
    #[arg(long, default_value = "info", value_name = "LEVEL")]
    log_level: String,
}

/// Tokio 异步入口函数。
/// 模型由脚本中的 model 块注册，无需在代码里写死。
#[tokio::main]
async fn main() {
    // 0. 解析命令行参数。
    let args = CliArgs::parse();

    // 0.1 初始化日志系统。
    init_logging(&args.log_level);

    // 1. 创建模型管理器（先为空，后续由脚本中的 model 块注册）。
    let mut manager = ModelManager::new();

    // 2. 确定要执行的 DSL 源码：优先使用命令行传入的脚本文件，其次回退到内置示例。
    let script_source = if let Some(path) = args.script {
        match fs::read_to_string(&path) {
            Ok(content) => {
                info!("从文件 `{}` 读取 DSL 脚本。", path.display());
                content
            }
            Err(err) => {
                warn!(
                    "读取脚本文件失败（{}），将使用内置示例脚本。错误: {err}",
                    path.display()
                );
                default_demo_script().to_string()
            }
        }
    } else {
        info!("未提供脚本路径，使用内置示例脚本。");
        default_demo_script().to_string()
    };

    // 3. 根据脚本中的 model 块注册 TTS Provider（在 .vox 里写 model xxx { type = "http", endpoint = "..." } 即可）。
    if let Err(err) = register_providers_from_script(&mut manager, &script_source, |def: &ModelDef| {
        model_def_to_provider(def)
    }) {
        tracing::error!("从脚本注册模型失败: {err:?}");
        return;
    }
    info!("已注册的模型数量: {}", manager.len());

    // 4. 调用 runner，将脚本交给引擎执行并在本地播放音频。
    if let Err(err) = run_script_with_audio(&manager, &script_source).await {
        tracing::error!("执行脚本失败: {err:?}");
    }
}

/// 根据脚本中的 model 块字段，创建对应的 TTS Provider。
/// 支持字段：type（默认 "http"）、endpoint、model_id、provider（"bert_vits2" | "gpt_sovits_v2"，默认 bert_vits2）。
fn model_def_to_provider(def: &ModelDef) -> Result<std::sync::Arc<dyn vox_core::TtsProvider>, String> {
    let typ = def.fields.get("type").map(String::as_str).unwrap_or("http");
    if typ != "http" {
        return Err(format!("不支持的 model type: {}", typ));
    }
    let endpoint = def
        .fields
        .get("endpoint")
        .cloned()
        .unwrap_or_else(|| "http://localhost:5000".to_string());
    let model_id = def
        .fields
        .get("model_id")
        .cloned()
        .unwrap_or_else(|| "0".to_string());
    let provider = def
        .fields
        .get("provider")
        .map(String::as_str)
        .unwrap_or("bert_vits2");

    match provider {
        "bert_vits2" => {
            let config = BertVits2Config {
                endpoint,
                model_id,
            };
            let p = BertVits2Provider::new(def.name.clone(), config);
            Ok(p.into_shared())
        }
        "gpt_sovits_v2" => {
            let config = GptSovitsV2Config {
                endpoint,
                model_id,
            };
            let p = GptSovitsV2Provider::new(def.name.clone(), config);
            Ok(p.into_shared())
        }
        _ => Err(format!("不支持的 provider: {}（可选: bert_vits2, gpt_sovits_v2）", provider)),
    }
}

/// 初始化日志输出。
/// - 默认输出到标准输出。
/// - 允许通过 `--log-level` 控制全局过滤级别。
/// - 用户也可以通过 `RUST_LOG` 覆盖（优先级更高）。
fn init_logging(level: &str) {
    // 若设置了 RUST_LOG，则优先使用它；否则使用 --log-level。
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
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

