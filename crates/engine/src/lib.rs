//! Vox 执行引擎（基础版）。
//! 负责：
//! - 从 DSL AST 中收集角色配置。
//! - 根据 `speak` / `sleep` 等语句顺序执行脚本。
//! - 调用已注册的 `TtsProvider` 完成合成，并返回或推送 `AudioStream`。
//!
//! 注意：本 crate 不依赖具体音频播放实现，也不管理输出设备。

mod model_manager;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use thiserror::Error;
use tokio::time::{sleep, Duration};
use tokio::sync::mpsc;
use tracing::{debug, error};
use vox_core::{AudioStream, SynthesisRequest, TtsError};
use vox_dsl::{parse_script, CondOp, IfCondition, Item, Script, SpeakStmt, SleepStmt};

pub use model_manager::ModelManager;

/// 执行引擎错误类型。
#[derive(Debug, Error)]
pub enum EngineError {
    /// DSL 解析错误。
    #[error("DSL parse error: {0}")]
    Parse(#[from] vox_dsl::ParseError),
    /// 角色未定义。
    #[error("unknown role: {0}")]
    UnknownRole(String),
    /// 模型 Provider 未注册。
    #[error("unknown model provider: {0}")]
    UnknownModel(String),
    /// 文本合成失败。
    #[error("synthesis failed: {0:?}")]
    Synthesis(TtsError),
    /// 音频/BGM 初始化或播放失败。
    #[error("audio/BGM: {0}")]
    Audio(String),
}

/// 角色运行时配置。
struct RoleRuntimeConfig {
    /// 绑定的模型名称（需与已注册 Provider 名称一致）。
    model: String,
    /// 默认参数表，例如 speed / language / speaker_id 等。
    params: HashMap<String, String>,
}

/// 执行时上下文：包含脚本本身、角色配置与变量表。
struct ExecContext {
    script: Script,
    roles: HashMap<String, RoleRuntimeConfig>,
    vars: HashMap<String, String>,
}

/// 从源码构建执行上下文：解析 DSL，并收集角色与变量定义。
fn build_exec_context(src: &str) -> Result<ExecContext, EngineError> {
    let script = parse_script(src)?;

    let mut roles: HashMap<String, RoleRuntimeConfig> = HashMap::new();
    let mut vars: HashMap<String, String> = HashMap::new();

    for item in &script.items {
        match item {
            Item::Role(role_def) => {
                roles.insert(
                    role_def.name.clone(),
                    RoleRuntimeConfig {
                        model: role_def.model.clone(),
                        params: role_def.params.clone(),
                    },
                );
            }
            Item::Let(let_stmt) => {
                vars.insert(let_stmt.name.clone(), let_stmt.value.clone());
            }
            _ => {}
        }
    }

    Ok(ExecContext {
        script,
        roles,
        vars,
    })
}

/// 运行一段 DSL 脚本：
/// 1. 使用 `vox-dsl` 解析为 AST。
/// 2. 从 AST 中提取角色配置。
/// 3. 顺序执行 `speak` 语句，调用模型完成合成。
///
/// 返回值为 `(模型名称, AudioStream)` 列表，调用者可以自行决定如何播放或保存音频。
pub async fn run_script_with_dsl(
    manager: &ModelManager,
    src: &str,
) -> Result<Vec<(String, AudioStream)>, EngineError> {
    let mut outputs = Vec::new();
    run_script_streaming(manager, src, |model_name, audio| {
        outputs.push((model_name, audio));
        async {}
    })
    .await?;

    Ok(outputs)
}

/// 以“流式”的方式运行脚本。
/// 每当遇到一条 `speak` 语句并成功合成音频时，就调用一次 `on_output` 回调；
/// `sleep` 会在引擎内部通过 `tokio::time::sleep` 延迟后续语句的执行。
pub async fn run_script_streaming<F, Fut>(
    manager: &ModelManager,
    src: &str,
    mut on_output: F,
) -> Result<(), EngineError>
where
    F: FnMut(String, AudioStream) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut ctx = build_exec_context(src)?;
    let items = ctx.script.items.clone();
    exec_items_streaming(manager, &mut ctx, &items, &mut on_output).await
}

