//! VoxStage CLI 入口。
//! 本程序演示：
//! 1. 注册 Bert-VITS2 / GPT-SoVITS-v2 Provider（通过 HTTP 调用远程 TTS 服务）。
//! 2. 使用 vox-dsl 解析脚本，并交由 vox-engine 执行。
//! 3. 通过 vox-runner 将执行结果中的音频数据在本地设备上播放。

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use vox_engine::ModelManager;
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
                eprintln!(
                    "读取脚本文件失败（{}），将使用内置示例脚本。错误: {err}",
                    path.display()
                );
                default_demo_script().to_string()
            }
        }
    } else {
        println!("未提供脚本路径，使用内置示例脚本。");
        default_demo_script().to_string()
    };

    // 5. 调用 runner，将脚本交给引擎执行并在本地播放音频。
    if let Err(err) = run_script_with_audio(&manager, &script_source).await {
        eprintln!("执行脚本失败: {err:?}");
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

