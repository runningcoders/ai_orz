//! tests 单元测试（拆分自 tool.rs）
//!
//! 文件瘦身：原 454 行 → 248 行，测试体 207 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use serde_json::json;

#[test]
fn builtin_config_validation_rules() {
    // 合法：已知字段类型正确 + 未知字段宽松保留
    assert!(
        validate_builtin_tool_config(&json!({
            "command": "agent-browser",
            "timeout_ms": 15000,
            "max_output_bytes": 4096,
            "custom_field": "any"
        }))
        .is_ok()
    );
    // command 空 / 非字符串
    assert_eq!(
        validate_builtin_tool_config(&json!({ "command": "" })).unwrap_err(),
        "config.command 必须为非空字符串"
    );
    assert!(validate_builtin_tool_config(&json!({ "command": 42 })).is_err());
    // 数字字段：0 / 非数字字符串均拒绝
    assert_eq!(
        validate_builtin_tool_config(&json!({ "timeout_ms": 0 })).unwrap_err(),
        "config.timeout_ms 必须为正整数"
    );
    assert!(validate_builtin_tool_config(&json!({ "max_output_bytes": "8k" })).is_err());
    // 非对象（含 Null 存量兼容）直接通过
    assert!(validate_builtin_tool_config(&serde_json::Value::Null).is_ok());
    assert!(validate_builtin_tool_config(&json!("text")).is_ok());
}

#[test]
fn http_method_whitelist() {
    assert!(is_supported_http_method("GET"));
    assert!(is_supported_http_method("POST"));
    assert!(is_supported_http_method("post"));
    assert!(!is_supported_http_method("DELETE"));
    assert!(!is_supported_http_method("PUT"));
    assert!(!is_supported_http_method(""));
}

#[test]
fn parameters_schema_accepts_common_subset() {
    // 前端默认值（无参工具）
    assert!(validate_tool_parameters_schema(&json!({"type": "object"})).is_ok());
    // 嵌套数组元素对象 + 可空联合类型 + enum，均为各家通吃的写法
    assert!(
        validate_tool_parameters_schema(&json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {"type": "object", "properties": {"id": {"type": "integer"}}}
                },
                "units": {"type": ["string", "null"], "enum": ["celsius", "fahrenheit"]},
                // 元组毒药的修复形态：array<array<string>>（对象形式的 items）
                "env": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}}
            },
            "required": ["items"]
        }))
        .is_ok()
    );
}

#[test]
fn parameters_schema_rejects_tuple_items_with_path() {
    let err = validate_tool_parameters_schema(&json!({
        "type": "object",
        "properties": {
            "env": {
                "type": ["array", "null"],
                "items": [{"type": "string"}, {"type": "string"}],
                "minItems": 2,
                "maxItems": 2
            }
        }
    }))
    .unwrap_err();
    // 必须精确定位到出问题的字段，否则用户无从下手
    assert!(err.contains("/properties/env/items"), "{err}");
    assert!(err.contains("元组校验"), "{err}");
}

#[test]
fn parameters_schema_rejects_array_without_object_items() {
    let err = validate_tool_parameters_schema(&json!({
        "type": "object",
        "properties": {"tags": {"type": "array"}}
    }))
    .unwrap_err();
    assert!(err.contains("/properties/tags/items"), "{err}");
    assert!(err.contains("array schema missing items"), "{err}");

    // items 显式为 null 同样要拒（等价于缺失）
    assert!(
        validate_tool_parameters_schema(&json!({
            "type": "object",
            "properties": {"tags": {"type": "array", "items": null}}
        }))
        .is_err()
    );
}

#[test]
fn parameters_schema_rejects_invalid_type_value() {
    // DeepSeek 见 `"type": null` 会拒掉整个请求
    for bad in [serde_json::Value::Null, json!(42), json!(["object", 7])] {
        let err = validate_tool_parameters_schema(&json!({
            "type": "object",
            "properties": {"width": {"type": bad.clone()}}
        }))
        .unwrap_err();
        assert!(err.contains("/properties/width/type"), "{err}");
    }
}

#[test]
fn parameters_schema_rejects_prefix_items() {
    let err = validate_tool_parameters_schema(&json!({
        "type": "object",
        "properties": {"coord": {"type": "array", "prefixItems": [{"type": "number"}]}}
    }))
    .unwrap_err();
    assert!(err.contains("/properties/coord/prefixItems"), "{err}");
}

#[test]
fn parameters_schema_requires_object_root_with_explicit_type() {
    assert_eq!(
        validate_tool_parameters_schema(&serde_json::Value::Null).unwrap_err(),
        "parameters_schema 必须是 JSON 对象"
    );
    assert!(validate_tool_parameters_schema(&json!("not-a-schema")).is_err());
    // 缺 type / 空对象
    assert!(validate_tool_parameters_schema(&json!({})).is_err());
    // 根部不是 object
    assert!(validate_tool_parameters_schema(&json!({"type": "string"})).is_err());
}

#[test]
fn parameters_schema_reports_bounded_violation_count() {
    // 构造 7 处违规，报告最多 5 处并提示剩余数量
    let mut properties = serde_json::Map::new();
    for idx in 0..7 {
        properties.insert(
            format!("field_{idx}"),
            json!({"type": "array", "items": [{"type": "string"}]}),
        );
    }
    let err = validate_tool_parameters_schema(&json!({"type": "object", "properties": properties}))
        .unwrap_err();
    assert!(err.contains("另有 2 处未列出"), "{err}");
}

#[test]
fn parameters_schema_allows_property_names_that_look_like_keywords() {
    // 回归：护栏测试（内置 create_mcp_server / update_mcp_server）当场抓到的假阳性 ——
    // 字段**名字**叫 type / items / properties 时必须放行，不能按键名误判成关键字。
    // 真实结构：{"properties": {"type": {"enum": ["env"], "type": "string"}}}
    assert!(
        validate_tool_parameters_schema(&json!({
            "type": "object",
            "properties": {
                "type": {"type": "string", "enum": ["env", "header"]},
                "items": {"type": "array", "items": {"type": "string"}},
                "properties": {"type": "object", "properties": {"a": {"type": "string"}}},
                "required": {"type": "boolean"}
            },
            "required": ["type"]
        }))
        .is_ok()
    );
    // definitions 下的同名键同理
    assert!(
        validate_tool_parameters_schema(&json!({
            "type": "object",
            "definitions": {
                "Binding": {"type": "object", "properties": {"type": {"type": "string"}}}
            },
            "properties": {"b": {"$ref": "#/definitions/Binding"}}
        }))
        .is_ok()
    );
}

#[test]
fn parameters_schema_still_descends_into_properties_and_definitions() {
    // 反过来锁住：放行同名键不等于放弃下钻 —— properties / definitions 内部
    // 的真实违规必须仍然被抓到（否则整个校验器会退化成只查顶层）
    let err = validate_tool_parameters_schema(&json!({
        "type": "object",
        "properties": {"ok": {"type": "string"}},
        "definitions": {
            "Bad": {
                "type": "object",
                "properties": {"pair": {"type": "array", "items": [{"type": "string"}]}}
            }
        }
    }))
    .unwrap_err();
    assert!(
        err.contains("/definitions/Bad/properties/pair/items"),
        "{err}"
    );
}
