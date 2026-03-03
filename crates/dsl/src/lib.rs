//! Vox DSL crate。
//! 提供：抽象语法树（AST）定义和从文本到 AST 的基础解析功能。

mod ast;
mod error;
mod parser;

pub use ast::*;
pub use error::ParseError;
pub use parser::{parse_expr, parse_script};

