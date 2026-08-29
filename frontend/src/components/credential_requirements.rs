//! 凭据需求只读表格（工具详情 / MCP server 详情共用）
//!
//! 消费 `common::models::CredentialRequirement`（类型级声明，全部字段非敏感直接展示）：
//! kind / platform / field / enhancer / binding 注入点五列；
//! 注入点格式化为「Env: NAME」之类可读文本（serde 内部 tag 值 + 名称/字段）。

use common::models::{CredentialBinding, CredentialEnhancerKind, CredentialRequirement};
use dioxus::prelude::*;

/// 格式化注入点为可读文本（Env/Header/Query 按注入名，Internal 按工具实例字段名）
pub fn format_binding(binding: &CredentialBinding) -> String {
    match binding {
        CredentialBinding::Env { name } => format!("Env: {name}"),
        CredentialBinding::Header { name } => format!("Header: {name}"),
        CredentialBinding::Query { name } => format!("Query: {name}"),
        CredentialBinding::Internal { field } => format!("Internal: {field}"),
    }
}

/// 格式化增强器（None = 规范可用值，展示占位符；值与 serde snake_case 值空间一致）
pub fn format_enhancer(enhancer: Option<CredentialEnhancerKind>) -> String {
    match enhancer {
        None => "—".to_string(),
        Some(CredentialEnhancerKind::BearerToken) => "bearer_token".to_string(),
        Some(CredentialEnhancerKind::BasicAuth) => "basic_auth".to_string(),
        Some(CredentialEnhancerKind::AccessToken) => "access_token".to_string(),
    }
}

/// 可选字段占位（None → 「—」）
fn optional_value(value: &Option<String>) -> String {
    value
        .as_deref()
        .map_or_else(|| "—".to_string(), str::to_string)
}

/// 凭据需求只读表格（空列表由调用方决定不渲染卡片）
#[component]
pub fn CredentialRequirementsTable(requirements: Vec<CredentialRequirement>) -> Element {
    rsx! {
        div { class: "overflow-x-auto",
            table { class: "table hud-table table-zebra table-sm",
                thead { tr {
                    th { "凭据类型" }
                    th { "平台" }
                    th { "提取字段" }
                    th { "增强器" }
                    th { "注入点" }
                }}
                tbody {
                    for requirement in requirements.iter() {
                        tr {
                            td { span { class: "badge hud-badge badge-neutral badge-sm", "{requirement.kind.as_str()}" } }
                            td { {optional_value(&requirement.platform)} }
                            td { {optional_value(&requirement.field)} }
                            td { {format_enhancer(requirement.enhancer)} }
                            td { class: "font-mono text-xs", {format_binding(&requirement.binding)} }
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

    #[test]
    fn format_binding_renders_all_variants() {
        assert_eq!(
            format_binding(&CredentialBinding::Env {
                name: "GITHUB_TOKEN".to_string()
            }),
            "Env: GITHUB_TOKEN"
        );
        assert_eq!(
            format_binding(&CredentialBinding::Header {
                name: "authorization".to_string()
            }),
            "Header: authorization"
        );
        assert_eq!(
            format_binding(&CredentialBinding::Query {
                name: "api_key".to_string()
            }),
            "Query: api_key"
        );
        assert_eq!(
            format_binding(&CredentialBinding::Internal {
                field: "app_id".to_string()
            }),
            "Internal: app_id"
        );
    }

    #[test]
    fn format_enhancer_covers_all_kinds() {
        assert_eq!(format_enhancer(None), "—");
        assert_eq!(
            format_enhancer(Some(CredentialEnhancerKind::BearerToken)),
            "bearer_token"
        );
        assert_eq!(
            format_enhancer(Some(CredentialEnhancerKind::BasicAuth)),
            "basic_auth"
        );
        assert_eq!(
            format_enhancer(Some(CredentialEnhancerKind::AccessToken)),
            "access_token"
        );
    }

    #[test]
    fn optional_value_uses_placeholder_for_none() {
        assert_eq!(optional_value(&None), "—");
        assert_eq!(optional_value(&Some("linear".to_string())), "linear");
    }
}
