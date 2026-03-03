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
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, Timelike};
use rand::random;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use tokio::sync::mpsc;
use tracing::{debug, error};
use vox_core::{AudioStream, SynthesisRequest, TtsError, TtsProvider};
use vox_dsl::{parse_expr, parse_script, Expr, Item, ModelDef, Script, SetStmt, SpeakStmt, SleepStmt};

pub use model_manager::ModelManager;

/// 根据脚本中的 `model` 块，通过调用方提供的工厂函数创建 Provider 并注册到 `ModelManager`。
/// 这样即可在 .vox 脚本里用 `model xxx { type = "http", endpoint = "..." }` 声明模型，由调用方（如 CLI）根据 `type` / `provider` 等字段实例化具体实现。
///
/// - `factory`: 对每个 `ModelDef` 调用一次，返回 `Ok(Arc<dyn TtsProvider>)` 即注册到 `def.name`；返回 `Err` 则中止并返回错误。
pub fn register_providers_from_script<F, E>(
    manager: &mut ModelManager,
    src: &str,
    mut factory: F,
) -> Result<(), EngineError>
where
    F: FnMut(&ModelDef) -> Result<Arc<dyn TtsProvider>, E>,
    E: std::fmt::Display,
{
    let script = parse_script(src)?;
    for item in &script.items {
        if let Item::Model(def) = item {
            let provider = factory(def).map_err(|e| EngineError::ModelRegistration(e.to_string()))?;
            manager.register(def.name.clone(), provider);
        }
    }
    Ok(())
}

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
    /// 根据脚本 model 块注册 Provider 时失败。
    #[error("model registration: {0}")]
    ModelRegistration(String),
    /// 文本插值错误（`${...}` 表达式解析或匹配失败等）。
    #[error("interpolation error: {0}")]
    Interpolation(String),
}

/// 角色运行时配置。
struct RoleRuntimeConfig {
    /// 绑定的模型名称（需与已注册 Provider 名称一致）。
    model: String,
    /// 默认参数表，例如 speed / language / speaker_id 等。
    params: HashMap<String, String>,
}

/// 执行时上下文：包含脚本本身、角色配置、变量表与语句索引映射。
struct ExecContext {
    script: Script,
    roles: HashMap<String, RoleRuntimeConfig>,
    vars: HashMap<String, String>,
    /// 静态语句索引表：将 AST 中的特定语句映射到 source_index（从 0 开始）。
    /// - 仅对会产生 EngineCommand 的语句（speak/sleep）赋值；
    /// - 同一条语句在循环中多次执行时复用同一个索引。
    source_index_map: HashMap<*const Item, u32>,
}

/// 表达式求值后得到的运行时值（仅在执行引擎内部使用）。
#[derive(Debug, Clone)]
enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

/// 从源码构建执行上下文：解析 DSL，并收集角色与变量定义。
fn build_exec_context(src: &str) -> Result<ExecContext, EngineError> {
    let script = parse_script(src)?;

    // 为命令语句分配静态 source_index。
    let mut source_index_map: HashMap<*const Item, u32> = HashMap::new();
    let mut next_index: u32 = 0;
    assign_source_indices(&script.items, &mut next_index, &mut source_index_map);

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
                let value = eval_expr(&let_stmt.expr, &vars);
                vars.insert(let_stmt.name.clone(), value_to_string(&value));
            }
            Item::Set(set_stmt) => {
                // 顶层 set 直接作用于“全局变量表”。
                let value = eval_expr(&set_stmt.expr, &vars);
                vars.insert(set_stmt.name.clone(), value_to_string(&value));
            }
            _ => {}
        }
    }

    Ok(ExecContext {
        script,
        roles,
        vars,
        source_index_map,
    })
}

