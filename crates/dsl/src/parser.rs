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

use crate::ast::{
    CondOp, IfCondition, IfStmt, Item, LetStmt, ModelDef, RoleDef, Script, SpeakStmt, SleepStmt,
    ForStmt, WhileStmt,
};
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
        } else if trimmed.starts_with("let ") {
            let let_stmt = parse_let(line_idx, trimmed)?;
            items.push(Item::Let(let_stmt));
        } else if trimmed.starts_with("speak ") {
            let speak = parse_speak(line_idx, trimmed)?;
            items.push(Item::Speak(speak));
        } else if trimmed.starts_with("sleep ") {
            let sleep = parse_sleep(line_idx, trimmed)?;
            items.push(Item::Sleep(sleep));
        } else if trimmed.starts_with("if ") {
            let if_stmt = parse_if(line_idx, trimmed, &mut lines, src)?;
            items.push(Item::If(if_stmt));
        } else if trimmed.starts_with("for ") {
            let for_stmt = parse_for(line_idx, trimmed, &mut lines, src)?;
            items.push(Item::For(for_stmt));
        } else if trimmed.starts_with("while ") {
            let while_stmt = parse_while(line_idx, trimmed, &mut lines, src)?;
            items.push(Item::While(while_stmt));
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

/// 解析 `if` 语句块。
fn parse_if<'a>(
    first_line_idx: usize,
    first_line: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    src: &str,
) -> Result<IfStmt, ParseError> {
    // 处理头部，可能是 `if cond {` 或 `if cond`
    let mut header = first_line.trim_end().to_string();
    let has_open_brace = header.ends_with('{');
    if has_open_brace {
        header.pop();
        header = header.trim_end().to_string();
    }

    let rest = header
        .strip_prefix("if")
        .ok_or_else(|| ParseError::new(first_line_idx + 1, 1, "无效的 if 语句".to_string()))?
        .trim();

    let condition = parse_if_condition(first_line_idx, rest)?;

    // 如果首行不带 `{`，则继续读取下一行，期望是 `{`
    if !has_open_brace {
        if let Some((idx, line)) = lines.next() {
            if line.trim() != "{" {
                return Err(ParseError::new(
                    idx + 1,
                    1,
                    "if 语句缺少 '{'".to_string(),
                ));
            }
        } else {
            return Err(ParseError::new(
                first_line_idx + 1,
                1,
                "if 语句未完成".to_string(),
            ));
        }
    }

    let body = parse_block_items(lines, src, first_line_idx, "if")?;

    Ok(IfStmt { condition, body })
}

/// 解析 if 条件部分：`var == "value"` 或 `var != "value"`。
fn parse_if_condition(line_idx: usize, rest: &str) -> Result<IfCondition, ParseError> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "if 条件语法错误，期望形如 `if var == \"value\"`".to_string(),
        ));
    }

    let var = parts[0].to_string();
    let op_str = parts[1];
    let op = match op_str {
        "==" => CondOp::Eq,
        "!=" => CondOp::Neq,
        _ => {
            return Err(ParseError::new(
                line_idx + 1,
                1,
                format!("不支持的 if 运算符: {op_str}"),
            ))
        }
    };

    // 将 value 的剩余部分拼成一个字符串再去掉引号。
    let value_raw = parts[2..].join(" ");
    let value = if (value_raw.starts_with('"') && value_raw.ends_with('"'))
        || (value_raw.starts_with('\'') && value_raw.ends_with('\''))
    {
        value_raw[1..value_raw.len() - 1].to_string()
    } else {
        value_raw
    };

    Ok(IfCondition { var, op, value })
}

