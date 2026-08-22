//! 凭据需求表单共享纯函数（MCP 服务器 / HTTP 工具创建表单共用）
//!
//! - 预校验 [`validate_requirements_scoped`] 对齐后端
//!   `pkg::credential::validate_requirements`（src/pkg/credential/mod.rs#L250-L293），
//!   以 [`CredentialRequirementScope`] 参数化协议差异（MCP 表单经 [`mcp_transport_scope`]
//!   从传输方式推导，HTTP 工具恒为 `HttpTool`）。
//! - [`is_sensitive_name`] 委托 common 单点实现 `is_sensitive_credential_name`
//!   （common/src/models/identity_credentials.rs；与后端 `is_sensitive_header`
//!   同源零漂移）。

use common::enums::McpTransport;
use common::models::{
    CredentialBinding, CredentialEnhancerKind, CredentialKind, CredentialRequirement,
    CredentialRequirementScope, default_enhancer, enhancer_supports,
    is_sensitive_credential_name,
};

/// 全部凭据类型（kind 下拉选项，serde 值 = 展示键）
pub fn all_credential_kinds() -> [CredentialKind; 6] {
    [
        CredentialKind::LarkApp,
        CredentialKind::GithubToken,
        CredentialKind::TavilyKey,
        CredentialKind::GenericToken,
        CredentialKind::OAuth,
        CredentialKind::UserPassword,
    ]
}

/// 按 serde 值解析凭据类型
pub fn kind_from_value(v: &str) -> Option<CredentialKind> {
    all_credential_kinds().into_iter().find(|k| k.as_str() == v)
}

/// 注入点名（binding 的 name / field）
pub fn binding_name(binding: &CredentialBinding) -> &str {
    match binding {
        CredentialBinding::Env { name }
        | CredentialBinding::Header { name }
        | CredentialBinding::Query { name } => name,
        CredentialBinding::Internal { field } => field,
    }
}

/// MCP 传输方式 → 需求作用域（stdio → McpStdio / streamable_http → McpHttp）
pub fn mcp_transport_scope(transport: McpTransport) -> CredentialRequirementScope {
    match transport {
        McpTransport::Stdio => CredentialRequirementScope::McpStdio,
        McpTransport::StreamableHttp => CredentialRequirementScope::McpHttp,
    }
}

/// 规范化：trim platform/field/注入名，空白 Option 归 None
pub fn normalize_requirements(list: Vec<CredentialRequirement>) -> Vec<CredentialRequirement> {
    list.into_iter()
        .map(|mut r| {
            r.platform = r
                .platform
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty());
            r.field = r
                .field
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty());
            let name = binding_name(&r.binding).trim().to_string();
            r.binding = match r.binding {
                CredentialBinding::Env { .. } => CredentialBinding::Env { name },
                CredentialBinding::Header { .. } => CredentialBinding::Header { name },
                CredentialBinding::Query { .. } => CredentialBinding::Query { name },
                CredentialBinding::Internal { .. } => CredentialBinding::Internal { field: name },
            };
            r
        })
        .collect()
}

/// binding ↔ scope 是否匹配（对齐后端 `pkg::credential::binding_allowed`）
fn binding_allowed(binding: &CredentialBinding, scope: CredentialRequirementScope) -> bool {
    matches!(
        (binding, scope),
        (CredentialBinding::Env { .. }, CredentialRequirementScope::McpStdio)
            | (CredentialBinding::Header { .. }, CredentialRequirementScope::McpHttp)
            | (CredentialBinding::Header { .. }, CredentialRequirementScope::HttpTool)
            | (CredentialBinding::Query { .. }, CredentialRequirementScope::HttpTool)
            | (CredentialBinding::Internal { .. }, CredentialRequirementScope::Builtin)
    )
}

/// binding ↔ scope 不匹配时的具体文案
fn binding_scope_mismatch_msg(scope: CredentialRequirementScope) -> String {
    match scope {
        CredentialRequirementScope::McpStdio | CredentialRequirementScope::McpHttp => {
            "凭据注入点与传输方式不匹配（Stdio 仅支持环境变量注入，StreamableHttp 仅支持请求头注入）".to_string()
        }
        CredentialRequirementScope::HttpTool => {
            "凭据注入点与工具协议不匹配（HTTP 工具仅支持请求头或查询参数注入）".to_string()
        }
        CredentialRequirementScope::Builtin => {
            "凭据注入点与工具协议不匹配（内置工具仅支持实例字段注入）".to_string()
        }
    }
}