/// 遍历并执行一组语句（streaming 版本）。
fn exec_items_streaming<'a, F, Fut>(
    manager: &'a ModelManager,
    ctx: &'a mut ExecContext,
    items: &'a [Item],
    on_output: &'a mut F,
) -> Pin<Box<dyn Future<Output = Result<(), EngineError>> + 'a>>
where
    F: FnMut(String, AudioStream) -> Fut,
    Fut: Future<Output = ()>,
{
    Box::pin(async move {
        for item in items {
            match item {
                Item::Model(_) => {}
                Item::Role(role_def) => {
                    ctx.roles.insert(
                        role_def.name.clone(),
                        RoleRuntimeConfig {
                            model: role_def.model.clone(),
                            params: role_def.params.clone(),
                        },
                    );
                }
                Item::Let(let_stmt) => {
                    ctx.vars
                        .insert(let_stmt.name.clone(), let_stmt.value.clone());
                }
                Item::Speak(speak) => {
                    let (model_name, audio) = execute_speak(manager, ctx, speak).await?;
                    on_output(model_name, audio).await;
                }
                Item::Sleep(stmt) => {
                    execute_sleep(stmt).await?;
                }
                Item::BgmPlay(_) | Item::BgmVolume(_) | Item::BgmPause | Item::BgmResume | Item::BgmStop => {
                    // 流式执行时 BGM 不产生音频输出，由命令模式 + runner 处理。
                }
                Item::If(if_stmt) => {
                    if eval_if_condition(&ctx.vars, &if_stmt.condition) {
                        exec_items_streaming(manager, ctx, &if_stmt.body, on_output).await?;
                    }
                }
                Item::For(for_stmt) => {
                    for _ in 0..for_stmt.times {
                        exec_items_streaming(manager, ctx, &for_stmt.body, on_output).await?;
                    }
                }
                Item::While(while_stmt) => {
                    while is_var_true(&ctx.vars, &while_stmt.var) {
                        exec_items_streaming(manager, ctx, &while_stmt.body, on_output).await?;
                    }
                }
            }
        }

        Ok(())
    })
}

/// 执行层命令枚举。
/// 这是从 DSL “预编译”后的结果，消费方拿到命令即可直接执行（如播放或进一步处理）。
#[derive(Debug)]
pub enum EngineCommand {
    /// 已经完成合成的一段音频数据，可以直接播放。
    SpeakAudio {
        model_name: String,
        data: Vec<u8>,
    },
    /// 执行级别的延迟（毫秒），通常用于拉开后续命令的时间。
    Sleep {
        duration_ms: u64,
    },
    /// 背景音：播放（path_or_url 由 runner 加载为字节后交给 audio）。
    BgmPlay {
        path_or_url: String,
        r#loop: bool,
    },
    /// 背景音：暂停 / 恢复 / 停止 / 音量。
    BgmPause,
    BgmResume,
    BgmStop,
    BgmVolume { volume: f32 },
}

/// 将脚本“编译”为一串顺序的执行命令（包含已合成的音频）。
/// - 对每个 `speak`：立即调用模型完成 TTS，将结果封装为 `SpeakAudio` 命令。
/// - 对每个 `sleep`：生成 `Sleep` 命令。
pub async fn compile_script_to_commands<F, Fut>(
    manager: &ModelManager,
    src: &str,
    mut on_command: F,
) -> Result<(), EngineError>
where
    F: FnMut(EngineCommand) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut ctx = build_exec_context(src)?;
    let items = ctx.script.items.clone();
    exec_items_to_commands(manager, &mut ctx, &items, &mut on_command).await
}

/// 遍历并执行一组语句（命令队列版本）。
fn exec_items_to_commands<'a, F, Fut>(
    manager: &'a ModelManager,
    ctx: &'a mut ExecContext,
    items: &'a [Item],
    on_command: &'a mut F,
) -> Pin<Box<dyn Future<Output = Result<(), EngineError>> + 'a>>
where
    F: FnMut(EngineCommand) -> Fut,
    Fut: Future<Output = ()>,
{
    Box::pin(async move {
        for item in items {
            match item {
                Item::Model(_) => {}
                Item::Role(role_def) => {
                    ctx.roles.insert(
                        role_def.name.clone(),
                        RoleRuntimeConfig {
                            model: role_def.model.clone(),
                            params: role_def.params.clone(),
                        },
                    );
                }
                Item::Let(let_stmt) => {
                    ctx.vars
                        .insert(let_stmt.name.clone(), let_stmt.value.clone());
                }
                Item::Speak(speak) => {
                    let (model_name, audio) = execute_speak(manager, ctx, speak).await?;
                    if let AudioStream::Full(data) = audio {
                        on_command(EngineCommand::SpeakAudio { model_name, data }).await;
                    }
                }
                Item::Sleep(stmt) => {
                    on_command(EngineCommand::Sleep {
                        duration_ms: stmt.duration_ms,
                    })
                    .await;
                }
                Item::BgmPlay(stmt) => {
                    on_command(EngineCommand::BgmPlay {
                        path_or_url: stmt.path_or_url.clone(),
                        r#loop: stmt.r#loop,
                    })
                    .await;
                }
                Item::BgmVolume(stmt) => {
                    on_command(EngineCommand::BgmVolume {
                        volume: stmt.volume,
                    })
                    .await;
                }
                Item::BgmPause => {
                    on_command(EngineCommand::BgmPause).await;
                }
                Item::BgmResume => {
                    on_command(EngineCommand::BgmResume).await;
                }
                Item::BgmStop => {
                    on_command(EngineCommand::BgmStop).await;
                }
                Item::If(if_stmt) => {
                    if eval_if_condition(&ctx.vars, &if_stmt.condition) {
                        exec_items_to_commands(manager, ctx, &if_stmt.body, on_command).await?;
                    }
                }
                Item::For(for_stmt) => {
                    for _ in 0..for_stmt.times {
                        exec_items_to_commands(manager, ctx, &for_stmt.body, on_command).await?;
                    }
                }
                Item::While(while_stmt) => {
                    while is_var_true(&ctx.vars, &while_stmt.var) {
                        exec_items_to_commands(manager, ctx, &while_stmt.body, on_command).await?;
                    }
                }
            }
        }

        Ok(())
    })
}

