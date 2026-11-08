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

// ===== 工具级凭据声明统一契约（顶层 DTO → config 归并）=====

use crate::enums::ToolProtocol;
use crate::models::{
    CredentialBinding, CredentialKind, CredentialRequirement, CredentialRequirementScope,
};

fn shell_env_requirement(name: &str) -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some("my_platform".to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Env {
            name: name.to_string(),
        },
    }
}

#[test]
fn tool_credential_scope_maps_editable_protocols() {
    assert_eq!(
        tool_credential_scope(ToolProtocol::Http),
        Some(CredentialRequirementScope::HttpTool)
    );
    assert_eq!(
        tool_credential_scope(ToolProtocol::Shell),
        Some(CredentialRequirementScope::Shell)
    );
    // Builtin 写侧与读侧同源：config.credential_requirements（空则回落工厂静态）
    assert_eq!(
        tool_credential_scope(ToolProtocol::Builtin),
        Some(CredentialRequirementScope::Builtin)
    );
    // MCP 声明属主是 server 行 → 工具行不承载声明
    assert_eq!(tool_credential_scope(ToolProtocol::Mcp), None);
}

#[test]
fn shell_scope_only_allows_env_binding() {
    let env = shell_env_requirement("DEPLOY_TOKEN");
    assert!(
        crate::models::validate_requirements(
            std::slice::from_ref(&env),
            CredentialRequirementScope::Shell,
        )
        .is_ok()
    );
    // Header / Query / Internal 在 Shell scope 下必须被配置期拒绝
    for binding in [
        CredentialBinding::Header {
            name: "authorization".to_string(),
        },
        CredentialBinding::Query {
            name: "token".to_string(),
        },
        CredentialBinding::Internal {
            field: "api_key".to_string(),
        },
    ] {
        let mut req = env.clone();
        req.binding = binding;
        assert!(
            crate::models::validate_requirements(&[req], CredentialRequirementScope::Shell)
                .is_err()
        );
    }
}

#[test]
fn apply_credential_requirements_merges_top_level_contract() {
    let mut config = json!({"program": "deploy-cli", "credential_requirements": "stale"});
    let reqs = vec![shell_env_requirement("DEPLOY_TOKEN")];
    apply_credential_requirements(
        &mut config,
        Some(&reqs),
        tool_credential_scope(ToolProtocol::Shell),
    )
    .unwrap();
    let parsed: crate::config::ShellToolConfig = serde_json::from_value(config.clone()).unwrap();
    assert_eq!(parsed.program, "deploy-cli");
    assert_eq!(parsed.credential_requirements.len(), 1);
    assert_eq!(
        config["credential_requirements"][0]["binding"]["name"],
        "DEPLOY_TOKEN"
    );
}

#[test]
fn apply_credential_requirements_none_keeps_existing_and_empty_clears() {
    let scope = tool_credential_scope(ToolProtocol::Shell);
    let mut config = json!({
        "program": "cli",
        "credential_requirements": [serde_json::to_value(shell_env_requirement("KEEP")).unwrap()]
    });
    apply_credential_requirements(&mut config, None, scope).unwrap();
    assert_eq!(
        config["credential_requirements"][0]["binding"]["name"],
        "KEEP"
    );

    apply_credential_requirements(&mut config, Some(&[]), scope).unwrap();
    assert!(config.get("credential_requirements").is_none());
}

#[test]
fn apply_credential_requirements_rejects_mcp_only() {
    let mut config = json!({"transport": "stdio"});
    let err = apply_credential_requirements(
        &mut config,
        Some(&[shell_env_requirement("TOKEN")]),
        tool_credential_scope(ToolProtocol::Mcp),
    )
    .unwrap_err();
    assert!(err.contains("不由工具行承载"), "{err}");
    assert!(err.contains("MCP 请在 Server 配置"), "{err}");
    // 拒绝发生在校验/归并之前：config 原样不动
    assert_eq!(config, json!({"transport": "stdio"}));
}

// ===== Builtin 作用域（写侧放开后的六规则边界）=====

fn builtin_env_requirement(name: &str) -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GithubToken,
        platform: None,
        field: None,
        enhancer: None,
        binding: CredentialBinding::Env {
            name: name.to_string(),
        },
    }
}

fn builtin_internal_requirement(field_name: &str) -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some("internal_slot".to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Internal {
            field: field_name.to_string(),
        },
    }
}

fn builtin_identity_requirement(field: &str, env_name: &str) -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::AgentInfo,
        platform: None,
        field: Some(field.to_string()),
        enhancer: None,
        binding: CredentialBinding::Env {
            name: env_name.to_string(),
        },
    }
}

#[test]
fn builtin_scope_accepts_env_and_internal_bindings_round_trip() {
    let scope = tool_credential_scope(ToolProtocol::Builtin);
    assert_eq!(scope, Some(CredentialRequirementScope::Builtin));
    let reqs = vec![
        builtin_env_requirement("GITHUB_TOKEN"),
        builtin_internal_requirement("api_key"),
        builtin_identity_requirement("email", "GIT_AUTHOR_EMAIL"),
    ];
    let mut config = json!({"command": "agent-browser"});
    apply_credential_requirements(&mut config, Some(&reqs), scope).unwrap();
    // 写：config 落 credential_requirements，其余 config 字段原样保留
    assert_eq!(config["command"], "agent-browser");
    let parsed: Vec<CredentialRequirement> =
        serde_json::from_value(config["credential_requirements"].clone()).unwrap();
    assert_eq!(parsed, reqs);
}

