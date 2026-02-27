//! DSL 解析错误类型。

use thiserror::Error;

/// 描述一次解析错误的位置和信息。
#[derive(Debug, Error)]
#[error("parse error at line {line}, column {column}: {message}")]
pub struct ParseError {
    /// 行号（从 1 开始）。
    pub line: usize,
    /// 列号（从 1 开始）。
    pub column: usize,
    /// 错误描述信息。
    pub message: String,
}

impl ParseError {
    /// 构造一个新的解析错误。
    pub fn new(line: usize, column: usize, message: String) -> Self {
        Self { line, column, message }
    }
}

