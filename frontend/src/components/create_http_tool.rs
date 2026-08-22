//! HTTP 工具创建表单（Modal）
//!
//! 对齐后端 `CreateToolRequest` 与 `HttpToolConfig` 结构：基础信息（name/description/tags）、
//! HTTP 配置（method/url/模板/超时/SSRF 白名单）、凭据需求动态列表（HttpTool scope：Header/Query binding）。
//!
//! 方法白名单委托 common 单点 `is_supported_http_method`（仅 GET/POST，与后端
//! `parse_supported_method` 共用）；headers/query 模板敏感名与后端
//! `validate_no_sensitive_template_keys` 同判（只能经凭据需求注入，D15）。

use dioxus::prelude::*;

use crate::api::finance::create_tool;
use crate::components::credential_form::{
    available_enhancers, binding_name, enhancer_display, enhancer_from_value,
    enhancer_to_value, has_any_enhancer_support, injection_value_preview, is_sensitive_name,
    kind_from_value, normalize_requirements, recommended_binding_name,
    validate_requirements_scoped,
};
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::CreateToolRequest;
use common::enums::ToolProtocol;
use common::models::{
    CredentialBinding, CredentialKind, CredentialRequirement, CredentialRequirementScope,
    enhancer_supports,
};

/// HTTP 工具表单状态（全部为文本输入，提交时统一解析校验）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HttpToolFormState {
    pub name: String,
    pub description: String,
    pub tags: String,
    pub method: String,
    pub url: String,
    pub headers: String,
    pub query: String,
    pub body: String,
    pub timeout_ms: String,
    pub response_max_bytes: String,
    pub allowed_status_codes: String,
    pub response_json_pointer: String,
    pub allowed_domains: String,
    pub blocked_domains: String,
    pub allow_local_network: bool,
    pub parameters_schema: String,
    pub credential_requirements: Vec<CredentialRequirement>,
}

/// 解析可选 JSON 文本域：空白返回 None，非法 JSON 返回错误信息
pub fn parse_optional_json(text: &str, field: &str) -> Result<Option<serde_json::Value>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(trimmed).map_err(|e| format!("{} 不是合法 JSON: {}", field, e))
}

/// 解析逗号分隔列表（忽略空白项）
pub fn parse_comma_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 解析可选 u64 数字输入
pub fn parse_optional_u64(text: &str, field: &str) -> Result<Option<u64>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u64>()
        .map(Some)
        .map_err(|_| format!("{} 必须是正整数", field))
}

/// 解析逗号分隔的状态码列表
pub fn parse_status_codes(text: &str) -> Result<Option<Vec<u16>>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut codes = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let code: u16 = part
            .parse()
            .map_err(|_| format!("状态码 {} 不是合法数字", part))?;
        codes.push(code);
    }
    Ok(Some(codes))
}

/// headers/query 模板文本中的敏感名 key 列表（None = 无法判定：空白/非法 JSON/非对象）
pub fn find_sensitive_json_keys(text: &str) -> Option<Vec<String>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let serde_json::Value::Object(object) = value else {
        return None;
    };
    Some(
        object
            .keys()
            .filter(|k| is_sensitive_name(k))
            .cloned()
            .collect(),
    )
}

/// headers/query 模板文本域的敏感名即时提示文案（None = 不提示）
pub fn sensitive_keys_hint(text: &str) -> Option<String> {
    let keys = find_sensitive_json_keys(text)?;
    if keys.is_empty() {
        return None;
    }
    Some(format!(
        "⚠ 敏感头只能通过凭据需求注入：{}",
        keys.join("、")
    ))
}

/// 提交期敏感名拦截（对齐后端 `validate_no_sensitive_template_keys`）
fn validate_no_sensitive_keys(
    field_name: &str,
    template: &Option<serde_json::Value>,
) -> Result<(), String> {
    let Some(serde_json::Value::Object(object)) = template else {
        return Ok(());
    };
    for key in object.keys() {
        if is_sensitive_name(key) {
            return Err(format!(
                "{} 中的 {} 是敏感名，敏感头只能通过凭据需求注入",
                field_name, key
            ));
        }
    }
    Ok(())
}