/// 前端预校验（后端 `validate_requirements` 的等价实现；失败返回具体错误文案）
///
/// 六条规则：binding↔scope / 注入名非空 / platform↔kind / field↔enhancer 互斥 /
/// enhancer↔kind supports 矩阵 / (kind, platform, 注入名) 三元组去重。
pub fn validate_requirements_scoped(
    requirements: &[CredentialRequirement],
    scope: CredentialRequirementScope,
) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for req in requirements {
        // 1. binding ↔ scope
        if !binding_allowed(&req.binding, scope) {
            return Err(binding_scope_mismatch_msg(scope));
        }
        // 2. 注入名非空
        if binding_name(&req.binding).trim().is_empty() {
            return Err("凭据注入点名不能为空".to_string());
        }
        // 3. platform ↔ kind（generic 类必填、专用类必空）
        if req.kind.requires_platform() != req.platform.is_some() {
            return Err(if req.kind.requires_platform() {
                format!("凭据类型 {} 必须填写平台标识", req.kind.as_str())
            } else {
                format!("凭据类型 {} 不适用平台标识，请清空", req.kind.as_str())
            });
        }
        // 4. field ↔ enhancer 互斥
        if req.field.is_some() && req.enhancer.is_some() {
            return Err(format!(
                "凭据类型 {} 的提取字段与增强器互斥，只能二选一",
                req.kind.as_str()
            ));
        }
        // 5. enhancer ↔ kind supports 矩阵
        if let Some(enhancer) = req.enhancer
            && !enhancer_supports(req.kind, enhancer)
        {
            return Err(format!(
                "凭据类型 {} 不支持增强器 {}",
                req.kind.as_str(),
                enhancer_to_value(enhancer)
            ));
        }
        // 6. (kind, platform, 注入名) 三元组去重
        let key = (
            req.kind,
            req.platform.clone(),
            binding_name(&req.binding).to_string(),
        );
        if !seen.insert(key) {
            return Err("存在重复的凭据需求（同凭据类型 + 同平台 + 同注入点）".to_string());
        }
    }
    Ok(())
}

/// 该类型是否存在任一受支持增强器（专用 kind 为 false，用于禁用提示区分）
pub fn has_any_enhancer_support(kind: CredentialKind) -> bool {
    all_enhancers()
        .iter()
        .any(|e| enhancer_supports(kind, *e))
}

/// 可选增强器列表（按 supports 矩阵过滤且排除默认增强器，D11 前端不暴露默认项）
pub fn available_enhancers(kind: CredentialKind) -> Vec<CredentialEnhancerKind> {
    all_enhancers()
        .into_iter()
        .filter(|e| enhancer_supports(kind, *e) && default_enhancer(kind) != Some(*e))
        .collect()
}

pub fn all_enhancers() -> [CredentialEnhancerKind; 3] {
    [
        CredentialEnhancerKind::BearerToken,
        CredentialEnhancerKind::BasicAuth,
        CredentialEnhancerKind::AccessToken,
    ]
}

/// 增强器下拉值（与 serde snake_case 值空间一致）
pub fn enhancer_to_value(e: CredentialEnhancerKind) -> &'static str {
    match e {
        CredentialEnhancerKind::BearerToken => "bearer_token",
        CredentialEnhancerKind::BasicAuth => "basic_auth",
        CredentialEnhancerKind::AccessToken => "access_token",
    }
}

/// 按下拉值解析增强器（"none" → None）
pub fn enhancer_from_value(v: &str) -> Option<CredentialEnhancerKind> {
    all_enhancers()
        .into_iter()
        .find(|e| enhancer_to_value(*e) == v)
}