#[test]
fn builtin_scope_rejects_invalid_declarations() {
    let scope = tool_credential_scope(ToolProtocol::Builtin);
    let cases = vec![
        // 身份类必须空 platform
        (
            CredentialRequirement {
                platform: Some("my_platform".to_string()),
                ..builtin_identity_requirement("email", "GIT_AUTHOR_EMAIL")
            },
            "不适用平台标识",
        ),
        // Builtin 不支持 Header 注入
        (
            CredentialRequirement {
                binding: CredentialBinding::Header {
                    name: "authorization".to_string(),
                },
                ..builtin_env_requirement("TOKEN")
            },
            "凭据注入点与工具协议不匹配",
        ),
        // 身份类 field 白名单
        (
            builtin_identity_requirement("password", "AGENT_SECRET"),
            "提取字段不合法",
        ),
        // 平台保留前缀
        (
            builtin_env_requirement("AI_ORZ_AGENT_ID"),
            "不允许使用 AI_ORZ_ 前缀",
        ),
    ];
    for (req, expected) in cases {
        let mut config = json!({"command": "agent-browser"});
        let err = apply_credential_requirements(&mut config, Some(&[req]), scope).unwrap_err();
        assert!(err.contains(expected), "{err}");
        // 校验失败不落库：config 原样不动
        assert!(config.get("credential_requirements").is_none());
        assert_eq!(config["command"], "agent-browser");
    }
}

#[test]
fn builtin_scope_rejects_duplicate_identity_injection_names() {
    let scope = tool_credential_scope(ToolProtocol::Builtin);
    let reqs = vec![
        builtin_identity_requirement("email", "AGENT_EMAIL"),
        builtin_identity_requirement("name", "AGENT_EMAIL"),
    ];
    let mut config = json!({});
    let err = apply_credential_requirements(&mut config, Some(&reqs), scope).unwrap_err();
    assert!(err.contains("重复的身份注入名"), "{err}");
}

#[test]
fn builtin_empty_requirements_clears_key_to_fall_back_to_factory() {
    let scope = tool_credential_scope(ToolProtocol::Builtin);
    let mut config = json!({
        "command": "agent-browser",
        "credential_requirements": [serde_json::to_value(builtin_env_requirement("OLD")).unwrap()]
    });
    apply_credential_requirements(&mut config, Some(&[]), scope).unwrap();
    // 空数组 = 显式清空 → 读侧回落工厂静态声明
    assert!(config.get("credential_requirements").is_none());
    assert_eq!(config["command"], "agent-browser");
}

#[test]
fn apply_credential_requirements_normalizes_null_config_and_validates() {
    let scope = tool_credential_scope(ToolProtocol::Shell);
    let mut config = serde_json::Value::Null;
    let mut bad = shell_env_requirement("TOKEN");
    bad.binding = CredentialBinding::Header {
        name: "authorization".to_string(),
    };
    // 非法注入点在配置期即被拒（不会静默落库）
    assert!(apply_credential_requirements(&mut config, Some(&[bad]), scope).is_err());

    apply_credential_requirements(&mut config, Some(&[shell_env_requirement("TOKEN")]), scope)
        .unwrap();
    assert!(config.is_object());
    assert_eq!(
        config["credential_requirements"][0]["binding"]["name"],
        "TOKEN"
    );
}

#[test]
fn agent_created_tool_tags_accepts_ordinary_tags() {
    let tags = vec!["data_analysis".to_string(), " finance ".to_string()];
    assert!(super::validate_agent_created_tool_tags(&tags).is_ok());
    assert!(super::validate_agent_created_tool_tags(&[]).is_ok());
}

#[test]
fn agent_created_tool_tags_reject_reserved_tags_that_would_escalate_reach() {
    for tag in super::AGENT_FORBIDDEN_TOOL_TAGS {
        let tags = vec![tag.to_string()];
        let err = super::validate_agent_created_tool_tags(&tags).unwrap_err();
        assert!(err.contains(tag), "{err}");
    }
    // 大小写与空白不能绕过（工具面按字面量匹配 tag）
    let err = super::validate_agent_created_tool_tags(&[" NEURAL ".to_string()]).unwrap_err();
    assert!(err.contains("neural"), "{err}");
}

#[test]
fn restricted_packs_are_also_forbidden_as_agent_created_tags() {
    // 受限包与禁用的自建标签必须同步：Agent 既不能装 admin 包，也不能造出
    // 一个带 admin 标签、被已授权 Agent 自动继承的工具。
    for pack in super::RESTRICTED_TOOL_PACKS {
        assert!(
            super::AGENT_FORBIDDEN_TOOL_TAGS.contains(pack),
            "受限包 {pack} 必须同时禁止作为 Agent 自建工具的标签"
        );
    }
}