/// 校验并构造 CreateToolRequest（纯函数，便于单测）
pub fn build_create_request(form: &HttpToolFormState) -> Result<CreateToolRequest, String> {
    if form.name.trim().is_empty() {
        return Err("名称不能为空".to_string());
    }
    if form.url.trim().is_empty() {
        return Err("URL 模板不能为空".to_string());
    }
    // 白名单单点在 common `is_supported_http_method`（与后端 parse_supported_method 共用）
    if !common::models::is_supported_http_method(&form.method) {
        return Err("方法仅支持 GET/POST".to_string());
    }

    let headers = parse_optional_json(&form.headers, "headers")?;
    let query = parse_optional_json(&form.query, "query")?;
    // 敏感名二次拦截（输入期已有即时提示，此处兜底）
    validate_no_sensitive_keys("headers", &headers)?;
    validate_no_sensitive_keys("query", &query)?;
    let body = parse_optional_json(&form.body, "body")?;
    let timeout_ms = parse_optional_u64(&form.timeout_ms, "超时时间")?;
    let response_max_bytes = parse_optional_u64(&form.response_max_bytes, "响应上限字节数")?;
    let allowed_status_codes = parse_status_codes(&form.allowed_status_codes)?;
    let parameters_schema = parse_optional_json(&form.parameters_schema, "参数 Schema")?;

    // 凭据需求预校验（规范化后执行；HTTP 工具恒 HttpTool scope：仅 Header/Query）
    let requirements = normalize_requirements(form.credential_requirements.clone());
    validate_requirements_scoped(&requirements, CredentialRequirementScope::HttpTool)?;

    let response_json_pointer = {
        let p = form.response_json_pointer.trim();
        if p.is_empty() {
            None
        } else {
            Some(p.to_string())
        }
    };
    let allowed_domains = parse_comma_list(&form.allowed_domains);
    let blocked_domains = parse_comma_list(&form.blocked_domains);
    let tags = parse_comma_list(&form.tags);

    let mut config = serde_json::json!({
        "method": form.method,
        "url": form.url.trim(),
        "headers": headers,
        "query": query,
        "body": body,
        "timeout_ms": timeout_ms,
        "response_max_bytes": response_max_bytes,
        "allowed_status_codes": allowed_status_codes,
        "response_json_pointer": response_json_pointer,
        "allowed_domains": if allowed_domains.is_empty() { None } else { Some(allowed_domains) },
        "blocked_domains": if blocked_domains.is_empty() { None } else { Some(blocked_domains) },
        "allow_local_network": form.allow_local_network,
    });
    if !requirements.is_empty() {
        config["credential_requirements"] = serde_json::to_value(&requirements)
            .map_err(|e| format!("凭据需求序列化失败: {}", e))?;
    }

    Ok(CreateToolRequest {
        name: form.name.trim().to_string(),
        description: form.description.trim().to_string(),
        protocol: ToolProtocol::Http,
        config: Some(config),
        parameters_schema,
        tags: if tags.is_empty() { None } else { Some(tags) },
        control_mode: None,
        enabled: None,
    })
}

