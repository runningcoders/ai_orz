//! HTTP 工具创建表单（Modal）
//!
//! 对齐后端 `CreateToolRequest` + `HttpToolConfig` 结构：
//! 基础信息（name/description/tags）+ HTTP 配置（method/url/模板/超时/SSRF 白名单）。
//! 方法白名单与后端一致：仅 GET/POST。

use dioxus::prelude::*;

use crate::api::finance::create_tool;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::CreateToolRequest;
use common::enums::ToolProtocol;

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

/// 校验并构造 CreateToolRequest（纯函数，便于单测）
pub fn build_create_request(form: &HttpToolFormState) -> Result<CreateToolRequest, String> {
    if form.name.trim().is_empty() {
        return Err("名称不能为空".to_string());
    }
    if form.url.trim().is_empty() {
        return Err("URL 模板不能为空".to_string());
    }
    if form.method != "GET" && form.method != "POST" {
        return Err("方法仅支持 GET/POST".to_string());
    }

    let headers = parse_optional_json(&form.headers, "headers")?;
    let query = parse_optional_json(&form.query, "query")?;
    let body = parse_optional_json(&form.body, "body")?;
    let timeout_ms = parse_optional_u64(&form.timeout_ms, "超时时间")?;
    let response_max_bytes = parse_optional_u64(&form.response_max_bytes, "响应上限字节数")?;
    let allowed_status_codes = parse_status_codes(&form.allowed_status_codes)?;
    let parameters_schema = parse_optional_json(&form.parameters_schema, "参数 Schema")?;

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

    let config = serde_json::json!({
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
                        placeholder: r#"{{"Authorization": "Bearer ..."}}"#,
                        value: "{form.read().headers}",
                        oninput: move |e| form.write().headers = e.value(),
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_form() -> HttpToolFormState {
        HttpToolFormState {
            name: "weather_query".to_string(),
            url: "https://api.example.com/weather".to_string(),
            method: "GET".to_string(),
            ..Default::default()
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
}
