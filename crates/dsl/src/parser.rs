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
    BgmPlayStmt, BgmVolumeStmt, BinaryOp, Expr, IfStmt, Item, LetStmt, ModelDef, RoleDef, Script,
    SetStmt, SpeakStmt, SleepStmt, ForStmt, WhileStmt, UnaryOp,
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
        } else if trimmed.starts_with("set ") {
            let set_stmt = parse_set(line_idx, trimmed)?;
            items.push(Item::Set(set_stmt));
        } else if trimmed.starts_with("bgm ") {
            let stmt = parse_bgm_play(line_idx, trimmed)?;
            items.push(Item::BgmPlay(stmt));
        } else if trimmed.starts_with("bgm_volume ") {
            let stmt = parse_bgm_volume(line_idx, trimmed)?;
            items.push(Item::BgmVolume(stmt));
        } else if trimmed == "bgm_pause" {
            items.push(Item::BgmPause);
        } else if trimmed == "bgm_resume" {
            items.push(Item::BgmResume);
        } else if trimmed == "bgm_stop" {
            items.push(Item::BgmStop);
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

/// 解析 if 条件部分：支持完整表达式语法（算术、比较、逻辑、括号等）。
fn parse_if_condition(line_idx: usize, rest: &str) -> Result<Expr, ParseError> {
    parse_expr_from_str(line_idx, rest)
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
            "for 语句缺少次数表达式".to_string(),
        ));
    }

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

    let times_expr = parse_expr_from_str(first_line_idx, rest)?;
    let body = parse_block_items(lines, src, first_line_idx, "for")?;

    Ok(ForStmt {
        times: times_expr,
        body,
    })
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
            "while 语句缺少条件表达式".to_string(),
        ));
    }

    let condition = parse_expr_from_str(first_line_idx, rest)?;

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

    Ok(WhileStmt { condition, body })
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
        } else if trimmed.starts_with("set ") {
            let set_stmt = parse_set(idx, trimmed)?;
            items.push(Item::Set(set_stmt));
        } else if trimmed.starts_with("bgm ") {
            let stmt = parse_bgm_play(idx, trimmed)?;
            items.push(Item::BgmPlay(stmt));
        } else if trimmed.starts_with("bgm_volume ") {
            let stmt = parse_bgm_volume(idx, trimmed)?;
            items.push(Item::BgmVolume(stmt));
        } else if trimmed == "bgm_pause" {
            items.push(Item::BgmPause);
        } else if trimmed == "bgm_resume" {
            items.push(Item::BgmResume);
        } else if trimmed == "bgm_stop" {
            items.push(Item::BgmStop);
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

/// 解析 `bgm "path_or_url"` 或 `bgm "path" loop`。
fn parse_bgm_play(line_idx: usize, line: &str) -> Result<BgmPlayStmt, ParseError> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("bgm")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 bgm 语句".to_string()))?
        .trim();

    let mut path_or_url = String::new();
    let mut r#loop = true;
    let mut chars = rest.chars().peekable();

    if let Some(&c) = chars.peek() {
        if c == '"' || c == '\'' {
            let quote = c;
            chars.next();
            while let Some(ch) = chars.next() {
                if ch == quote {
                    break;
                } else if ch == '\\' {
                    // 与 speak 文本保持一致的转义处理。
                    if let Some(next) = chars.next() {
                        match next {
                            '\\' | '"' => path_or_url.push(next),
                            'n' => path_or_url.push('\n'),
                            't' => path_or_url.push('\t'),
                            other => {
                                path_or_url.push('\\');
                                path_or_url.push(other);
                            }
                        }
                    } else {
                        path_or_url.push('\\');
                    }
                } else {
                    path_or_url.push(ch);
                }
            }
        }
    }
    if path_or_url.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "bgm 语句需要带引号的路径或 URL".to_string(),
        ));
    }

    let tail: String = chars.collect();
    let tail = tail.trim();
    if tail.eq_ignore_ascii_case("loop") {
        r#loop = true;
    } else if tail.eq_ignore_ascii_case("once") || tail.eq_ignore_ascii_case("no_loop") {
        r#loop = false;
    } else if !tail.is_empty() {
        r#loop = tail.eq_ignore_ascii_case("true");
    }

    Ok(BgmPlayStmt {
        path_or_url,
        r#loop,
    })
}

/// 解析 `bgm_volume 0.5`。
fn parse_bgm_volume(line_idx: usize, line: &str) -> Result<BgmVolumeStmt, ParseError> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("bgm_volume")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 bgm_volume 语句".to_string()))?
        .trim();

    let volume: f32 = rest.parse().map_err(|_| {
        ParseError::new(
            line_idx + 1,
            1,
            format!("无法解析 bgm_volume 数值: {rest}"),
        )
    })?;

    Ok(BgmVolumeStmt { volume })
}
/// 解析一行 `let` 变量定义语句。
/// 语法：`let name = value`，其中 value 可以是带引号的字符串或裸数字。
fn parse_let(line_idx: usize, line: &str) -> Result<LetStmt, ParseError> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("let")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 let 语句".to_string()))?
        .trim();

    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            format!("无法解析 let 语句: {rest}"),
        ));
    }

    let name = parts[0].trim();
    if name.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "let 语句缺少变量名".to_string(),
        ));
    }

    let expr_src = parts[1].trim();
    if expr_src.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "let 语句缺少右侧表达式".to_string(),
        ));
    }

    let expr = parse_expr_from_str(line_idx, expr_src)?;
    Ok(LetStmt {
        name: name.to_string(),
        expr,
    })
}

