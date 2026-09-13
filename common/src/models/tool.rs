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

/// shell_exec 隔离 HOME 下支持「根目录指回真实 HOME」的工具链映射
///
/// 三元组：(工具链名, 官方环境变量, 真实 HOME 相对路径)。
/// 这些工具链都支持用环境变量指定根目录而不依赖 `HOME`，因此隔离 HOME
/// （git/gh 身份隔离）与工具链自身配置（`~/.cargo`、`~/.nvm` 等）可以兼得。
/// 后端注入与前端表单校验共用此单点；未列出的名字两侧都忽略/拒绝。
pub const SHELL_TOOLCHAIN_HOME_VARS: &[(&str, &str, &str)] = &[
    ("nvm", "NVM_DIR", ".nvm"),
    ("cargo", "CARGO_HOME", ".cargo"),
    ("rustup", "RUSTUP_HOME", ".rustup"),
    ("pyenv", "PYENV_ROOT", ".pyenv"),
    ("rbenv", "RBENV_ROOT", ".rbenv"),
    ("go", "GOPATH", "go"),
    ("npm", "NPM_CONFIG_USERCONFIG", ".npmrc"),
];

/// 工具链名是否在支持列表中（大小写不敏感）
pub fn is_supported_toolchain(name: &str) -> bool {
    let lowered = name.trim().to_lowercase();
    SHELL_TOOLCHAIN_HOME_VARS
        .iter()
        .any(|(supported, _, _)| *supported == lowered)
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

/// 工具 `parameters` JSON Schema 的「跨 provider 安全」校验
///
/// 我们支持的 provider（OpenAI / DeepSeek / Qwen / Doubao / DoubaoVision / Ollama /
/// Custom）全部走同一个 OpenAI 兼容协议（`service/dao/cortex/native/mod.rs`），
/// 但各家对 JSON Schema 的接受度并不一致，且失败方式分两类：
///
/// - **硬拒**：整个 `chat/completions` 请求被回 400，报错还指不出是哪个工具；
/// - **静默忽略**：请求成功，但该关键字从未参与约束，schema 退化成纯文本提示。
///
/// 因此用户经 `create_tool` / `update_tool` 写入的 schema 必须收敛到最小公共子集
/// （LCD）—— 否则一个工具就能毒死持有它的 Agent 的**每一轮**模型调用。
///
/// 拒绝规则（每条都对应真实的 provider 行为，不是风格偏好）：
///
/// - 整体必须是 JSON 对象，且根部 `type` 显式为 `"object"`：工具参数契约即一个对象；
///   无参工具应写 `{"type":"object","properties":{}}`
/// - 任何 `type` 只能是字符串或字符串数组（`null` / 数字会被 DeepSeek 拒整包）
/// - 任何位置 `items` 都不得是数组（draft-07 的元组校验）：火山方舟与 OpenAI 均回
///   400 `[...] is not of type 'object', 'boolean'`
/// - 声明为数组的位置必须带**对象**形式的 `items`：OpenAI 回 400
///   `array schema missing items`
/// - 不得使用 `prefixItems`：OpenAI 未实现 2020-12 的元组关键字
///
/// 错误信息以 JSON Pointer（`/properties/env/items`）定位，便于用户直接改。
/// 后端 create/update 写入口与前端表单提交前校验共用此单点。
pub fn validate_tool_parameters_schema(schema: &serde_json::Value) -> Result<(), String> {
    let Some(root) = schema.as_object() else {
        return Err("parameters_schema 必须是 JSON 对象".to_string());
    };

    let mut violations: Vec<String> = Vec::new();
    match root.get("type") {
        Some(serde_json::Value::String(kind)) if kind == "object" => {}
        Some(other) => violations.push(format!(
            "/type 必须为 \"object\"，当前为 {other}；无参工具请写 {{\"type\":\"object\",\"properties\":{{}}}}"
        )),
        None => violations.push(
            "/type 缺失，必须显式声明 \"object\"（无参工具请写 {\"type\":\"object\",\"properties\":{}}）"
                .to_string(),
        ),
    }
    collect_schema_violations(schema, "", &mut violations);

    if violations.is_empty() {
        return Ok(());
    }
    const MAX_REPORTED: usize = 5;
    let shown = violations
        .iter()
        .take(MAX_REPORTED)
        .cloned()
        .collect::<Vec<String>>()
        .join("；");
    let more = if violations.len() > MAX_REPORTED {
        format!("（另有 {} 处未列出）", violations.len() - MAX_REPORTED)
    } else {
        String::new()
    };
    Err(format!(
        "parameters_schema 不符合跨 provider 安全子集：{shown}{more}"
    ))
}

/// 递归收集「会让网关拒绝或静默忽略」的 schema 写法，路径为 JSON Pointer 片段。
///
/// ⚠️ 必须沿**已知的 schema 承载关键字**下钻，绝不能把 `properties` 里的键名当关键字：
/// 用户完全可能有一个**字段就叫 `type`**（如 `{"type":"env"}`），若按 key 名泛化匹配，
/// `properties.type` 会被误判成 JSON Schema 的 `type` 关键字 → 假阳性直接挡住合法工具
/// （`create_mcp_server` / `update_mcp_server` 的 `CredentialBinding` 就是这种结构）。
fn collect_schema_violations(node: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    let serde_json::Value::Object(map) = node else {
        return;
    };

    // ① `type` 关键字的合法性与「array 必须带对象 items」的配套约束
    if let Some(kind) = map.get("type") {
        match schema_type_names(kind) {
            Some(names) => {
                if names.contains(&"array") {
                    match map.get("items") {
                        // 元组写法由 ② 报；对象形式合法
                        Some(serde_json::Value::Array(_)) | Some(serde_json::Value::Object(_)) => {}
                        Some(other) => {
                            out.push(format!("{path}/items 必须为对象，当前为 {other}"));
                        }
                        None => out.push(format!(
                            "{path}/items 缺失：声明为 array 的 schema 必须提供对象形式的 items（OpenAI 会回 400 array schema missing items）"
                        )),
                    }
                }
            }
            None => out.push(format!(
                "{path}/type 必须为字符串或字符串数组，当前为 {kind}"
            )),
        }
    }

    // ② 元组写法：`items` 为数组
    if let Some(serde_json::Value::Array(_)) = map.get("items") {
        out.push(format!(
            "{path}/items 不能是数组：数组形式的 items 是元组校验，火山方舟与 OpenAI 会回 400"
        ));
    }

    // ③ 2020-12 元组关键字，OpenAI 未实现
    if map.contains_key("prefixItems") {
        out.push(format!(
            "{path}/prefixItems 不受支持（2020-12 元组关键字，OpenAI 会回 400）"
        ));
    }

    // ④ 沿 schema 承载关键字下钻
    for (key, value) in map {
        let child = format!("{path}/{key}");
        match key.as_str() {
            // 名字 → schema 的映射（键名是字段名，不是关键字）
            "properties" | "patternProperties" | "definitions" | "$defs" | "dependentSchemas" => {
                if let serde_json::Value::Object(entries) = value {
                    for (name, sub) in entries {
                        collect_schema_violations(sub, &format!("{child}/{name}"), out);
                    }
                }
            }
            // 单个 schema 或（元组/2020-12 形态的）schema 数组
            "items" | "prefixItems" => match value {
                serde_json::Value::Object(_) => collect_schema_violations(value, &child, out),
                serde_json::Value::Array(elements) => {
                    for (idx, element) in elements.iter().enumerate() {
                        collect_schema_violations(element, &format!("{child}[{idx}]"), out);
                    }
                }
                _ => {}
            },
            // schema 数组（组合子）
            "anyOf" | "oneOf" | "allOf" => {
                if let serde_json::Value::Array(branches) = value {
                    for (idx, branch) in branches.iter().enumerate() {
                        collect_schema_violations(branch, &format!("{child}[{idx}]"), out);
                    }
                }
            }
            // 单 schema 位置
            "additionalProperties"
            | "propertyNames"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else" => collect_schema_violations(value, &child, out),
            // 其余关键字（required / enum / description / $ref / minItems …）不承载子 schema
            _ => {}
        }
    }
}

/// 解析 `type` 的取值：字符串 → 单元素；字符串数组 → 多元素；其余（含 null）→ None
fn schema_type_names(kind: &serde_json::Value) -> Option<Vec<&str>> {
    match kind {
        serde_json::Value::String(name) => Some(vec![name.as_str()]),
        serde_json::Value::Array(names) => names
            .iter()
            .map(serde_json::Value::as_str)
            .collect::<Option<Vec<&str>>>(),
        _ => None,
    }
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
        let err =
            validate_tool_parameters_schema(&json!({"type": "object", "properties": properties}))
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
}