/// 解析 `for` 次数循环块。
fn parse_for<'a>(
    first_line_idx: usize,
    first_line: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    src: &str,
) -> Result<ForStmt, ParseError> {
    let mut header = first_line.trim_end().to_string();
    let has_open_brace = header.ends_with('{');
    if has_open_brace {
        header.pop();
        header = header.trim_end().to_string();
    }

    let rest = header
        .strip_prefix("for")
        .ok_or_else(|| ParseError::new(first_line_idx + 1, 1, "无效的 for 语句".to_string()))?
        .trim();

    if rest.is_empty() {
        return Err(ParseError::new(
            first_line_idx + 1,
            1,
            "for 语句缺少次数参数".to_string(),
        ));
    }

    let times: u64 = rest.parse().map_err(|_| {
        ParseError::new(
            first_line_idx + 1,
            1,
            format!("无法解析 for 次数: {rest}"),
        )
    })?;

    if !has_open_brace {
        if let Some((idx, line)) = lines.next() {
            if line.trim() != "{" {
                return Err(ParseError::new(
                    idx + 1,
                    1,
                    "for 语句缺少 '{'".to_string(),
                ));
            }
        } else {
            return Err(ParseError::new(
                first_line_idx + 1,
                1,
                "for 语句未完成".to_string(),
            ));
        }
    }

    let body = parse_block_items(lines, src, first_line_idx, "for")?;

    Ok(ForStmt { times, body })
}

/// 解析 `while` 循环块。
fn parse_while<'a>(
    first_line_idx: usize,
    first_line: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    src: &str,
) -> Result<WhileStmt, ParseError> {
    let mut header = first_line.trim_end().to_string();
    let has_open_brace = header.ends_with('{');
    if has_open_brace {
        header.pop();
        header = header.trim_end().to_string();
    }

    let rest = header
        .strip_prefix("while")
        .ok_or_else(|| ParseError::new(first_line_idx + 1, 1, "无效的 while 语句".to_string()))?
        .trim();

    if rest.is_empty() {
        return Err(ParseError::new(
            first_line_idx + 1,
            1,
            "while 语句缺少条件变量名".to_string(),
        ));
    }

    let var = rest.to_string();

    if !has_open_brace {
        if let Some((idx, line)) = lines.next() {
            if line.trim() != "{" {
                return Err(ParseError::new(
                    idx + 1,
                    1,
                    "while 语句缺少 '{'".to_string(),
                ));
            }
        } else {
            return Err(ParseError::new(
                first_line_idx + 1,
                1,
                "while 语句未完成".to_string(),
            ));
        }
    }

    let body = parse_block_items(lines, src, first_line_idx, "while")?;

    Ok(WhileStmt { var, body })
}

/// 解析一个 `{ ... }` 代码块内部的语句列表，直到遇到对应的 `}`。
fn parse_block_items<'a>(
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    src: &str,
    open_line_idx: usize,
    block_name: &str,
) -> Result<Vec<Item>, ParseError> {
    let mut items = Vec::new();

    while let Some((idx, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if trimmed == "}" {
            return Ok(items);
        }

        if trimmed.starts_with("model ") {
            let model = parse_model_block(idx, trimmed, lines, src)?;
            items.push(Item::Model(model));
        } else if trimmed.starts_with("role ") {
            let role = parse_role_block(idx, trimmed, lines, src)?;
            items.push(Item::Role(role));
        } else if trimmed.starts_with("let ") {
            let let_stmt = parse_let(idx, trimmed)?;
            items.push(Item::Let(let_stmt));
        } else if trimmed.starts_with("speak ") {
            let speak = parse_speak(idx, trimmed)?;
            items.push(Item::Speak(speak));
        } else if trimmed.starts_with("sleep ") {
            let sleep = parse_sleep(idx, trimmed)?;
            items.push(Item::Sleep(sleep));
        } else if trimmed.starts_with("if ") {
            let if_stmt = parse_if(idx, trimmed, lines, src)?;
            items.push(Item::If(if_stmt));
        } else if trimmed.starts_with("for ") {
            let for_stmt = parse_for(idx, trimmed, lines, src)?;
            items.push(Item::For(for_stmt));
        } else if trimmed.starts_with("while ") {
            let while_stmt = parse_while(idx, trimmed, lines, src)?;
            items.push(Item::While(while_stmt));
        } else {
            return Err(ParseError::new(
                idx + 1,
                1,
                format!("无法识别的语句开头: {trimmed}"),
            ));
        }
    }

    Err(ParseError::new(
        open_line_idx + 1,
        1,
        format!("{block_name} 块缺少 '}}'"),
    ))
}
/// 解析一行 `let` 变量定义语句。
/// 语法：`let name = value`，其中 value 可以是带引号的字符串或裸数字。
fn parse_let(line_idx: usize, line: &str) -> Result<LetStmt, ParseError> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("let")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 let 语句".to_string()))?
        .trim();

    if let Some((name, value)) = parse_key_value(rest) {
        if name.is_empty() {
            return Err(ParseError::new(
                line_idx + 1,
                1,
                "let 语句缺少变量名".to_string(),
            ));
        }
        Ok(LetStmt {
            name: name.to_string(),
            value,
        })
    } else {
        Err(ParseError::new(
            line_idx + 1,
            1,
            format!("无法解析 let 语句: {rest}"),
        ))
    }
}