/// 解析一行 `set` 赋值语句。
/// 语法：`set name = expr`。
fn parse_set(line_idx: usize, line: &str) -> Result<SetStmt, ParseError> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("set")
        .ok_or_else(|| ParseError::new(line_idx + 1, 1, "无效的 set 语句".to_string()))?
        .trim();

    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            format!("无法解析 set 语句: {rest}"),
        ));
    }

    let name = parts[0].trim();
    if name.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "set 语句缺少变量名".to_string(),
        ));
    }

    let expr_src = parts[1].trim();
    if expr_src.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "set 语句缺少右侧表达式".to_string(),
        ));
    }

    let expr = parse_expr_from_str(line_idx, expr_src)?;
    Ok(SetStmt {
        name: name.to_string(),
        expr,
    })
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
        } else if ch == '\\' {
            // 处理常见转义：\\、\"、\n、\t；其它保持原样（保留反斜杠）。
            if let Some(next) = chars.next() {
                match next {
                    '\\' | '"' => text.push(next),
                    'n' => text.push('\n'),
                    't' => text.push('\t'),
                    other => {
                        text.push('\\');
                        text.push(other);
                    }
                }
            } else {
                text.push('\\');
            }
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

// ---------------------------------------------------------------------------
// 表达式解析：支持字面量、变量、算术、比较、逻辑、括号与一元运算
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Number(String),
    Str(String),
    Op(String),
    LParen,
    RParen,
    Comma,
}

fn tokenize_expr(line_idx: usize, src: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = src.chars().peekable();

    while let Some(ch) = chars.peek().cloned() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch == '"' || ch == '\'' {
            // 字符串字面量。
            let quote = ch;
            chars.next(); // 跳过引号
            let mut value = String::new();
            while let Some(c) = chars.next() {
                if c == quote {
                    break;
                } else {
                    value.push(c);
                }
            }
            tokens.push(Token::Str(value));
            continue;
        }

        if ch.is_ascii_digit() {
            let mut num = String::new();
            while let Some(c) = chars.peek().cloned() {
                if c.is_ascii_digit() {
                    num.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(num));
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut ident = String::new();
            while let Some(c) = chars.peek().cloned() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    ident.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Ident(ident));
            continue;
        }

        // 操作符与括号。
        // 先尝试双字符操作符：&& || == != <= >=
        if let Some(next) = chars.clone().nth(1) {
            let two = format!("{ch}{next}");
            if matches!(two.as_str(), "&&" | "||" | "==" | "!=" | "<=" | ">=") {
                tokens.push(Token::Op(two));
                chars.next();
                chars.next();
                continue;
            }
        }

        // 单字符操作符、逗号或括号。
        match ch {
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '+' | '-' | '*' | '/' | '%' | '<' | '>' | '!' => {
                tokens.push(Token::Op(ch.to_string()));
                chars.next();
            }
            ',' => {
                tokens.push(Token::Comma);
                chars.next();
            }
            _ => {
                return Err(ParseError::new(
                    line_idx + 1,
                    1,
                    format!("无法解析的表达式字符: {}", ch),
                ));
            }
        }
    }

    Ok(tokens)
}

fn op_precedence(op: &str) -> Option<(u8, bool)> {
    // (优先级, 是否右结合)
    match op {
        "||" => Some((1, false)),
        "&&" => Some((2, false)),
        "==" | "!=" => Some((3, false)),
        "<" | "<=" | ">" | ">=" => Some((4, false)),
        "+" | "-" => Some((5, false)),
        "*" | "/" | "%" => Some((6, false)),
        _ => None,
    }
}

fn parse_expr_from_str(line_idx: usize, src: &str) -> Result<Expr, ParseError> {
    let tokens = tokenize_expr(line_idx, src)?;
    if tokens.is_empty() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "空表达式".to_string(),
        ));
    }
    let mut pos = 0;
    let expr = parse_expr_bp(line_idx, &tokens, &mut pos, 0)?;
    if pos != tokens.len() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            format!("无法完整解析表达式: {}", src),
        ));
    }
    Ok(expr)
}

