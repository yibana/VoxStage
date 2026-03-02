//! 全局配置：模型与角色，存于 app_data_dir/config.json

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

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

const CONFIG_FILENAME: &str = "config.json";

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join(CONFIG_FILENAME))
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
        .invoke_handler(tauri::generate_handler![
            greet,
            get_config,
            save_config,
            get_roles,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
