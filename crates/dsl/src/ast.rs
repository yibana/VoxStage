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
    /// `let` 变量定义。
    Let(LetStmt),
    /// `speak` 语句。
    Speak(SpeakStmt),
    /// `sleep` 语句，用于在执行过程中延迟一段时间（毫秒）。
    Sleep(SleepStmt),
    /// 条件语句。
    If(IfStmt),
    /// 固定次数循环。
    For(ForStmt),
    /// 条件循环。
    While(WhileStmt),
    /// 赋值语句：更新已有变量或创建新变量。
    Set(SetStmt),
    /// 背景音：播放。
    BgmPlay(BgmPlayStmt),
    /// 背景音：音量。
    BgmVolume(BgmVolumeStmt),
    /// 背景音：暂停 / 恢复 / 停止（无参语句）。
    BgmPause,
    BgmResume,
    BgmStop,
}

// ---------------------------------------------------------------------------
// 表达式系统：为 let / if / for / while 提供统一的表达式表示
// ---------------------------------------------------------------------------

/// 表达式节点。
#[derive(Debug, Clone)]
pub enum Expr {
    /// 字面量：数字或布尔（由执行层解释），序列化回 .vox 时不加引号。
    Literal(String),
    /// 字符串字面量（源码中带双引号），序列化回 .vox 时需保留引号。
    StrLiteral(String),
    /// 变量引用，例如 `foo`。
    Var(String),
    /// 一元运算，例如 `!flag` 或 `-x`。
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    /// 二元运算，例如 `a + b`、`x == 1`、`a && b` 等。
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// 函数调用，例如 `rand_int(1, 10)` 或 `rand_choice("a", "b")`。
    Call {
        name: String,
        args: Vec<Expr>,
    },
}

/// 一元运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// 逻辑非：`!expr`。
    Not,
    /// 数值取反：`-expr`。
    Neg,
}

/// 二元运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // 算术运算。
    Add, // +
    Sub, // -
    Mul, // *
    Div, // /
    Mod, // %
    // 比较运算。
    Eq,  // ==
    Neq, // !=
    Lt,  // <
    Lte, // <=
    Gt,  // >
    Gte, // >=
    // 逻辑运算。
    And, // &&
    Or,  // ||
}

/// `bgm "path_or_url"` 或 `bgm "path" loop`。
#[derive(Debug, Clone)]
pub struct BgmPlayStmt {
    /// 文件路径或 URL 字符串（由 runner 负责解析与加载）。
    pub path_or_url: String,
    /// 是否循环播放，默认 true。
    pub r#loop: bool,
}

/// `bgm_volume 0.5`。
#[derive(Debug, Clone)]
pub struct BgmVolumeStmt {
    pub volume: f32,
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

/// `let` 变量定义语句。
/// 语法示例：`let user_name = "小明"` 或 `let speed_fast = 1.3`。
#[derive(Debug, Clone)]
pub struct LetStmt {
    /// 变量名称。
    pub name: String,
    /// 右侧表达式（可以是字面量、变量或简单运算）。
    pub expr: Expr,
}

/// `set` 赋值语句：更新已有变量或创建新变量。
/// 语法示例：`set user_name = "小红"` 或 `set speed_fast = speed_fast + 0.1`。
#[derive(Debug, Clone)]
pub struct SetStmt {
    /// 变量名称。
    pub name: String,
    /// 右侧表达式。
    pub expr: Expr,
}

/// `sleep` 语句。
/// 语法：`sleep 1000` 表示延迟 1000 毫秒。
#[derive(Debug, Clone)]
pub struct SleepStmt {
    /// 延迟时长（毫秒）。
    pub duration_ms: u64,
}

/// if 语句。
/// MVP 阶段只支持 if，无 else。
#[derive(Debug, Clone)]
pub struct IfStmt {
    /// 条件表达式，例如 `score >= 60 && lang == "ZH"`。
    pub condition: Expr,
    pub body: Vec<Item>,
}

/// for 语句，按次数循环。
#[derive(Debug, Clone)]
pub struct ForStmt {
    /// 循环次数表达式，例如 `3` 或 `loop_count + 2`。
    pub times: Expr,
    /// 循环体语句。
    pub body: Vec<Item>,
}

/// while 语句，基于变量字符串值判断是否继续循环。
#[derive(Debug, Clone)]
pub struct WhileStmt {
    /// 条件表达式，求值为逻辑真时继续循环。
    pub condition: Expr,
    /// 循环体语句。
    pub body: Vec<Item>,
}

