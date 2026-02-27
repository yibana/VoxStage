//! DSL 抽象语法树（AST）定义。
//! 该层不关心具体执行逻辑，只负责以结构化形式描述脚本内容。

use std::collections::HashMap;

/// 一个完整脚本，由若干顶层语句组成。
#[derive(Debug, Clone)]
pub struct Script {
    /// 顶层语句列表，按源码顺序保存。
    pub items: Vec<Item>,
}

/// 顶层语句枚举。
#[derive(Debug, Clone)]
pub enum Item {
    /// `model` 定义块。
    Model(ModelDef),
    /// `role` 定义块。
    Role(RoleDef),
    /// `speak` 语句。
    Speak(SpeakStmt),
    // 预留：PresetDef / Let / If / For / While 等。
}

/// `model xxx { ... }` 定义。
#[derive(Debug, Clone)]
pub struct ModelDef {
    /// 模型名称，例如 `girl_model`。
    pub name: String,
    /// 模型内部的键值对配置，例如 `type = "http"`, `endpoint = "http://..."`。
    pub fields: HashMap<String, String>,
}

/// `role Xxx { ... }` 定义。
#[derive(Debug, Clone)]
pub struct RoleDef {
    /// 角色名称，例如 `Girl`。
    pub name: String,
    /// 绑定的模型名称，例如 `girl_model`。
    pub model: String,
    /// 角色默认参数（如 speed / volume 等）。
    pub params: HashMap<String, String>,
}

/// `speak` 语句。
/// MVP 阶段先支持：`speak RoleName "文本"` 或带简单参数覆盖。
#[derive(Debug, Clone)]
pub struct SpeakStmt {
    /// 说话目标（目前简单用字符串表示角色名或模型名）。
    pub target: String,
    /// 要说的文本（暂不展开字符串插值）。
    pub text: String,
    /// 覆盖参数，例如 `speed = 1.2`。
    pub params: HashMap<String, String>,
}