/// 为脚本中的语句分配静态 source_index，仅对 speak/sleep 语句赋值。
fn assign_source_indices(
    items: &[Item],
    next_index: &mut u32,
    map: &mut HashMap<*const Item, u32>,
) {
    for item in items {
        match item {
            Item::Speak(_) | Item::Sleep(_) => {
                let ptr: *const Item = item;
                map.insert(ptr, *next_index);
                *next_index += 1;
            }
            Item::If(stmt) => {
                assign_source_indices(&stmt.body, next_index, map);
            }
            Item::For(stmt) => {
                assign_source_indices(&stmt.body, next_index, map);
            }
            Item::While(stmt) => {
                assign_source_indices(&stmt.body, next_index, map);
            }
            _ => {}
        }
    }
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
                    let value = eval_expr(&let_stmt.expr, &ctx.vars);
                    ctx.vars
                        .insert(let_stmt.name.clone(), value_to_string(&value));
                }
                Item::Set(set_stmt) => {
                    apply_set(&mut ctx.vars, set_stmt);
                }
                Item::Set(set_stmt) => {
                    apply_set(&mut ctx.vars, set_stmt);
                }
                Item::Set(set_stmt) => {
                    apply_set(&mut ctx.vars, set_stmt);
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
                    let cond = eval_expr(&if_stmt.condition, &ctx.vars);
                    if value_to_bool(&cond) {
                        exec_items_streaming(manager, ctx, &if_stmt.body, on_output).await?;
                    }
                }
                Item::For(for_stmt) => {
                    let times_val = eval_expr(&for_stmt.times, &ctx.vars);
                    let times = value_to_u64(&times_val);
                    for _ in 0..times {
                        exec_items_streaming(manager, ctx, &for_stmt.body, on_output).await?;
                    }
                }
                Item::While(while_stmt) => {
                    while value_to_bool(&eval_expr(&while_stmt.condition, &ctx.vars)) {
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
#[derive(Debug, Clone)]
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

/// 带有来源索引的命令封装。
/// - `source_index`：静态脚本中的语句序号（从 0 开始），同一条语句在循环中多次执行会重复使用同一个索引。
#[derive(Debug, Clone)]
pub struct EngineCommandWithMeta {
    pub source_index: u32,
    pub command: EngineCommand,
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
    F: FnMut(EngineCommandWithMeta) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut ctx = build_exec_context(src)?;
    // 通过裸指针获取对脚本 items 的只读切片，以避免同时对 ctx.script 与 ctx 产生冲突借用。
    let items_ptr: *const [Item] = &ctx.script.items[..];
    let items: &[Item] = unsafe { &*items_ptr };
    exec_items_to_commands(manager, &mut ctx, items, &mut on_command).await
}

/// 遍历并执行一组语句（命令队列版本）。
fn exec_items_to_commands<'a, F, Fut>(
    manager: &'a ModelManager,
    ctx: &'a mut ExecContext,
    items: &'a [Item],
    on_command: &'a mut F,
) -> Pin<Box<dyn Future<Output = Result<(), EngineError>> + 'a>>
where
    F: FnMut(EngineCommandWithMeta) -> Fut,
    Fut: Future<Output = ()>,
{
    Box::pin(async move {
        for item in items {
            let ptr: *const Item = item;
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
                    let value = eval_expr(&let_stmt.expr, &ctx.vars);
                    ctx.vars
                        .insert(let_stmt.name.clone(), value_to_string(&value));
                }
                Item::Set(set_stmt) => {
                    apply_set(&mut ctx.vars, set_stmt);
                }
                Item::Speak(speak) => {
                    let (model_name, audio) = execute_speak(manager, ctx, speak).await?;
                    if let AudioStream::Full(data) = audio {
                        let idx = *ctx
                            .source_index_map
                            .get(&ptr)
                            .unwrap_or(&u32::MAX);
                        on_command(EngineCommandWithMeta {
                            source_index: idx,
                            command: EngineCommand::SpeakAudio { model_name, data },
                        })
                        .await;
                    }
                }
                Item::Sleep(stmt) => {
                    let idx = *ctx
                        .source_index_map
                        .get(&ptr)
                        .unwrap_or(&u32::MAX);
                    on_command(EngineCommandWithMeta {
                        source_index: idx,
                        command: EngineCommand::Sleep {
                            duration_ms: stmt.duration_ms,
                        },
                    })
                    .await;
                }
                Item::BgmPlay(stmt) => {
                    // 支持在 BGM 路径中使用 `${...}` 表达式插值，例如：bgm \"${bgm_path}\" loop
                    let path = interpolate_text(&stmt.path_or_url, &ctx.vars)?;
                    on_command(EngineCommandWithMeta {
                        // BGM 命令当前不会在 GUI 中高亮，进度索引使用占位值。
                        source_index: u32::MAX,
                        command: EngineCommand::BgmPlay {
                            path_or_url: path,
                            r#loop: stmt.r#loop,
                        },
                    })
                    .await;
                }
                Item::BgmVolume(stmt) => {
                    on_command(EngineCommandWithMeta {
                        source_index: u32::MAX,
                        command: EngineCommand::BgmVolume {
                            volume: stmt.volume,
                        },
                    })
                    .await;
                }
                Item::BgmPause => {
                    on_command(EngineCommandWithMeta {
                        source_index: u32::MAX,
                        command: EngineCommand::BgmPause,
                    })
                    .await;
                }
                Item::BgmResume => {
                    on_command(EngineCommandWithMeta {
                        source_index: u32::MAX,
                        command: EngineCommand::BgmResume,
                    })
                    .await;
                }
                Item::BgmStop => {
                    on_command(EngineCommandWithMeta {
                        source_index: u32::MAX,
                        command: EngineCommand::BgmStop,
                    })
                    .await;
                }
                Item::If(if_stmt) => {
                    let cond = eval_expr(&if_stmt.condition, &ctx.vars);
                    if value_to_bool(&cond) {
                        exec_items_to_commands(manager, ctx, &if_stmt.body, on_command).await?;
                    }
                }
                Item::For(for_stmt) => {
                    let times_val = eval_expr(&for_stmt.times, &ctx.vars);
                    let times = value_to_u64(&times_val);
                    for _ in 0..times {
                        exec_items_to_commands(manager, ctx, &for_stmt.body, on_command).await?;
                    }
                }
                Item::While(while_stmt) => {
                    while value_to_bool(&eval_expr(&while_stmt.condition, &ctx.vars)) {
                        exec_items_to_commands(manager, ctx, &while_stmt.body, on_command).await?;
                    }
                }
                Item::BgmPlay(_) | Item::BgmVolume(_) | Item::BgmPause | Item::BgmResume | Item::BgmStop => {
                    // 已在上方分支处理，这里仅为保持匹配完整性。
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
    sender: mpsc::Sender<EngineCommandWithMeta>,
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

    // 其余参数原样透传到 extra，便于 Provider 使用自定义字段（如 GPT-SoVITS-v2 的 ref_audio_path 等）。
    const RESERVED_KEYS: &[&str] = &["speed", "volume", "pitch", "emotion", "language", "speaker_id"];
    for (k, v) in &role_cfg.params {
        if !RESERVED_KEYS.contains(&k.as_str()) && !extra.contains_key(k) {
            extra.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &speak.params {
        if !RESERVED_KEYS.contains(&k.as_str()) {
            extra.insert(k.clone(), v.clone());
        }
    }

    let interpolated_text = interpolate_text(&speak.text, &ctx.vars)?;

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

/// 将运行时值转为字符串，供变量表或日志使用。
fn value_to_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
    }
}

/// 将运行时值转换为布尔，用于 if / while 条件判断。
fn value_to_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Str(s) => s.eq_ignore_ascii_case("true"),
    }
}

/// 将运行时值转换为循环次数（负数或无法解析时视为 0）。
fn value_to_u64(v: &Value) -> u64 {
    match v {
        Value::Int(i) if *i > 0 => *i as u64,
        Value::Int(_) => 0,
        Value::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        Value::Str(s) => s.parse::<u64>().unwrap_or(0),
    }
}

/// 将字符串解析为运行时值。
fn parse_literal(s: &str) -> Value {
    // 布尔
    if s.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    // 整数
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    // 其它一律当作字符串
    Value::Str(s.to_string())
}

/// 表达式求值。
fn eval_expr(expr: &Expr, vars: &HashMap<String, String>) -> Value {
    match expr {
        Expr::Literal(s) => parse_literal(s),
        Expr::StrLiteral(s) => Value::Str(s.clone()),
        Expr::Var(name) => {
            if let Some(v) = vars.get(name) {
                parse_literal(v)
            } else {
                Value::Str(String::new())
            }
        }
        Expr::Unary { op, expr } => {
            let v = eval_expr(expr, vars);
            match op {
                vox_dsl::UnaryOp::Not => Value::Bool(!value_to_bool(&v)),
                vox_dsl::UnaryOp::Neg => match v {
                    Value::Int(i) => Value::Int(-i),
                    other => other,
                },
            }
        }
        Expr::Binary { op, left, right } => {
            let lv = eval_expr(left, vars);
            let rv = eval_expr(right, vars);
            use vox_dsl::BinaryOp::*;
            match op {
                // 算术：目前只对 Int 生效，其它类型返回左值原样。
                Add => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                    (a, _) => a,
                },
                Sub => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
                    (a, _) => a,
                },
                Mul => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
                    (a, _) => a,
                },
                Div => match (lv, rv) {
                    (Value::Int(_), Value::Int(0)) => Value::Int(0),
                    (Value::Int(a), Value::Int(b)) => Value::Int(a / b),
                    (a, _) => a,
                },
                Mod => match (lv, rv) {
                    (Value::Int(_), Value::Int(0)) => Value::Int(0),
                    (Value::Int(a), Value::Int(b)) => Value::Int(a % b),
                    (a, _) => a,
                },
                // 比较：为保持与旧逻辑一致，统一按字符串比较。
                Eq => Value::Bool(value_to_string(&lv) == value_to_string(&rv)),
                Neq => Value::Bool(value_to_string(&lv) != value_to_string(&rv)),
                Lt => Value::Bool(value_to_string(&lv) < value_to_string(&rv)),
                Lte => Value::Bool(value_to_string(&lv) <= value_to_string(&rv)),
                Gt => Value::Bool(value_to_string(&lv) > value_to_string(&rv)),
                Gte => Value::Bool(value_to_string(&lv) >= value_to_string(&rv)),
                // 逻辑运算。
                And => Value::Bool(value_to_bool(&lv) && value_to_bool(&rv)),
                Or => Value::Bool(value_to_bool(&lv) || value_to_bool(&rv)),
            }
        }
        Expr::Call { name, args } => {
            let evaled_args: Vec<Value> = args.iter().map(|e| eval_expr(e, vars)).collect();
            eval_builtin(name, &evaled_args)
        }
    }
}