fn parse_expr_bp(
    line_idx: usize,
    tokens: &[Token],
    pos: &mut usize,
    min_prec: u8,
) -> Result<Expr, ParseError> {
    // 解析前缀部分（字面量、变量、括号、一元运算）。
    let mut left = parse_prefix(line_idx, tokens, pos)?;

    while *pos < tokens.len() {
        let op = match &tokens[*pos] {
            Token::Op(op) => op.clone(),
            _ => break,
        };

        if let Some((prec, right_assoc)) = op_precedence(&op) {
            if prec < min_prec {
                break;
            }
            *pos += 1; // 消费操作符
            let next_min_prec = if right_assoc { prec } else { prec + 1 };
            let right = parse_expr_bp(line_idx, tokens, pos, next_min_prec)?;
            let bin_op = match op.as_str() {
                "+" => BinaryOp::Add,
                "-" => BinaryOp::Sub,
                "*" => BinaryOp::Mul,
                "/" => BinaryOp::Div,
                "%" => BinaryOp::Mod,
                "==" => BinaryOp::Eq,
                "!=" => BinaryOp::Neq,
                "<" => BinaryOp::Lt,
                "<=" => BinaryOp::Lte,
                ">" => BinaryOp::Gt,
                ">=" => BinaryOp::Gte,
                "&&" => BinaryOp::And,
                "||" => BinaryOp::Or,
                _ => {
                    return Err(ParseError::new(
                        line_idx + 1,
                        1,
                        format!("不支持的二元运算符: {}", op),
                    ))
                }
            };
            left = Expr::Binary {
                op: bin_op,
                left: Box::new(left),
                right: Box::new(right),
            };
        } else {
            break;
        }
    }

    Ok(left)
}

fn parse_prefix(
    line_idx: usize,
    tokens: &[Token],
    pos: &mut usize,
) -> Result<Expr, ParseError> {
    if *pos >= tokens.len() {
        return Err(ParseError::new(
            line_idx + 1,
            1,
            "意外结束的表达式".to_string(),
        ));
    }

    match &tokens[*pos] {
        Token::Op(op) if op == "!" || op == "-" => {
            let op_kind = if op == "!" { UnaryOp::Not } else { UnaryOp::Neg };
            *pos += 1;
            let expr = parse_prefix(line_idx, tokens, pos)?;
            Ok(Expr::Unary {
                op: op_kind,
                expr: Box::new(expr),
            })
        }
        Token::LParen => {
            *pos += 1; // 跳过 '('
            let expr = parse_expr_bp(line_idx, tokens, pos, 0)?;
            if *pos >= tokens.len() || !matches!(tokens[*pos], Token::RParen) {
                return Err(ParseError::new(
                    line_idx + 1,
                    1,
                    "缺少右括号 ')'".to_string(),
                ));
            }
            *pos += 1; // 跳过 ')'
            Ok(expr)
        }
        Token::Number(n) => {
            let lit = Expr::Literal(n.clone());
            *pos += 1;
            Ok(lit)
        }
        Token::Str(s) => {
            let lit = Expr::StrLiteral(s.clone());
            *pos += 1;
            Ok(lit)
        }
        Token::Ident(name) => {
            let ident = name.clone();
            *pos += 1;
            // 函数调用：ident '(' args? ')'
            if *pos < tokens.len() {
                if let Token::LParen = tokens[*pos] {
                    *pos += 1; // 跳过 '('
                    let mut args = Vec::new();
                    // 解析零个或多个参数表达式，使用逗号分隔。
                    if *pos < tokens.len() && !matches!(tokens[*pos], Token::RParen) {
                        loop {
                            let arg = parse_expr_bp(line_idx, tokens, pos, 0)?;
                            args.push(arg);
                            if *pos >= tokens.len() {
                                return Err(ParseError::new(
                                    line_idx + 1,
                                    1,
                                    "函数调用缺少右括号 ')'".to_string(),
                                ));
                            }
                            match &tokens[*pos] {
                                Token::Comma => {
                                    *pos += 1;
                                    continue;
                                }
                                Token::RParen => {
                                    *pos += 1;
                                    break;
                                }
                                other => {
                                    return Err(ParseError::new(
                                        line_idx + 1,
                                        1,
                                        format!("函数调用参数列表语法错误: {:?}", other),
                                    ));
                                }
                            }
                        }
                    } else {
                        // 无参数：期望紧跟右括号。
                        if *pos >= tokens.len() || !matches!(tokens[*pos], Token::RParen) {
                            return Err(ParseError::new(
                                line_idx + 1,
                                1,
                                "函数调用缺少右括号 ')'".to_string(),
                            ));
                        }
                        *pos += 1;
                    }
                    return Ok(Expr::Call {
                        name: ident,
                        args,
                    });
                }
            }
            // 非调用场景：标识符要么是布尔字面量，要么是变量引用。
            if ident.eq_ignore_ascii_case("true") || ident.eq_ignore_ascii_case("false") {
                Ok(Expr::Literal(ident))
            } else {
                Ok(Expr::Var(ident))
            }
        }
        other => Err(ParseError::new(
            line_idx + 1,
            1,
            format!("无法解析的表达式起始: {:?}", other),
        )),
    }
}