/// 敏感名判定（委托 common 单点 `is_sensitive_credential_name`，双端同源零漂移；
/// 规则：authorization/cookie/set-cookie 精确匹配，api-key/token/secret/password 子串匹配，忽略大小写）
pub fn is_sensitive_name(name: &str) -> bool {
    is_sensitive_credential_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(
        kind: CredentialKind,
        platform: Option<&str>,
        field: Option<&str>,
        enhancer: Option<CredentialEnhancerKind>,
        binding: CredentialBinding,
    ) -> CredentialRequirement {
        CredentialRequirement {
            kind,
            platform: platform.map(|s| s.to_string()),
            field: field.map(|s| s.to_string()),
            enhancer,
            binding,
        }
    }

    fn env(name: &str) -> CredentialBinding {
        CredentialBinding::Env {
            name: name.to_string(),
        }
    }

    fn header(name: &str) -> CredentialBinding {
        CredentialBinding::Header {
            name: name.to_string(),
        }
    }

    fn query(name: &str) -> CredentialBinding {
        CredentialBinding::Query {
            name: name.to_string(),
        }
    }

    // ===== validate_requirements_scoped 规则矩阵 =====

    #[test]
    fn validate_accepts_valid_requirements() {
        let list = vec![
            req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                env("GITHUB_TOKEN"),
            ),
            req(
                CredentialKind::GenericToken,
                Some("linear"),
                Some("token"),
                None,
                env("LINEAR_TOKEN"),
            ),
            req(
                CredentialKind::OAuth,
                Some("okta"),
                None,
                Some(CredentialEnhancerKind::BearerToken),
                env("OKTA_TOKEN"),
            ),
        ];
        assert!(
            validate_requirements_scoped(&list, CredentialRequirementScope::McpStdio).is_ok()
        );
    }

    #[test]
    fn validate_rejects_binding_scope_mismatch_mcp() {
        // Env binding 用于 streamable_http → 拒绝
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            env("GITHUB_TOKEN"),
        )];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::McpHttp).unwrap_err();
        assert!(err.contains("不匹配"), "unexpected: {err}");
        // Header binding 用于 stdio → 拒绝
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            header("authorization"),
        )];
        assert!(
            validate_requirements_scoped(&list, CredentialRequirementScope::McpStdio).is_err()
        );
    }

    #[test]
    fn validate_http_tool_scope_allows_header_and_query_only() {
        let base = |binding| {
            vec![req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                binding,
            )]
        };
        let http = CredentialRequirementScope::HttpTool;
        assert!(validate_requirements_scoped(&base(header("authorization")), http).is_ok());
        assert!(validate_requirements_scoped(&base(query("api_key")), http).is_ok());
        let err = validate_requirements_scoped(&base(env("GITHUB_TOKEN")), http).unwrap_err();
        assert!(err.contains("仅支持请求头或查询参数"), "unexpected: {err}");
        assert!(validate_requirements_scoped(
            &base(CredentialBinding::Internal {
                field: "token".to_string()
            }),
            http
        )
        .is_err());
    }

    #[test]
    fn validate_rejects_empty_binding_name() {
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            header("   "),
        )];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).unwrap_err();
        assert!(err.contains("注入点名"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_platform_kind_mismatch() {
        // generic 类缺 platform → 拒绝
        let list = vec![req(
            CredentialKind::GenericToken,
            None,
            None,
            None,
            header("X-TOKEN"),
        )];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).unwrap_err();
        assert!(err.contains("必须填写平台标识"), "unexpected: {err}");
        // 专用类带 platform → 拒绝
        let list = vec![req(
            CredentialKind::GithubToken,
            Some("github"),
            None,
            None,
            header("X-TOKEN"),
        )];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).unwrap_err();
        assert!(err.contains("不适用平台标识"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_field_enhancer_conflict() {
        let list = vec![req(
            CredentialKind::GenericToken,
            Some("linear"),
            Some("token"),
            Some(CredentialEnhancerKind::BearerToken),
            header("X-LINEAR"),
        )];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).unwrap_err();
        assert!(err.contains("互斥"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_unsupported_enhancer() {
        // github_token 无任何受支持增强器
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            Some(CredentialEnhancerKind::BearerToken),
            header("authorization"),
        )];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).unwrap_err();
        assert!(err.contains("不支持增强器"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_duplicate_triple() {
        // 同 (kind, platform, 注入名) 三元组 → 拒绝（field 不同也算重复）
        let list = vec![
            req(
                CredentialKind::GenericToken,
                Some("linear"),
                Some("token"),
                None,
                header("X-LINEAR"),
            ),
            req(
                CredentialKind::GenericToken,
                Some("linear"),
                None,
                Some(CredentialEnhancerKind::BearerToken),
                header("X-LINEAR"),
            ),
        ];
        let err =
            validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).unwrap_err();
        assert!(err.contains("重复"), "unexpected: {err}");
    }

    #[test]
    fn validate_allows_same_kind_different_binding_name() {
        let list = vec![
            req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                header("X-GITHUB"),
            ),
            req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                header("X-GITHUB-ENTERPRISE"),
            ),
        ];
        assert!(validate_requirements_scoped(&list, CredentialRequirementScope::HttpTool).is_ok());
    }

    #[test]
    fn validate_empty_list_passes() {
        for scope in [
            CredentialRequirementScope::McpStdio,
            CredentialRequirementScope::McpHttp,
            CredentialRequirementScope::HttpTool,
        ] {
            assert!(validate_requirements_scoped(&[], scope).is_ok());
        }
    }

    // ===== normalize_requirements =====

    #[test]
    fn normalize_trims_and_drops_empty_options() {
        let list = vec![req(
            CredentialKind::GenericToken,
            Some("  linear  "),
            Some("  "),
            None,
            header("  X-LINEAR  "),
        )];
        let normalized = normalize_requirements(list);
        assert_eq!(normalized.len(), 1);
        let r = &normalized[0];
        assert_eq!(r.platform.as_deref(), Some("linear"));
        assert_eq!(r.field, None, "空白 field 归 None");
        assert_eq!(binding_name(&r.binding), "X-LINEAR");
    }

    #[test]
    fn normalize_preserves_binding_variant() {
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            query("  api_key "),
        )];
        let r = &normalize_requirements(list)[0];
        assert!(matches!(&r.binding, CredentialBinding::Query { name } if name == "api_key"));
    }

    // ===== 增强器选项矩阵（D11：默认增强器不暴露） =====

    #[test]
    fn available_enhancers_follow_supports_matrix_excluding_defaults() {
        use CredentialEnhancerKind as E;
        // 专用 kind：零可选项
        for kind in [
            CredentialKind::LarkApp,
            CredentialKind::GithubToken,
            CredentialKind::TavilyKey,
        ] {
            assert!(available_enhancers(kind).is_empty(), "{kind:?}");
            assert!(!has_any_enhancer_support(kind), "{kind:?}");
        }
        // generic_token：仅 bearer_token（无默认增强器）
        assert_eq!(
            available_enhancers(CredentialKind::GenericToken),
            vec![E::BearerToken]
        );
        // oauth：bearer_token 可选，access_token 为默认项不暴露
        assert_eq!(
            available_enhancers(CredentialKind::OAuth),
            vec![E::BearerToken]
        );
        assert!(has_any_enhancer_support(CredentialKind::OAuth));
        // user_password：basic_auth 为默认项不暴露 → 空列表但存在支持
        assert!(available_enhancers(CredentialKind::UserPassword).is_empty());
        assert!(has_any_enhancer_support(CredentialKind::UserPassword));
    }

    // ===== 值解析辅助 =====

    #[test]
    fn kind_and_enhancer_value_roundtrip() {
        for kind in all_credential_kinds() {
            assert_eq!(kind_from_value(kind.as_str()), Some(kind));
        }
        assert_eq!(kind_from_value("unknown"), None);
        for e in all_enhancers() {
            assert_eq!(enhancer_from_value(enhancer_to_value(e)), Some(e));
        }
        assert_eq!(enhancer_from_value("none"), None);
    }

    #[test]
    fn mcp_transport_scope_mapping() {
        assert_eq!(
            mcp_transport_scope(McpTransport::Stdio),
            CredentialRequirementScope::McpStdio
        );
        assert_eq!(
            mcp_transport_scope(McpTransport::StreamableHttp),
            CredentialRequirementScope::McpHttp
        );
    }

    // ===== 敏感名判定（与后端 is_sensitive_header 同源） =====

    #[test]
    fn is_sensitive_name_matches_backend_rules() {
        // 精确匹配（忽略大小写）
        for name in ["authorization", "Authorization", "AUTHORIZATION", "cookie", "set-cookie"] {
            assert!(is_sensitive_name(name), "should match: {name}");
        }
        // 子串匹配
        for name in [
            "X-API-KEY",
            "X-Api-Key",
            "My-Token",
            "github_token",
            "client_secret",
            "user_password",
        ] {
            assert!(is_sensitive_name(name), "should match: {name}");
        }
        // 非敏感名
        for name in ["Content-Type", "Accept", "X-Api", "city", "page_size"] {
            assert!(!is_sensitive_name(name), "should not match: {name}");
        }
    }
}