/// 应用一次 set 赋值：如果变量已存在则就地更新，否则在当前表中创建。
fn apply_set(vars: &mut HashMap<String, String>, set: &SetStmt) {
    let value = eval_expr(&set.expr, vars);
    vars.insert(set.name.clone(), value_to_string(&value));
}

fn current_unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs() as i64
}

fn eval_builtin(name: &str, args: &[Value]) -> Value {
    match name {
        // 时间相关：使用本地时间（Local::now），拆出小时/分钟/秒。
        "now" => Value::Int(current_unix_ts()),
        "time_hour" => {
            let now = Local::now();
            Value::Int(now.hour() as i64)
        }
        "time_minute" => {
            let now = Local::now();
            Value::Int(now.minute() as i64)
        }
        "time_second" => {
            let now = Local::now();
            Value::Int(now.second() as i64)
        }

        // 随机数相关。
        "rand" => {
            let v: i64 = (random::<u64>() % 1_000_000_000) as i64;
            Value::Int(v)
        }
        "rand_int" => {
            let (mut min, mut max) = match args {
                [Value::Int(a), Value::Int(b), ..] => (*a, *b),
                [Value::Int(a)] => (0, *a),
                _ => (0, 100),
            };
            if max < min {
                std::mem::swap(&mut min, &mut max);
            }
            if min == max {
                return Value::Int(min);
            }
            let span = (max - min + 1) as u64;
            let v = min + (random::<u64>() % span) as i64;
            Value::Int(v)
        }
        "rand_bool" => {
            Value::Bool(random::<bool>())
        }
        "rand_choice" => {
            if args.is_empty() {
                return Value::Str(String::new());
            }
            let len = args.len().max(1) as u64;
            let idx = (random::<u64>() % len) as usize;
            args[idx].clone()
        }

        // 未知内置函数：返回空字符串，避免 panic。
        _ => Value::Str(String::new()),
    }
}

