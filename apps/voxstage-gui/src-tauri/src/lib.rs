//! 全局配置：模型与角色，存于 app_data_dir/config.json
//! 剧本解析：将 .vox 文本解析为前端 ScriptItem 列表。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use std::sync::Arc;
use vox_dsl::{parse_script, ModelDef, BinaryOp, Expr, Item, ParseError, UnaryOp};
use vox_engine::{register_providers_from_script, EngineError, ModelManager};
use vox_runner::run_script_with_audio;
use vox_tts_http::{BertVits2Config, BertVits2Provider, GptSovitsV2Config, GptSovitsV2Provider};

/// 单条模型配置（对应 DSL 的 model 块）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub extra: std::collections::HashMap<String, String>,
}

/// 单条角色配置（对应 DSL 的 role 块）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    pub name: String,
    /// 绑定的模型名称
    pub model: String,
    #[serde(default)]
    pub params: std::collections::HashMap<String, String>,
}

/// 全局配置：模型列表 + 角色列表
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub roles: Vec<RoleEntry>,
}

/// 与前端 ScriptItem 一致的剧本项 DTO（用于 Code → 编辑 解析结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptItemDto {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub indent: u32,
    /// 静态语句索引（仅对 speak/sleep 等会产生 EngineCommand 的语句赋值），用于运行进度高亮。
    #[serde(skip_serializing_if = "Option::is_none", rename = "sourceIndex")]
    pub source_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "varName")]
    pub var_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "bgmPath")]
    pub bgm_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "bgmLoop")]
    pub bgm_loop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "bgmVolume")]
    pub bgm_volume: Option<f32>,
}

const CONFIG_FILENAME: &str = "config.json";
const SCRIPT_DRAFT_FILENAME: &str = "script_draft.json";

/// 播放控制：包含“暂停 / 停止”标志，供 runner 在循环内检查。
struct PlaybackControl {
    pause_flag: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
}

/// 字符串字面量中需要转义的字符（用于 .vox 输出）
fn escape_expr_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 将 Expr 序列化为可编辑的表达式字符串（与 .vox 书写习惯一致）
fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Literal(s) => s.clone(),
        // 对字符串字面量，这里只负责补上引号，不再额外转义，避免多次往返时重复增加反斜杠。
        // 反斜杠等转义细节由 DSL 自身的解析/序列化规则保证一致性。
        Expr::StrLiteral(s) => format!("\"{}\"", s),
        Expr::Var(s) => s.clone(),
        Expr::Unary { op, expr: e } => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{op_str}{}", expr_to_string(e))
        }
        Expr::Binary { op, left, right } => {
            let op_str = match op {
                BinaryOp::Add => " + ",
                BinaryOp::Sub => " - ",
                BinaryOp::Mul => " * ",
                BinaryOp::Div => " / ",
                BinaryOp::Mod => " % ",
                BinaryOp::Eq => " == ",
                BinaryOp::Neq => " != ",
                BinaryOp::Lt => " < ",
                BinaryOp::Lte => " <= ",
                BinaryOp::Gt => " > ",
                BinaryOp::Gte => " >= ",
                BinaryOp::And => " && ",
                BinaryOp::Or => " || ",
            };
            format!(
                "{}{}{}",
                expr_to_string(left),
                op_str,
                expr_to_string(right)
            )
        }
        Expr::Call { name, args } => {
            let args_str: Vec<String> = args.iter().map(expr_to_string).collect();
            format!("{}({})", name, args_str.join(", "))
        }
    }
}