/// HTTP 工具创建弹窗
#[component]
pub fn CreateHttpToolModal(
    show: bool,
    on_close: EventHandler<()>,
    on_created: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let mut form = use_signal(HttpToolFormState::default);
    let mut error_msg = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    // 关闭时重置表单
    use_effect(move || {
        if !show {
            form.set(HttpToolFormState::default());
            error_msg.set(String::new());
        }
    });

    let submit = move |_| {
        let req = match build_create_request(&form.read()) {
            Ok(req) => req,
            Err(e) => {
                error_msg.set(e);
                return;
            }
        };
        error_msg.set(String::new());
        submitting.set(true);
        spawn(async move {
            match create_tool(req).await {
                Ok(resp) => {
                    toast.success(format!("工具 {} 创建成功", resp.name));
                    submitting.set(false);
                    on_created.call(());
                    on_close.call(());
                }
                Err(e) => {
                    error_msg.set(format!("创建失败: {}", e));
                    submitting.set(false);
                }
            }
        });
    };

    // 敏感名即时提示（输入期预检）
    let headers_sensitive_hint = sensitive_keys_hint(&form.read().headers);
    let query_sensitive_hint = sensitive_keys_hint(&form.read().query);
    let requirements_list = form.read().credential_requirements.clone();

    let on_add_requirement = move |_| {
        form.write().credential_requirements.push(CredentialRequirement {
            kind: CredentialKind::GithubToken,
            platform: None,
            field: None,
            enhancer: None,
            binding: CredentialBinding::Header {
                name: String::new(),
            },
        });
    };

    rsx! {
        Modal {
            title: "创建 HTTP 工具".to_string(),
            show,
            on_close: move |_| on_close.call(()),
            footer: Some(rsx! {
                button {
                    class: "btn btn-primary",
                    disabled: submitting(),
                    onclick: submit,
                    if submitting() { "创建中..." } else { "创建" }
                }
            }),
            div { class: "max-h-[70vh] overflow-y-auto space-y-3 pr-1",
                if !error_msg().is_empty() {
                    div { class: "alert alert-error py-2 text-sm", "{error_msg}" }
                }

                // ===== 基础信息 =====
                div { class: "form-control",
                    label { class: "form-label", "名称 *" }
                    input {
                        class: "input input-bordered w-full",
                        placeholder: "例如：weather_query",
                        value: "{form.read().name}",
                        oninput: move |e| form.write().name = e.value(),
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "描述" }
                    textarea {
                        class: "textarea textarea-bordered w-full",
                        rows: 2,
                        placeholder: "工具用途说明（供 Agent 理解何时调用）",
                        value: "{form.read().description}",
                        oninput: move |e| form.write().description = e.value(),
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "标签（逗号分隔）" }
                    input {
                        class: "input input-bordered w-full",
                        placeholder: "例如：weather,query",
                        value: "{form.read().tags}",
                        oninput: move |e| form.write().tags = e.value(),
                    }
                }

                // ===== HTTP 配置 =====
                div { class: "flex gap-3",
                    div { class: "form-control w-32",
                        label { class: "form-label", "方法 *" }
                        select {
                            class: "select select-bordered w-full",
                            value: "{form.read().method}",
                            onchange: move |e| form.write().method = e.value(),
                            option { value: "GET", "GET" }
                            option { value: "POST", "POST" }
                        }
                    }
                    div { class: "form-control flex-1",
                        label { class: "form-label", "URL 模板 *" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "https://api.example.com/v1/weather?city={{city}}",
                            value: "{form.read().url}",
                            oninput: move |e| form.write().url = e.value(),
                        }
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "Headers 模板（JSON，可选）" }
                    textarea {
                        class: "textarea textarea-bordered w-full font-mono text-xs",
                        rows: 2,
                        placeholder: r#"{{"Content-Type": "application/json"}}"#,
                        value: "{form.read().headers}",
                        oninput: move |e| form.write().headers = e.value(),
                    }
                    if let Some(hint) = headers_sensitive_hint {
                        p { class: "text-xs text-error mt-1", "{hint}" }
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "Query 模板（JSON，可选）" }
                    textarea {
                        class: "textarea textarea-bordered w-full font-mono text-xs",
                        rows: 2,
                        placeholder: r#"{{"city": "{{city}}"}}"#,
                        value: "{form.read().query}",
                        oninput: move |e| form.write().query = e.value(),
                    }
                    if let Some(hint) = query_sensitive_hint {
                        p { class: "text-xs text-error mt-1", "{hint}" }
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "Body 模板（JSON，可选，POST 用）" }
                    textarea {
                        class: "textarea textarea-bordered w-full font-mono text-xs",
                        rows: 2,
                        placeholder: r#"{{"query": "{{q}}"}}"#,
                        value: "{form.read().body}",
                        oninput: move |e| form.write().body = e.value(),
                    }
                }
                div { class: "flex gap-3",
                    div { class: "form-control flex-1",
                        label { class: "form-label", "超时（ms，可选）" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "30000",
                            value: "{form.read().timeout_ms}",
                            oninput: move |e| form.write().timeout_ms = e.value(),
                        }
                    }
                    div { class: "form-control flex-1",
                        label { class: "form-label", "响应上限（字节，可选）" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "1048576",
                            value: "{form.read().response_max_bytes}",
                            oninput: move |e| form.write().response_max_bytes = e.value(),
                        }
                    }
                }
                div { class: "flex gap-3",
                    div { class: "form-control flex-1",
                        label { class: "form-label", "允许状态码（逗号分隔，可选）" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "200,201",
                            value: "{form.read().allowed_status_codes}",
                            oninput: move |e| form.write().allowed_status_codes = e.value(),
                        }
                    }
                    div { class: "form-control flex-1",
                        label { class: "form-label", "响应 JSON Pointer（可选）" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "/data",
                            value: "{form.read().response_json_pointer}",
                            oninput: move |e| form.write().response_json_pointer = e.value(),
                        }
                    }
                }
                div { class: "flex gap-3",
                    div { class: "form-control flex-1",
                        label { class: "form-label", "域名白名单（逗号分隔，可选）" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "api.example.com",
                            value: "{form.read().allowed_domains}",
                            oninput: move |e| form.write().allowed_domains = e.value(),
                        }
                    }
                    div { class: "form-control flex-1",
                        label { class: "form-label", "域名黑名单（逗号分隔，可选）" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "internal.example.com",
                            value: "{form.read().blocked_domains}",
                            oninput: move |e| form.write().blocked_domains = e.value(),
                        }
                    }
                }
                label { class: "label cursor-pointer justify-start gap-3",
                    input {
                        "type": "checkbox",
                        class: "checkbox checkbox-sm",
                        checked: form.read().allow_local_network,
                        onchange: move |_| {
                            let v = !form.read().allow_local_network;
                            form.write().allow_local_network = v;
                        },
                    }
                    span { class: "label-text", "允许访问本机/内网地址（SSRF 风险确认）" }
                }
                div { class: "form-control",
                    label { class: "form-label", "参数 Schema（JSON Schema，可选）" }
                    textarea {
                        class: "textarea textarea-bordered w-full font-mono text-xs",
                        rows: 3,
                        placeholder: r#"{{"type":"object","properties":{{"city":{{"type":"string"}}}},"required":["city"]}}"#,
                        value: "{form.read().parameters_schema}",
                        oninput: move |e| form.write().parameters_schema = e.value(),
                    }
                }

                // ===== 凭据需求动态列表（HttpTool scope：Header/Query binding） =====
                div { class: "form-control",
                    div { class: "flex justify-between items-center",
                        span { class: "label-text font-medium", "凭据需求（可选）" }
                        button { class: "btn btn-ghost btn-xs", onclick: on_add_requirement, "＋ 添加" }
                    }
                    p { class: "text-xs text-base-content/60 mt-1",
                        "声明该工具所需凭据类型与注入点（请求头/查询参数）；调用时以调用者身份自动注入（类型级声明，不绑定具体凭据实例）。" }
                    if requirements_list.is_empty() {
                        p { class: "text-xs text-base-content/40 mt-2", "暂未声明凭据需求" }
                    } else {
                        div { class: "mt-2 space-y-2",
                            for (idx, req) in requirements_list.iter().enumerate() {
                                {
                                    let kind_value = req.kind.as_str();
                                    let platform_value = req.platform.clone().unwrap_or_default();
                                    let field_value = req.field.clone().unwrap_or_default();
                                    let enhancer_value = req.enhancer.map_or("none", enhancer_to_value);
                                    let enhancer_opts = available_enhancers(req.kind);
                                    let enhancer_disabled = req.field.is_some() || enhancer_opts.is_empty();
                                    let field_disabled = req.enhancer.is_some();
                                    let requires_platform = req.kind.requires_platform();
                                    let is_query_binding = matches!(req.binding, CredentialBinding::Query { .. });
                                    let binding_kind_value = if is_query_binding { "query" } else { "header" };
                                    let binding_name_value = binding_name(&req.binding).to_string();
                                    let binding_name_label = if is_query_binding {
                                        "注入查询参数名（Query）*"
                                    } else {
                                        "注入请求头名（Header）*"
                                    };
                                    let binding_name_placeholder = if is_query_binding {
                                        "如 api_key"
                                    } else {
                                        "如 authorization"
                                    };
                                    let preview = injection_value_preview(req);
                                    let name_recommendation = if binding_name_value.is_empty() {
                                        recommended_binding_name(req)
                                    } else {
                                        None
                                    };
                                    let idx_remove = idx;
                                    let idx_kind = idx;
                                    let idx_platform = idx;
                                    let idx_field = idx;
                                    let idx_enhancer = idx;
                                    let idx_binding_kind = idx;
                                    let idx_binding_name = idx;

                                    rsx! {
                                        div { class: "border border-base-300 rounded-box p-3 space-y-2", key: "{idx}",
                                            div { class: "flex justify-between items-center",
                                                span { class: "text-xs font-semibold text-base-content/60", "需求 #{idx + 1}" }
                                                button { class: "btn btn-ghost btn-xs text-error",
                                                    onclick: move |_| {
                                                        form.write().credential_requirements.remove(idx_remove);
                                                    }, "✕ 移除" }
                                            }
                                            div { class: "flex gap-2 flex-wrap",
                                                div { class: "form-control flex-1 min-w-[10rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "凭据类型 *" }
                                                    }
                                                    select { class: "select select-bordered select-sm w-full", value: "{kind_value}",
                                                        onchange: move |e| {
                                                            let kind = kind_from_value(&e.value()).unwrap_or(CredentialKind::GithubToken);
                                                            let mut f = form.write();
                                                            let r = &mut f.credential_requirements[idx_kind];
                                                            r.kind = kind;
                                                            // 联动重置：专用类清空 platform；不支持当前增强器时清空
                                                            if !kind.requires_platform() {
                                                                r.platform = None;
                                                            }
                                                            if r.enhancer.is_some_and(|en| !enhancer_supports(kind, en)) {
                                                                r.enhancer = None;
                                                            }
                                                        },
                                                        option { value: "lark_app", "lark_app（飞书应用）" }
                                                        option { value: "github_token", "github_token（GitHub 令牌）" }
                                                        option { value: "tavily_key", "tavily_key（Tavily Key）" }
                                                        option { value: "generic_token", "generic_token（通用平台令牌）" }
                                                        option { value: "oauth", "oauth（OAuth 刷新凭据）" }
                                                        option { value: "user_password", "user_password（用户名密码）" }
                                                    }
                                                }
                                                if requires_platform {
                                                    div { class: "form-control flex-1 min-w-[8rem]",
                                                        label { class: "label",
                                                            span { class: "label-text text-xs", "平台标识 *" }
                                                        }
                                                        input { class: "input input-bordered input-sm w-full", value: "{platform_value}",
                                                            oninput: move |e| {
                                                                let v = e.value();
                                                                let mut f = form.write();
                                                                f.credential_requirements[idx_platform].platform =
                                                                    if v.is_empty() { None } else { Some(v) };
                                                            },
                                                            placeholder: "如 linear / notion" }
                                                    }
                                                }
                                            }
                                            div { class: "flex gap-2 flex-wrap",
                                                div { class: "form-control flex-1 min-w-[8rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "提取字段（可选）" }
                                                    }
                                                    input { class: "input input-bordered input-sm w-full", value: "{field_value}",
                                                        disabled: field_disabled,
                                                        oninput: move |e| {
                                                            let v = e.value();
                                                            let mut f = form.write();
                                                            let r = &mut f.credential_requirements[idx_field];
                                                            if v.is_empty() {
                                                                r.field = None;
                                                            } else {
                                                                // 互斥防御：填写字段时清空增强器
                                                                r.field = Some(v);
                                                                r.enhancer = None;
                                                            }
                                                        },
                                                        placeholder: "如 token / api_key" }
                                                }
                                                div { class: "form-control flex-1 min-w-[8rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "增强器（可选）" }
                                                    }
                                                    select { class: "select select-bordered select-sm w-full", value: "{enhancer_value}",
                                                        disabled: enhancer_disabled,
                                                        onchange: move |e| {
                                                            let enhancer = enhancer_from_value(&e.value());
                                                            let mut f = form.write();
                                                            let r = &mut f.credential_requirements[idx_enhancer];
                                                            r.enhancer = enhancer;
                                                            // 互斥防御：选择增强器时清空提取字段
                                                            if enhancer.is_some() {
                                                                r.field = None;
                                                            }
                                                        },
                                                        option { value: "none", "不使用增强器" }
                                                        for e in enhancer_opts.iter() {
                                                            option { value: "{enhancer_to_value(*e)}", "{enhancer_display(*e)}" }
                                                        }
                                                    }
                                                    if enhancer_disabled {
                                                        span { class: "text-xs text-base-content/50",
                                                            { if req.field.is_some() {
                                                                "已指定提取字段，二选一".to_string()
                                                            } else if has_any_enhancer_support(req.kind) {
                                                                "默认增强器自动生效，无需选择".to_string()
                                                            } else {
                                                                "该凭据类型不适用增强器".to_string()
                                                            } }
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "flex gap-2 flex-wrap",
                                                div { class: "form-control w-40",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "注入点类型 *" }
                                                    }
                                                    select { class: "select select-bordered select-sm w-full", value: "{binding_kind_value}",
                                                        onchange: move |e| {
                                                            // 变体切换：保留已填注入名
                                                            let name = {
                                                                let f = form.read();
                                                                binding_name(&f.credential_requirements[idx_binding_kind].binding).to_string()
                                                            };
                                                            let binding = if e.value() == "query" {
                                                                CredentialBinding::Query { name }
                                                            } else {
                                                                CredentialBinding::Header { name }
                                                            };
                                                            form.write().credential_requirements[idx_binding_kind].binding = binding;
                                                        },
                                                        option { value: "header", "Header（请求头）" }
                                                        option { value: "query", "Query（查询参数）" }
                                                    }
                                                }
                                                div { class: "form-control flex-1 min-w-[8rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "{binding_name_label}" }
                                                    }
                                                    input { class: "input input-bordered input-sm w-full", value: "{binding_name_value}",
                                                        oninput: move |e| {
                                                            let v = e.value();
                                                            let mut f = form.write();
                                                            match &mut f.credential_requirements[idx_binding_name].binding {
                                                                CredentialBinding::Env { name }
                                                                | CredentialBinding::Header { name }
                                                                | CredentialBinding::Query { name } => *name = v,
                                                                CredentialBinding::Internal { field } => *field = v,
                                                            }
                                                        },
                                                        placeholder: "{binding_name_placeholder}" }
                                                }
                                            }
                                            // ===== 注入值预览 + 惯用名建议（只读示意） =====
                                            div { class: "flex gap-2 flex-wrap items-center text-xs text-base-content/50",
                                                span {
                                                    code { class: "bg-base-200 rounded px-1", "{preview}" }
                                                    " ← 注入值形态"
                                                }
                                                if let Some(rec) = name_recommendation {
                                                    span { class: "text-info",
                                                        "建议注入名：{rec}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::models::CredentialEnhancerKind;

    fn base_form() -> HttpToolFormState {
        HttpToolFormState {
            name: "weather_query".to_string(),
            url: "https://api.example.com/weather".to_string(),
            method: "GET".to_string(),
            ..Default::default()
        }
    }

    fn header_req() -> CredentialRequirement {
        CredentialRequirement {
            kind: CredentialKind::GithubToken,
            platform: None,
            field: None,
            enhancer: None,
            binding: CredentialBinding::Header {
                name: "authorization".to_string(),
            },
        }
    }

    #[test]
    fn build_request_requires_name_and_url() {
        let mut form = base_form();
        form.name = String::new();
        assert!(build_create_request(&form).is_err());

        let mut form = base_form();
        form.url = String::new();
        assert!(build_create_request(&form).is_err());
    }

    #[test]
    fn build_request_rejects_unsupported_method() {
        let mut form = base_form();
        form.method = "DELETE".to_string();
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("GET/POST"));
    }

    #[test]
    fn build_request_parses_full_config() {
        let mut form = base_form();
        form.tags = "weather, query".to_string();
        form.headers = r#"{"X-Api": "k"}"#.to_string();
        form.timeout_ms = "30000".to_string();
        form.allowed_status_codes = "200,201".to_string();
        form.allowed_domains = "api.example.com, ".to_string();
        form.parameters_schema = r#"{"type":"object"}"#.to_string();

        let req = build_create_request(&form).expect("valid form should build");
        assert_eq!(req.name, "weather_query");
        assert_eq!(
            req.tags,
            Some(vec!["weather".to_string(), "query".to_string()])
        );
        assert!(req.parameters_schema.is_some());

        let config = req.config.expect("config should be present");
        assert_eq!(config.get("method").and_then(|v| v.as_str()), Some("GET"));
        assert_eq!(
            config.get("timeout_ms").and_then(|v| v.as_u64()),
            Some(30000)
        );
        let codes = config
            .get("allowed_status_codes")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(codes.len(), 2);
        let domains = config
            .get("allowed_domains")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(domains.len(), 1);
        // 空凭据需求不输出 key（对齐后端 skip_serializing_if）
        assert!(config.get("credential_requirements").is_none());
    }

    #[test]
    fn invalid_json_field_reports_error() {
        let mut form = base_form();
        form.headers = "{invalid".to_string();
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("headers"));
    }

    #[test]
    fn invalid_number_field_reports_error() {
        let mut form = base_form();
        form.timeout_ms = "abc".to_string();
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("超时时间"));
    }

    #[test]
    fn parse_comma_list_trims_and_filters_empty() {
        assert_eq!(
            parse_comma_list(" a , b ,,c "),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(parse_comma_list(" , ").is_empty());
    }

    // ===== 敏感名预检 =====

    #[test]
    fn build_request_rejects_sensitive_header_key() {
        let mut form = base_form();
        form.headers = r#"{"Authorization": "Bearer xxx"}"#.to_string();
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("敏感"), "unexpected: {err}");
        assert!(err.contains("Authorization"), "unexpected: {err}");
    }

    #[test]
    fn build_request_rejects_sensitive_query_key() {
        let mut form = base_form();
        form.query = r#"{"token": "{{token}}" }"#.to_string();
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("token"), "unexpected: {err}");
    }

    #[test]
    fn sensitive_keys_hint_reports_live() {
        // 命中：含敏感 key 的合法 JSON
        let hint = sensitive_keys_hint(r#"{"X-Token": "v", "Accept": "json"}"#).unwrap();
        assert!(hint.contains("X-Token"), "unexpected: {hint}");
        assert!(hint.contains("敏感头只能通过凭据需求注入"));
        // 合法 JSON 无敏感 key → 不提示
        assert!(sensitive_keys_hint(r#"{"Content-Type": "application/json"}"#).is_none());
        // 空白 / 非法 JSON / 非对象 → 不提示（合法性由既有 JSON 校验负责）
        assert!(sensitive_keys_hint("   ").is_none());
        assert!(sensitive_keys_hint("{invalid").is_none());
        assert!(sensitive_keys_hint(r#""scalar""#).is_none());
        // find_sensitive_json_keys：None 表示无法判定
        assert!(find_sensitive_json_keys("{invalid").is_none());
        assert_eq!(
            find_sensitive_json_keys(r#"{"token": "v"}"#),
            Some(vec!["token".to_string()])
        );
    }

    // ===== 凭据需求构造与预校验 =====

    #[test]
    fn build_request_includes_credential_requirements() {
        let mut form = base_form();
        form.credential_requirements = vec![
            header_req(),
            CredentialRequirement {
                kind: CredentialKind::GenericToken,
                platform: Some("  linear ".to_string()),
                field: None,
                enhancer: Some(CredentialEnhancerKind::BearerToken),
                binding: CredentialBinding::Query {
                    name: " api_key ".to_string(),
                },
            },
        ];
        let req = build_create_request(&form).expect("valid requirements should build");
        let config = req.config.unwrap();
        let reqs = config
            .get("credential_requirements")
            .and_then(|v| v.as_array())
            .expect("credential_requirements should be present");
        assert_eq!(reqs.len(), 2);
        // Header 变体（internally tagged：{"type": "header", "name": ...}）
        assert_eq!(
            reqs[0].get("binding"),
            Some(&serde_json::json!({ "type": "header", "name": "authorization" }))
        );
        // normalize 生效：platform/api_key 已 trim
        assert_eq!(
            reqs[1].get("platform").and_then(|p| p.as_str()),
            Some("linear")
        );
        assert_eq!(
            reqs[1].get("binding"),
            Some(&serde_json::json!({ "type": "query", "name": "api_key" }))
        );
    }

    #[test]
    fn build_request_rejects_env_binding_for_http_tool() {
        let mut form = base_form();
        form.credential_requirements = vec![CredentialRequirement {
            kind: CredentialKind::GithubToken,
            platform: None,
            field: None,
            enhancer: None,
            binding: CredentialBinding::Env {
                name: "GITHUB_TOKEN".to_string(),
            },
        }];
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("仅支持请求头或查询参数"), "unexpected: {err}");
    }

    #[test]
    fn build_request_rejects_empty_binding_name() {
        let mut form = base_form();
        form.credential_requirements = vec![CredentialRequirement {
            binding: CredentialBinding::Header {
                name: "  ".to_string(),
            },
            ..header_req()
        }];
        let err = build_create_request(&form).unwrap_err();
        assert!(err.contains("注入点名"), "unexpected: {err}");
    }
}
