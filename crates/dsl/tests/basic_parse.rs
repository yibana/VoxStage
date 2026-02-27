//! vox-dsl 开发阶段基础解析测试。
//! 这些测试只关注 AST 结构是否按预期构造，不涉及执行与模型调用。

use std::collections::HashMap;

use vox_dsl::{parse_script, Item};

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