/// 将 AST 的 Item 列表转为带缩进的扁平 ScriptItemDto，只保留 speak/sleep/let/set/if/for/while
/// 同时为 speak/sleep 语句分配与 EngineCommand 相同的 source_index（从 0 开始）。
fn items_to_dtos(
    items: &[Item],
    base_indent: u32,
    next_id: &mut u64,
    next_index: &mut u32,
) -> Vec<ScriptItemDto> {
    let mut out = Vec::new();
    for item in items {
        let id = format!("item-{}", *next_id);
        *next_id += 1;
        match item {
            Item::Let(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "let".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: Some(stmt.name.clone()),
                    expr: Some(expr_to_string(&stmt.expr)),
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::Set(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "set".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: Some(stmt.name.clone()),
                    expr: Some(expr_to_string(&stmt.expr)),
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::Speak(stmt) => {
                let idx = *next_index;
                *next_index += 1;
                out.push(ScriptItemDto {
                    id,
                    item_type: "speak".to_string(),
                    indent: base_indent,
                    source_index: Some(idx),
                    role: Some(stmt.target.clone()),
                    text: Some(stmt.text.clone()),
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::Sleep(stmt) => {
                let idx = *next_index;
                *next_index += 1;
                out.push(ScriptItemDto {
                    id,
                    item_type: "sleep".to_string(),
                    indent: base_indent,
                    source_index: Some(idx),
                    role: None,
                    text: None,
                    ms: Some(stmt.duration_ms),
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::If(stmt) => {
                out.push(ScriptItemDto {
                    id: id.clone(),
                    item_type: "if".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: Some(expr_to_string(&stmt.condition)),
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
                out.extend(items_to_dtos(
                    &stmt.body,
                    base_indent + 1,
                    next_id,
                    next_index,
                ));
            }
            Item::For(stmt) => {
                out.push(ScriptItemDto {
                    id: id.clone(),
                    item_type: "for".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: Some(expr_to_string(&stmt.times)),
                    var_name: None,
                    expr: None,
                     bgm_path: None,
                     bgm_loop: None,
                     bgm_volume: None,
                });
                out.extend(items_to_dtos(
                    &stmt.body,
                    base_indent + 1,
                    next_id,
                    next_index,
                ));
            }
            Item::While(stmt) => {
                out.push(ScriptItemDto {
                    id: id.clone(),
                    item_type: "while".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: Some(expr_to_string(&stmt.condition)),
                    times: None,
                    var_name: None,
                    expr: None,
                     bgm_path: None,
                     bgm_loop: None,
                     bgm_volume: None,
                });
                out.extend(items_to_dtos(
                    &stmt.body,
                    base_indent + 1,
                    next_id,
                    next_index,
                ));
            }
            Item::BgmPlay(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "bgm_play".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: Some(stmt.path_or_url.clone()),
                    bgm_loop: Some(stmt.r#loop),
                    bgm_volume: None,
                });
            }
            Item::BgmVolume(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "bgm_volume".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: Some(stmt.volume),
                });
            }
            Item::BgmPause => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "bgm_pause".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::BgmResume => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "bgm_resume".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::BgmStop => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "bgm_stop".to_string(),
                    indent: base_indent,
                    source_index: None,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                    bgm_path: None,
                    bgm_loop: None,
                    bgm_volume: None,
                });
            }
            Item::Model(_) | Item::Role(_) => {}
        }
    }
    out
}

/// 解析 .vox 文本为前端剧本列表（仅包含剧本语句，不含 model/role）
#[tauri::command]
fn parse_vox_to_script(vox_text: String) -> Result<Vec<ScriptItemDto>, String> {
    let script = parse_script(&vox_text).map_err(|e: ParseError| e.to_string())?;
    let mut next_id = 1u64;
    let mut next_index = 0u32;
    Ok(items_to_dtos(
        &script.items,
        0,
        &mut next_id,
        &mut next_index,
    ))
}

/// 打开脚本文件结果：路径与内容（.vox 或 .json 文本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenScriptResult {
    pub path: String,
    pub content: String,
}

/// 打开文件对话框，读取 .vox 或 .json 脚本文件
#[tauri::command]
async fn open_script_file(app: tauri::AppHandle) -> Result<OpenScriptResult, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Vox 脚本", &["vox", "json"])
        .add_filter("所有文件", &["*"])
        .blocking_pick_file();
    let Some(fp) = file_path else {
        return Err("用户取消".to_string());
    };
    let path_buf = fp.into_path().map_err(|e| e.to_string())?;
    let content = fs::read_to_string(&path_buf).map_err(|e| e.to_string())?;
    let path = path_buf.to_string_lossy().into_owned();
    Ok(OpenScriptResult { path, content })
}

/// 另存为：打开保存对话框，将内容写入 .vox 文件
#[tauri::command]
async fn save_script_file(app: tauri::AppHandle, content: String) -> Result<String, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Vox 脚本", &["vox"])
        .add_filter("所有文件", &["*"])
        .set_file_name("script.vox")
        .blocking_save_file();
    let Some(fp) = file_path else {
        return Err("用户取消".to_string());
    };
    let path_buf = fp.into_path().map_err(|e| e.to_string())?;
    fs::write(&path_buf, content).map_err(|e| e.to_string())?;
    Ok(path_buf.to_string_lossy().into_owned())
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join(CONFIG_FILENAME))
}

fn script_draft_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join(SCRIPT_DRAFT_FILENAME))
}

