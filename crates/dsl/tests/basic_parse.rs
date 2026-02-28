//! vox-dsl 开发阶段基础解析测试。
//! 这些测试只关注 AST 结构是否按预期构造，不涉及执行与模型调用。

use vox_dsl::{parse_script, Item, Expr, BinaryOp};

/// 基础脚本解析：model + role + speak。
#[test]
fn parse_basic_model_role_speak() {
    let src = r#"
model girl_model {
  type = "http"
  endpoint = "http://localhost:5000"
}

role Girl {
  model = girl_model
  speed = "1.2"
}

speak Girl "你好"
"#;

    let script = parse_script(src).expect("parse_script should succeed");
    assert_eq!(script.items.len(), 3, "应解析出 3 个顶层语句");

    // 检查 model 定义。
    match &script.items[0] {
        Item::Model(model) => {
            assert_eq!(model.name, "girl_model");
            assert_eq!(model.fields.get("type").unwrap(), "http");
            assert_eq!(
                model.fields.get("endpoint").unwrap(),
                "http://localhost:5000"
            );
        }
        other => panic!("首个语句应为 ModelDef，实际为: {:?}", other),
    }

    // 检查 role 定义。
    match &script.items[1] {
        Item::Role(role) => {
            assert_eq!(role.name, "Girl");
            assert_eq!(role.model, "girl_model");
            assert_eq!(role.params.get("speed").unwrap(), "1.2");
        }
        other => panic!("第二个语句应为 RoleDef，实际为: {:?}", other),
    }

    // 检查 speak 语句。
    match &script.items[2] {
        Item::Speak(speak) => {
            assert_eq!(speak.target, "Girl");
            assert_eq!(speak.text, "你好");
            assert!(speak.params.is_empty());
        }
        other => panic!("第三个语句应为 SpeakStmt，实际为: {:?}", other),
    }
}

/// 解析错误路径：缺少 model 名称。
#[test]
fn parse_error_on_invalid_model_header() {
    let src = r#"
model {
  type = "http"
}
"#;

    let err = parse_script(src).expect_err("应当解析失败");
    assert!(err.message.contains("model 定义缺少名称"));
}

/// speak 参数覆盖语法：speak Girl(speed = 1.3, language = "EN") "..."
#[test]
fn parse_speak_with_params() {
    let src = r#"
role Girl {
  model = bert_vits2
  speed = "1.1"
}

speak Girl(speed = 1.3, language = "EN") "Hi"
"#;

    let script = parse_script(src).expect("parse_script should succeed");
    assert_eq!(script.items.len(), 2);

    match &script.items[1] {
        Item::Speak(speak) => {
            assert_eq!(speak.target, "Girl");
            assert_eq!(speak.text, "Hi");
            assert_eq!(speak.params.get("speed").unwrap(), "1.3");
            assert_eq!(speak.params.get("language").unwrap(), "EN");
        }
        other => panic!("第二个语句应为 SpeakStmt，实际为: {:?}", other),
    }
}

/// sleep 语句解析：sleep 1000
#[test]
fn parse_sleep_stmt() {
    let src = r#"
sleep 1000
"#;

    let script = parse_script(src).expect("parse_script should succeed");
    assert_eq!(script.items.len(), 1);

    match &script.items[0] {
        Item::Sleep(s) => {
            assert_eq!(s.duration_ms, 1000);
        }
        other => panic!("首个语句应为 SleepStmt，实际为: {:?}", other),
    }
}

/// let 变量定义解析：let user_name = "小明"
#[test]
fn parse_let_stmt() {
    let src = r#"
let user_name = "小明"
"#;

    let script = parse_script(src).expect("parse_script should succeed");
    assert_eq!(script.items.len(), 1);

    match &script.items[0] {
        Item::Let(stmt) => {
            assert_eq!(stmt.name, "user_name");
            match &stmt.expr {
                Expr::Literal(v) => assert_eq!(v, "小明"),
                other => panic!("let 表达式应为 Literal，小明，实际为: {:?}", other),
            }
        }
        other => panic!("首个语句应为 LetStmt，实际为: {:?}", other),
    }
}

