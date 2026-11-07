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

/// 受限工具包：Agent 不得自行安装（管理员经 HTTP 显式授权不受限）
///
/// `admin` 内含模型提供商 / MCP Server / 消息通道 / 组织 / 技能发布等管理面
/// 工具，默认不在 `BASE_AGENT_PACKS`；而 `install_tool_pack` 自身挂在
/// `tool_management`（Agent 出生可达），若无边界则 Agent 可自开 admin 包
/// 完成权限提升。封堵落在 domain，不能只靠 HTTP 路由门闩——内置工具是
/// handler 直调，不经过 axum 中间件。
pub const RESTRICTED_TOOL_PACKS: &[&str] = &["admin"];

/// Agent 自建工具禁止携带的标签（防通过 tag 把工具扩散给其他 Agent）
///
/// `neural` 恒在工具面 tag_filter（所有 Agent 天生拥有）、`internal` 为系统
/// 自用、`admin` 会被已装 admin 包的 Agent 继承——任一都能让 Agent 造出的
/// 工具越过自身授权边界。管理员 HTTP 创建不受此限（保留既有运维能力）。
pub const AGENT_FORBIDDEN_TOOL_TAGS: &[&str] = &["neural", "internal", "admin"];

/// Agent 创建工具时的标签校验（规则本体单点，供后端管理面复用）
pub fn validate_agent_created_tool_tags(tags: &[String]) -> Result<(), String> {
    let forbidden: Vec<String> = tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| AGENT_FORBIDDEN_TOOL_TAGS.contains(&tag.as_str()))
        .collect();
    if forbidden.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Agent 创建的工具不允许携带保留标签：{}",
            forbidden.join(", ")
        ))
    }
}

/// 协议 → 工具凭据需求作用域（管理期统一校验单点）
///
/// Mcp 的声明属主是 `McpServerPo.config`（同一 server 下 N 个工具共享连接属性，
/// 工具行是 sync 派生的远端镜像）；Builtin 的声明是工厂代码静态。这两类协议
/// 不经工具管理面编辑凭据需求，返回 `None`。
pub fn tool_credential_scope(
    protocol: crate::enums::ToolProtocol,
) -> Option<crate::models::CredentialRequirementScope> {
    use crate::enums::ToolProtocol;
    use crate::models::CredentialRequirementScope;
    match protocol {
        ToolProtocol::Http => Some(CredentialRequirementScope::HttpTool),
        ToolProtocol::Shell => Some(CredentialRequirementScope::Shell),
        ToolProtocol::Mcp | ToolProtocol::Builtin => None,
    }
}

/// 工具级凭据需求声明 → `ToolPo.config` 归并（创建/更新管理面共用单一实现）
///
/// 契约层（顶层 DTO 字段）与存储层（各协议 config 内同名键）在此收口：
/// - `None` = 调用方未声明，保留 config 内既有声明（兼容 HTTP 工具旧表单）
/// - `Some([])` = 显式清空（删除键）
/// - 非 object 的 config（存量 Null）先归一为空对象
///
/// 声明本身非敏感（不含 credential_id / 凭据原文，D1），归并前按协议 scope
/// 走 common 六规则单点校验。
pub fn apply_credential_requirements(
    config: &mut serde_json::Value,
    requirements: Option<&[crate::models::CredentialRequirement]>,
    scope: Option<crate::models::CredentialRequirementScope>,
) -> Result<(), String> {
    let Some(requirements) = requirements else {
        return Ok(());
    };
    let Some(scope) = scope else {
        return Err(
            "该协议的工具凭据声明不由工具行承载（MCP 请在 Server 配置，Builtin 由代码静态声明）"
                .to_string(),
        );
    };
    crate::models::identity_credentials::validate_requirements(requirements, scope)?;
    if !config.is_object() {
        *config = serde_json::Value::Object(serde_json::Map::new());
    }
    let object = config
        .as_object_mut()
        .expect("config normalized to object above");
    if requirements.is_empty() {
        object.remove("credential_requirements");
    } else {
        let value =
            serde_json::to_value(requirements).map_err(|e| format!("凭据需求序列化失败: {}", e))?;
        object.insert("credential_requirements".to_string(), value);
    }
    Ok(())
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
#[path = "tool_tests.rs"]
mod tests;