/// 将脚本编译后的命令推入 mpsc 队列。
pub async fn compile_script_to_channel(
    manager: &ModelManager,
    src: &str,
    sender: mpsc::Sender<EngineCommand>,
) -> Result<(), EngineError> {
    compile_script_to_commands(manager, src, move |cmd| {
        let sender = sender.clone();
        async move {
            // 如果接收端已关闭，则静默丢弃后续命令。
            let _ = sender.send(cmd).await;
        }
    })
    .await
}

/// 执行单条 `speak` 语句：根据角色找到对应 Provider，合成文本并返回音频流。
async fn execute_speak(
    manager: &ModelManager,
    ctx: &ExecContext,
    speak: &SpeakStmt,
) -> Result<(String, AudioStream), EngineError> {
    debug!(target = %speak.target, "execute speak");
    let role_cfg = ctx
        .roles
        .get(&speak.target)
        .ok_or_else(|| EngineError::UnknownRole(speak.target.clone()))?;

    let provider_name = &role_cfg.model;
    let provider = manager
        .get(provider_name)
        .ok_or_else(|| EngineError::UnknownModel(provider_name.clone()))?;

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

    let interpolated_text = interpolate_text(&speak.text, &ctx.vars);

    let req = SynthesisRequest {
        text: interpolated_text,
        role: Some(speak.target.clone()),
        speed,
        volume,
        pitch,
        emotion,
        extra,
    };

    let audio = provider
        .synthesize(req)
        .await
        .map_err(|e| {
            error!(error = ?e, provider = %provider_name, "tts synthesize failed");
            EngineError::Synthesis(e)
        })?;

    Ok((provider_name.clone(), audio))
}

/// 从角色和 speak 参数中获取某个浮点参数（speak 覆盖 role）。
fn get_param_f32(role: &RoleRuntimeConfig, speak: &SpeakStmt, key: &str) -> Option<f32> {
    // speak.params 目前只支持从 DSL 中通过 `(speed = 1.3, ...)` 语法写入字符串。
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

/// 执行一条 `sleep` 语句，在当前任务内延迟指定毫秒数。
async fn execute_sleep(stmt: &SleepStmt) -> Result<(), EngineError> {
    sleep(Duration::from_millis(stmt.duration_ms)).await;
    Ok(())
}

/// 判断变量是否为逻辑真（字符串值为 "true" 时，忽略大小写）。
fn is_var_true(vars: &HashMap<String, String>, name: &str) -> bool {
    vars.get(name)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 计算 if 条件是否为真。
fn eval_if_condition(vars: &HashMap<String, String>, cond: &IfCondition) -> bool {
    let current = vars.get(&cond.var).cloned().unwrap_or_default();
    match cond.op {
        CondOp::Eq => current == cond.value,
        CondOp::Neq => current != cond.value,
    }
}

/// 在 speak 文本中执行 `${var}` 风格的简单字符串插值。
fn interpolate_text(text: &str, vars: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            if let Some('{') = chars.peek().copied() {
                chars.next(); // 跳过 '{'
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next(); // 跳过 '}'
                        break;
                    } else {
                        name.push(c);
                        chars.next();
                    }
                }
                if let Some(value) = vars.get(&name) {
                    result.push_str(value);
                } else {
                    // 如果变量未定义，则保留原样。
                    result.push_str("${");
                    result.push_str(&name);
                    result.push('}');
                }
                continue;
            }
        }
        result.push(ch);
    }

    result
}