/// set 赋值语句解析：set user_name = "小红"
#[test]
fn parse_set_stmt() {
    let src = r#"
set user_name = "小红"
"#;

    let script = parse_script(src).expect("parse_script should succeed");
    assert_eq!(script.items.len(), 1);

    match &script.items[0] {
        Item::Set(stmt) => {
            assert_eq!(stmt.name, "user_name");
            match &stmt.expr {
                Expr::Literal(v) => assert_eq!(v, "小红"),
                other => panic!("set 表达式应为 Literal，小红，实际为: {:?}", other),
            }
        }
        other => panic!("首个语句应为 SetStmt，实际为: {:?}", other),
    }
}

/// if / for / while 语句解析（结构校验）。
#[test]
fn parse_control_flow_stmts() {
    let src = r#"
let lang = "ZH"
let keep_running = "true"

if lang == "ZH" {
  speak Girl "你好"
}

for 3 {
  speak Girl "for 循环"
}

while keep_running {
  speak Girl "while 一次"
}
"#;

    let script = parse_script(src).expect("parse_script should succeed");

    // lang let, keep_running let, if, for, while
    assert_eq!(script.items.len(), 5);

    match &script.items[2] {
        Item::If(stmt) => {
            match &stmt.condition {
                Expr::Binary { op, left, right } => {
                    assert_eq!(*op, BinaryOp::Eq);
                    match (&**left, &**right) {
                        (Expr::Var(name), Expr::Literal(val)) => {
                            assert_eq!(name, "lang");
                            assert_eq!(val, "ZH");
                        }
                        other => panic!("if 条件结构不符合预期: {:?}", other),
                    }
                }
                other => panic!("if 条件应为 Binary 表达式，实际为: {:?}", other),
            }
        }
        other => panic!("第三个语句应为 IfStmt，实际为: {:?}", other),
    }

    match &script.items[3] {
        Item::For(stmt) => {
            match &stmt.times {
                Expr::Literal(v) => assert_eq!(v, "3"),
                other => panic!("for 次数字段应为 Literal(\"3\")，实际为: {:?}", other),
            }
        }
        other => panic!("第四个语句应为 ForStmt，实际为: {:?}", other),
    }

    match &script.items[4] {
        Item::While(stmt) => {
            match &stmt.condition {
                Expr::Var(name) => assert_eq!(name, "keep_running"),
                other => panic!("while 条件应为 Var(\"keep_running\")，实际为: {:?}", other),
            }
        }
        other => panic!("第五个语句应为 WhileStmt，实际为: {:?}", other),
    }
}

/// BGM 相关语句解析：bgm "path"、bgm_volume、bgm_pause / bgm_resume / bgm_stop。
#[test]
fn parse_bgm_stmts() {
    let src = r#"
bgm "crates/sounds/laugh1.wav"
bgm "music.mp3" loop
bgm "once.ogg" once
bgm_volume 0.5
bgm_pause
bgm_resume
bgm_stop
"#;

    let script = parse_script(src).expect("parse_script should succeed");
    assert_eq!(script.items.len(), 7);

    match &script.items[0] {
        Item::BgmPlay(stmt) => {
            assert_eq!(stmt.path_or_url, "crates/sounds/laugh1.wav");
            assert!(stmt.r#loop);
        }
        other => panic!("首个应为 BgmPlay，实际为: {:?}", other),
    }
    match &script.items[1] {
        Item::BgmPlay(stmt) => {
            assert_eq!(stmt.path_or_url, "music.mp3");
            assert!(stmt.r#loop);
        }
        other => panic!("第二个应为 BgmPlay，实际为: {:?}", other),
    }
    match &script.items[2] {
        Item::BgmPlay(stmt) => {
            assert_eq!(stmt.path_or_url, "once.ogg");
            assert!(!stmt.r#loop);
        }
        other => panic!("第三个应为 BgmPlay(once)，实际为: {:?}", other),
    }
    match &script.items[3] {
        Item::BgmVolume(stmt) => assert!((stmt.volume - 0.5).abs() < 1e-5),
        other => panic!("第四个应为 BgmVolume，实际为: {:?}", other),
    }
    assert!(matches!(script.items[4], Item::BgmPause));
    assert!(matches!(script.items[5], Item::BgmResume));
    assert!(matches!(script.items[6], Item::BgmStop));
}

