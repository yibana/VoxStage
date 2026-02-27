//! 非常基础的 DSL 解析器实现。
//! 目前采用“按行解析 + 简单状态机”的方式，只支持：model / role / speak 三种结构。
//! 语法示例：
//!
//! ```text
//! model girl_model {
//!   type = "http"
//!   endpoint = "http://localhost:5000"
//! }
//!
//! role Girl {
//!   model = girl_model
//!   speed = 1.2
//! }
//!
//! speak Girl "你好"
//! ```

use std::collections::HashMap;

use crate::ast::{Item, ModelDef, RoleDef, Script, SpeakStmt};
use crate::error::ParseError;

/// 将 DSL 源码解析为 `Script` AST。
pub fn parse_script(src: &str) -> Result<Script, ParseError> {
    let mut items = Vec::new();
    let mut lines = src.lines().enumerate().peekable();

    while let Some((line_idx, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        if trimmed.starts_with("model ") {
            let model = parse_model_block(line_idx, trimmed, &mut lines, src)?;
            items.push(Item::Model(model));
        } else if trimmed.starts_with("role ") {
            let role = parse_role_block(line_idx, trimmed, &mut lines, src)?;
            items.push(Item::Role(role));
        } else if trimmed.starts_with("speak ") {
            let speak = parse_speak(line_idx, trimmed)?;
            items.push(Item::Speak(speak));
        } else {
            return Err(ParseError::new(
                line_idx + 1,
                1,
                format!("无法识别的语句开头: {trimmed}"),
            ));
        }
    }

    Ok(Script { items })
}

/// 解析 `model xxx { ... }` 块。
fn parse_model_block<'a>(
    first_line_idx: usize,
    first_line: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    _src: &str,
) -> Result<ModelDef, ParseError> {
    // 预期形如：model girl_model {  或  model girl_model
    let mut header = first_line.trim_end().to_string();
    let mut has_open_brace = header.ends_with('{');
    if has_open_brace {
        header.pop(); // 去掉末尾 '{'
        header = header.trim_end().to_string();
    }

    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(ParseError::new(
            first_line_idx + 1,
            1,
            "model 定义缺少名称".to_string(),
        ));
    }
    let name = parts[1].to_string();

    let mut fields = HashMap::new();

    // 如果首行不带 `{`，则继续读取下一行，期望是 `{`
    if !has_open_brace {
        if let Some((idx, line)) = lines.next() {
            if line.trim() != "{" {
                return Err(ParseError::new(
                    idx + 1,
                    1,
                    "model 定义缺少 '{'".to_string(),
                ));
            }
        } else {
            return Err(ParseError::new(
                first_line_idx + 1,
                1,
                "model 定义未完成".to_string(),
            ));
        }
    }

    // 读取块内容，直到 `}`。
    while let Some((idx, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if trimmed == "}" {
            break;
        }

        // 解析 key = value 行。
        if let Some((key, value)) = parse_key_value(trimmed) {
            fields.insert(key.to_string(), value.to_string());
        } else {
            return Err(ParseError::new(
                idx + 1,
                1,
                format!("无法解析 model 字段: {trimmed}"),
            ));
        }
    }

    Ok(ModelDef { name, fields })
}

/// 解析 `role Xxx { ... }` 块。
fn parse_role_block<'a>(
    first_line_idx: usize,
    first_line: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    _src: &str,
) -> Result<RoleDef, ParseError> {
    let mut header = first_line.trim_end().to_string();
    let mut has_open_brace = header.ends_with('{');
    if has_open_brace {
        header.pop();
        header = header.trim_end().to_string();
    }

    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(ParseError::new(
            first_line_idx + 1,
            1,
            "role 定义缺少名称".to_string(),
        ));
    }
    let name = parts[1].to_string();

    let mut model_name: Option<String> = None;
    let mut params = HashMap::new();

    if !has_open_brace {
        if let Some((idx, line)) = lines.next() {
            if line.trim() != "{" {
                return Err(ParseError::new(
                    idx + 1,
                    1,
                    "role 定义缺少 '{'".to_string(),
                ));
            }
        } else {
            return Err(ParseError::new(
                first_line_idx + 1,
                1,
                "role 定义未完成".to_string(),
            ));
        }
    }

    while let Some((idx, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if trimmed == "}" {
            break;
        }

        if let Some((key, value)) = parse_key_value(trimmed) {
            if key == "model" {
                model_name = Some(value.to_string());
            } else {
                params.insert(key.to_string(), value.to_string());
            }
        } else {
            return Err(ParseError::new(
                idx + 1,
                1,
                format!("无法解析 role 字段: {trimmed}"),
            ));
        }
    }

    let model = model_name.ok_or_else(|| {
        ParseError::new(
            first_line_idx + 1,
            1,
            "role 定义缺少 model 字段".to_string(),
        )
    })?;

    Ok(RoleDef { name, model, params })
}

/// 解析一行 `speak` 语句。
fn parse_speak(line_idx: usize, line: &str) -> Result<SpeakStmt, ParseError> {
    // 期望格式：speak Target "文本内容"
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("speak")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 speak 语句".to_string()))?
        .trim_start();

    // 拿到目标名称（直到空白字符）。
    let mut parts = rest.splitn(2, char::is_whitespace);
    let target = parts
        .next()
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "speak 缺少目标名称".to_string()))?
        .to_string();

    let after_target = parts
        .next()
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "speak 缺少文本部分".to_string()))?
        .trim_start();

    // 只解析最简单的双引号字符串，不处理转义。
    if !after_target.starts_with('"') {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "speak 文本必须以双引号开头".to_string(),
        ));
    }
    let mut chars = after_target.chars();
    chars.next(); // 跳过开头的 "
    let mut text = String::new();
    let mut closed = false;
    while let Some(ch) = chars.next() {
        if ch == '"' {
            closed = true;
            break;
        } else {
            text.push(ch);
        }
    }
    if !closed {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "speak 文本缺少结尾双引号".to_string(),
        ));
    }

    Ok(SpeakStmt {
        target,
        text,
        params: HashMap::new(),
    })
}

/// 解析简单的 `key = value` 形式。
/// value 两侧的引号会被去掉。
fn parse_key_value(line: &str) -> Option<(&str, String)> {
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }
    let key = parts[0].trim();
    let mut value = parts[1].trim().to_string();

    // 去掉前后双引号或单引号。
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value = value[1..value.len() - 1].to_string();
    }

    Some((key, value))
}