/// 解析一行 `sleep` 语句。
/// 语法：`sleep 1000`，单位为毫秒。
fn parse_sleep(line_idx: usize, line: &str) -> Result<SleepStmt, ParseError> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("sleep")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 sleep 语句".to_string()))?
        .trim();

    if rest.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "sleep 缺少时长参数（毫秒）".to_string(),
        ));
    }

    // 允许使用下划线分隔数字，例如 1_000。
    let digits: String = rest.chars().filter(|c| *c != '_').collect();
    let duration_ms: u64 = digits.parse().map_err(|_| {
        ParseError::new(
            line_idx + 1,
            1,
            format!("无法解析 sleep 时长（毫秒）: {rest}"),
        )
    })?;

    Ok(SleepStmt { duration_ms })
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
/// 支持语法：
/// - `speak Girl "文本"`
/// - `speak Girl(speed = 1.2, language = "ZH") "文本"`
fn parse_speak(line_idx: usize, line: &str) -> Result<SpeakStmt, ParseError> {
    let trimmed = line.trim_start();
    let mut rest = trimmed
        .strip_prefix("speak")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 speak 语句".to_string()))?
        .trim_start();

    // 1. 解析目标名称：读取到第一个空白或 '('。
    let mut chars = rest.chars().peekable();
    let mut target = String::new();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() || ch == '(' || ch == '"' {
            break;
        }
        target.push(ch);
        chars.next();
    }
    if target.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "speak 缺少目标名称".to_string(),
        ));
    }

    // 2. 跳过目标名称后的空白。
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }

    // 3. 如果下一个字符是 '('，则解析参数列表。
    let mut params = HashMap::new();
    if let Some(&'(') = chars.peek() {
        chars.next(); // 跳过 '('
        let mut param_buf = String::new();
        let mut depth = 1;
        while let Some(ch) = chars.next() {
            match ch {
                '(' => {
                    depth += 1;
                    param_buf.push(ch);
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    } else {
                        param_buf.push(ch);
                    }
                }
                _ => param_buf.push(ch),
            }
        }
        if depth != 0 {
            return Err(ParseError::new(
                line_idx + 1,
                1,
                "speak 参数缺少右括号 ')'".to_string(),
            ));
        }

        // 将括号内部内容按 ',' 拆分为若干 `key = value`。
        for part in param_buf.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = parse_key_value(trimmed) {
                params.insert(k.to_string(), v);
            } else {
                return Err(ParseError::new(
                    line_idx + 1,
                    1,
                    format!("无法解析 speak 参数: {trimmed}"),
                ));
            }
        }

        // 括号结束后继续跳过空白。
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
    }

    // 4. 此时应当来到文本部分，必须以双引号开头。
    if let Some(&first) = chars.peek() {
        if first != '"' {
            return Err(ParseError::new(
                line_idx + 1,
                1,
                "speak 文本必须以双引号开头".to_string(),
            ));
        }
    } else {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "speak 缺少文本部分".to_string(),
        ));
    }

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
        params,
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