/// 读取编辑中的脚本草稿（JSON 数组），不存在或空则返回 "[]"
#[tauri::command]
fn get_script_draft(app: tauri::AppHandle) -> Result<String, String> {
    let path = script_draft_path(&app)?;
    if !path.exists() {
        return Ok("[]".to_string());
    }
    let s = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(s)
}

/// 保存编辑中的脚本草稿（JSON 字符串）
#[tauri::command]
fn save_script_draft(app: tauri::AppHandle, json: String) -> Result<(), String> {
    let path = script_draft_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    let path = config_path(&app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let s = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    let path = config_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())
}

/// 仅返回角色列表（供前端下拉等使用）
#[tauri::command]
fn get_roles(app: tauri::AppHandle) -> Result<Vec<RoleEntry>, String> {
    let config = get_config(app)?;
    Ok(config.roles)
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 根据脚本中的 model 块创建 TTS Provider（与 CLI 逻辑一致）
fn model_def_to_provider(def: &ModelDef) -> Result<Arc<dyn vox_core::TtsProvider>, String> {
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
            Ok(BertVits2Provider::new(def.name.clone(), config).into_shared())
        }
        "gpt_sovits_v2" => {
            let config = GptSovitsV2Config {
                endpoint,
                model_id,
            };
            Ok(GptSovitsV2Provider::new(def.name.clone(), config).into_shared())
        }
        _ => Err(format!(
            "不支持的 provider: {}（可选: bert_vits2, gpt_sovits_v2）",
            provider
        )),
    }
}

/// 运行剧本：解析 .vox 文本，注册模型，执行并播放。
/// 因音频播放上下文（rodio/cpal）非 Send，在当前线程 block_on 执行；脚本较长时 invoke 会持续直到结束。
/// 通过 PlaybackControl 中的 pause_flag 支持在 runner 循环内暂停/继续。
#[tauri::command]
async fn run_script(
    app: tauri::AppHandle,
    vox_text: String,
    playback: tauri::State<'_, PlaybackControl>,
) -> Result<(), String> {
    // 每次运行前确保处于“未暂停 / 未停止”状态
    playback.pause_flag.store(false, Ordering::SeqCst);
    playback.stop_flag.store(false, Ordering::SeqCst);

    let pause_flag = playback.pause_flag.clone();
    let stop_flag = playback.stop_flag.clone();
    let app_handle = app.clone();

    // 关键：把阻塞式播放逻辑放到后台线程，避免卡住 WebView/UI。
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut manager = ModelManager::new();
        register_providers_from_script(&mut manager, &vox_text, |def: &ModelDef| {
            model_def_to_provider(def)
        })
        .map_err(|e: EngineError| e.to_string())?;

        // 进度回调：每当 runner 即将执行一条命令时，向前端广播当前 source_index。
        let app_for_progress = app_handle.clone();
        let progress_cb: Arc<dyn Fn(u32) + Send + Sync> = Arc::new(move |index: u32| {
            let _ = app_for_progress.emit("script-progress", index);
        });

        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let result = rt
            .block_on(run_script_with_audio(
                &manager,
                &vox_text,
                Some(pause_flag),
                Some(stop_flag),
                Some(progress_cb),
            ))
            .map_err(|e| e.to_string());

        // 不论成功或失败，都通知前端脚本已结束，便于清理高亮与状态。
        let _ = app_handle.emit("script-finished", ());

        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 暂停当前运行中的剧本（设置暂停标志为 true），runner 会在下一条命令处理前阻塞。
#[tauri::command]
fn pause_script(playback: tauri::State<'_, PlaybackControl>) {
    playback.pause_flag.store(true, Ordering::SeqCst);
}

/// 继续运行当前已暂停的剧本（将暂停标志重置为 false）。
#[tauri::command]
fn resume_script(playback: tauri::State<'_, PlaybackControl>) {
    playback.pause_flag.store(false, Ordering::SeqCst);
}

/// 停止当前运行中的剧本（设置停止标志为 true），runner 会尽快结束命令循环并停止 BGM。
#[tauri::command]
fn stop_script(playback: tauri::State<'_, PlaybackControl>) {
    playback.stop_flag.store(true, Ordering::SeqCst);
    // 取消暂停状态，避免卡在暂停检查里
    playback.pause_flag.store(false, Ordering::SeqCst);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PlaybackControl {
            pause_flag: Arc::new(AtomicBool::new(false)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_config,
            save_config,
            get_roles,
            parse_vox_to_script,
            open_script_file,
            save_script_file,
            get_script_draft,
            save_script_draft,
            run_script,
            pause_script,
            resume_script,
            stop_script,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
