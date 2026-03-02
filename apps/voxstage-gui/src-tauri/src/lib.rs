//! 全局配置：模型与角色，存于 app_data_dir/config.json
//! 剧本解析：将 .vox 文本解析为前端 ScriptItem 列表。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use vox_dsl::{parse_script, BinaryOp, Expr, Item, ParseError, UnaryOp};

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
}

const CONFIG_FILENAME: &str = "config.json";
const SCRIPT_DRAFT_FILENAME: &str = "script_draft.json";

/// 字符串字面量中需要转义的字符（用于 .vox 输出）
fn escape_expr_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 将 Expr 序列化为可编辑的表达式字符串（与 .vox 书写习惯一致）
fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Literal(s) => s.clone(),
        Expr::StrLiteral(s) => format!("\"{}\"", escape_expr_string(s)),
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
fn items_to_dtos(items: &[Item], base_indent: u32, next_id: &mut u64) -> Vec<ScriptItemDto> {
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
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: Some(stmt.name.clone()),
                    expr: Some(expr_to_string(&stmt.expr)),
                });
            }
            Item::Set(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "set".to_string(),
                    indent: base_indent,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: Some(stmt.name.clone()),
                    expr: Some(expr_to_string(&stmt.expr)),
                });
            }
            Item::Speak(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "speak".to_string(),
                    indent: base_indent,
                    role: Some(stmt.target.clone()),
                    text: Some(stmt.text.clone()),
                    ms: None,
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                });
            }
            Item::Sleep(stmt) => {
                out.push(ScriptItemDto {
                    id,
                    item_type: "sleep".to_string(),
                    indent: base_indent,
                    role: None,
                    text: None,
                    ms: Some(stmt.duration_ms),
                    condition: None,
                    times: None,
                    var_name: None,
                    expr: None,
                });
            }
            Item::If(stmt) => {
                out.push(ScriptItemDto {
                    id: id.clone(),
                    item_type: "if".to_string(),
                    indent: base_indent,
                    role: None,
                    text: None,
                    ms: None,
                    condition: Some(expr_to_string(&stmt.condition)),
                    times: None,
                    var_name: None,
                    expr: None,
                });
                out.extend(items_to_dtos(&stmt.body, base_indent + 1, next_id));
            }
            Item::For(stmt) => {
                out.push(ScriptItemDto {
                    id: id.clone(),
                    item_type: "for".to_string(),
                    indent: base_indent,
                    role: None,
                    text: None,
                    ms: None,
                    condition: None,
                    times: Some(expr_to_string(&stmt.times)),
                    var_name: None,
                    expr: None,
                });
                out.extend(items_to_dtos(&stmt.body, base_indent + 1, next_id));
            }
            Item::While(stmt) => {
                out.push(ScriptItemDto {
                    id: id.clone(),
                    item_type: "while".to_string(),
                    indent: base_indent,
                    role: None,
                    text: None,
                    ms: None,
                    condition: Some(expr_to_string(&stmt.condition)),
                    times: None,
                    var_name: None,
                    expr: None,
                });
                out.extend(items_to_dtos(&stmt.body, base_indent + 1, next_id));
            }
            Item::Model(_) | Item::Role(_) | Item::BgmPlay(_) | Item::BgmVolume(_) => {}
            Item::BgmPause | Item::BgmResume | Item::BgmStop => {}
        }
    }
    out
}

/// 解析 .vox 文本为前端剧本列表（仅包含剧本语句，不含 model/role）
#[tauri::command]
fn parse_vox_to_script(vox_text: String) -> Result<Vec<ScriptItemDto>, String> {
    let script = parse_script(&vox_text).map_err(|e: ParseError| e.to_string())?;
    let mut next_id = 1u64;
    Ok(items_to_dtos(&script.items, 0, &mut next_id))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
