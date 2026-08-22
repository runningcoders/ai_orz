//! Common tool-related types.

use serde::{Deserialize, Serialize};

/// Lightweight tool call trace reference.
///
/// Points to a detailed tool execution trace stored in tool-specific storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallTraceRef {
    /// Tool ID that this call belongs to.
    pub tool_id: String,
    /// Unique call ID for this specific tool execution.
    pub call_id: String,
}

impl ToolCallTraceRef {
    /// Create a new ToolCallTraceRef.
    pub fn new(tool_id: String, call_id: String) -> Self {
        Self { tool_id, call_id }
    }
}

/// Builtin 工具 config 已知字段轻量校验（D28：CLI 命令与行为参数进 PO config）
///
/// 仅校验已知字段的类型与取值（command 非空 string / timeout_ms·max_output_bytes
/// 正整数），未知字段宽松保留（不做白名单封闭，保持 config 扩展性）；
/// `config` 非对象（含 Null，存量 DB 兼容）时无已知字段可校验，直接通过。
/// 后端 update_tool 校验与前端表单提交前校验共用此单点。
pub fn validate_builtin_tool_config(config: &serde_json::Value) -> Result<(), String> {
    let Some(object) = config.as_object() else {
        return Ok(());
    };
    for (key, value) in object {
        match key.as_str() {
            "command" if !value.as_str().is_some_and(|command| !command.is_empty()) => {
                return Err("config.command 必须为非空字符串".to_string());
            }
            "timeout_ms" | "max_output_bytes"
                if !value.as_u64().is_some_and(|number| number > 0) =>
            {
                return Err(format!("config.{key} 必须为正整数"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// HTTP 工具支持的请求方法白名单（GET / POST；大小写不敏感，单点）
pub fn is_supported_http_method(method: &str) -> bool {
    matches!(method.to_ascii_uppercase().as_str(), "GET" | "POST")
}

#[cfg(test)]
mod tests {
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
}