/// 在文本中执行 `${...}` 表达式插值。
///
/// - `${name}`：视为变量引用，与旧逻辑兼容；
/// - `${i + 1}` / `${format_time(ts)}`：按照 DSL 表达式语法解析并求值。
fn interpolate_text(text: &str, vars: &HashMap<String, String>) -> Result<String, EngineError> {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();
    let mut last_pos: usize = 0;

    while let Some((i, ch)) = chars.next() {
        if ch == '$' {
            if let Some(&(_brace_idx, '{')) = chars.peek() {
                // 先把 `${` 之前的字面量片段写入结果。
                if i > last_pos {
                    result.push_str(&text[last_pos..i]);
                }

                // 消费 '{'，记录表达式起始位置。
                let (brace_idx, brace_ch) = chars.next().unwrap();
                debug_assert_eq!(brace_ch, '{');
                let expr_start = brace_idx + brace_ch.len_utf8();

                // 向前扫描，找到匹配的 '}'，同时跳过字符串字面量中的字符。
                let mut in_string = false;
                let mut string_delim = '\0';
                let mut end_idx: Option<usize> = None;

                while let Some((j, c)) = chars.next() {
                    if in_string {
                        if c == string_delim {
                            in_string = false;
                        }
                        continue;
                    }
                    if c == '"' || c == '\'' {
                        in_string = true;
                        string_delim = c;
                        continue;
                    }
                    if c == '}' {
                        end_idx = Some(j);
                        break;
                    }
                }

                let end_idx = match end_idx {
                    Some(idx) => idx,
                    None => {
                        return Err(EngineError::Interpolation(format!(
                            "未闭合的插值占位符，从位置 {} 开始: {}",
                            i, text
                        )));
                    }
                };

                let inner = &text[expr_start..end_idx];
                let inner_trimmed = inner.trim();
                if inner_trimmed.is_empty() {
                    return Err(EngineError::Interpolation(
                        "空的插值表达式 `${}`".to_string(),
                    ));
                }

                let expr = parse_expr(inner_trimmed).map_err(|e| {
                    EngineError::Interpolation(format!(
                        "插值表达式解析失败 `{}`: {}",
                        inner_trimmed, e
                    ))
                })?;
                let value = eval_expr(&expr, vars);
                result.push_str(&value_to_string(&value));

                // 更新下一个字面量片段的起始位置（跳过 '}'）。
                last_pos = end_idx + '}'.len_utf8();
                continue;
            }
        }
    }

    // 追加最后一个字面量片段（包括无任何插值的情况）。
    if last_pos < text.len() {
        result.push_str(&text[last_pos..]);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_expr_i_plus_1() {
        let mut vars = HashMap::new();
        vars.insert("i".to_string(), "1".to_string());
        let r = interpolate_text("结果是：${i+1}", &vars).unwrap();
        assert_eq!(r, "结果是：2");
    }

    #[test]
    fn interpolate_var_only() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "世界".to_string());
        let r = interpolate_text("你好，${name}！", &vars).unwrap();
        assert_eq!(r, "你好，世界！");
    }

    #[test]
    fn interpolate_empty_expr_fails() {
        let vars = HashMap::new();
        let r = interpolate_text("${}", &vars);
        assert!(r.is_err());
    }
}

