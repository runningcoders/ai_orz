//! tests 单元测试（拆分自 mod.rs）
//!
//! 文件瘦身：原 806 行 → 268 行，测试体 539 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use common::models::{
    CredentialBinding, CredentialEnhancerKind, CredentialRequirement, CredentialRequirementScope,
};

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

fn header(name: &str) -> CredentialBinding {
    CredentialBinding::Header {
        name: name.to_string(),
    }
}

// ==================== validate_requirements ====================

/// Env binding 仅限 stdio MCP（HttpTool → Err）
#[test]
fn validate_rejects_env_binding_outside_stdio() {
    let r = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        CredentialBinding::Env {
            name: "LINEAR_TOKEN".to_string(),
        },
    );
    assert!(validate_requirements(&[r], CredentialRequirementScope::HttpTool).is_err());
}

/// Query binding 仅限 HTTP 工具（McpStdio / McpHttp → Err）
#[test]
fn validate_rejects_query_binding_for_mcp() {
    let r = req(
        CredentialKind::GenericToken,
        Some("notion"),
        None,
        None,
        CredentialBinding::Query {
            name: "api_key".to_string(),
        },
    );
    assert!(
        validate_requirements(
            std::slice::from_ref(&r),
            CredentialRequirementScope::McpStdio
        )
        .is_err()
    );
    assert!(validate_requirements(&[r], CredentialRequirementScope::McpHttp).is_err());
}

/// Internal binding 仅限内置工具（D25）
#[test]
fn validate_internal_binding_only_for_builtin() {
    let r = req(
        CredentialKind::LarkApp,
        None,
        None,
        None,
        CredentialBinding::Internal {
            field: "credential".to_string(),
        },
    );
    assert!(
        validate_requirements(
            std::slice::from_ref(&r),
            CredentialRequirementScope::Builtin
        )
        .is_ok()
    );
    assert!(
        validate_requirements(
            std::slice::from_ref(&r),
            CredentialRequirementScope::HttpTool
        )
        .is_err()
    );
    assert!(
        validate_requirements(
            std::slice::from_ref(&r),
            CredentialRequirementScope::McpStdio
        )
        .is_err()
    );
}

/// platform ↔ kind：generic 无 platform / 专用带 platform → Err（D3）
#[test]
fn validate_rejects_platform_mismatch() {
    let generic_no_platform = req(
        CredentialKind::GenericToken,
        None,
        None,
        None,
        header("X-Token"),
    );
    assert!(
        validate_requirements(&[generic_no_platform], CredentialRequirementScope::HttpTool)
            .is_err()
    );
    let dedicated_with_platform = req(
        CredentialKind::LarkApp,
        Some("lark"),
        None,
        None,
        header("X-Token"),
    );
    assert!(
        validate_requirements(
            &[dedicated_with_platform],
            CredentialRequirementScope::HttpTool
        )
        .is_err()
    );
}

/// field 与 enhancer 互斥（D8）
#[test]
fn validate_rejects_field_and_enhancer_both_set() {
    let r = req(
        CredentialKind::OAuth,
        Some("linear"),
        Some("client_id"),
        Some(CredentialEnhancerKind::AccessToken),
        header("Authorization"),
    );
    assert!(validate_requirements(&[r], CredentialRequirementScope::HttpTool).is_err());
}

/// enhancer ↔ kind 矩阵（D12）：专用 kind 零支持
#[test]
fn validate_rejects_enhancer_kind_mismatch() {
    let r = req(
        CredentialKind::GithubToken,
        None,
        None,
        Some(CredentialEnhancerKind::BearerToken),
        header("Authorization"),
    );
    assert!(validate_requirements(&[r], CredentialRequirementScope::HttpTool).is_err());
}

/// 显式选择默认增强器幂等允许（D11：oauth+access_token / user_password+basic_auth）
#[test]
fn validate_accepts_explicit_default_enhancer() {
    let oauth = req(
        CredentialKind::OAuth,
        Some("linear"),
        None,
        Some(CredentialEnhancerKind::AccessToken),
        header("Authorization"),
    );
    let up = req(
        CredentialKind::UserPassword,
        Some("jira"),
        None,
        Some(CredentialEnhancerKind::BasicAuth),
        header("Authorization"),
    );
    assert!(validate_requirements(&[oauth, up], CredentialRequirementScope::HttpTool).is_ok());
}

/// 同 (kind, platform, 注入名) 两条 → Err（D10）
#[test]
fn validate_rejects_duplicate_injection_point() {
    let a = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        header("X-Token"),
    );
    let b = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        header("X-Token"),
    );
    assert!(validate_requirements(&[a, b], CredentialRequirementScope::HttpTool).is_err());
}

/// 同凭据不同注入点 → Ok（access_token env + Bearer header 场景）
#[test]
fn validate_allows_same_credential_different_bindings() {
    let oauth_token = req(
        CredentialKind::OAuth,
        Some("linear"),
        None,
        Some(CredentialEnhancerKind::AccessToken),
        header("X-Linear-Token"),
    );
    let oauth_bearer = req(
        CredentialKind::OAuth,
        Some("linear"),
        None,
        Some(CredentialEnhancerKind::BearerToken),
        header("Authorization"),
    );
    assert!(
        validate_requirements(
            &[oauth_token, oauth_bearer],
            CredentialRequirementScope::HttpTool
        )
        .is_ok()
    );
}

// ==================== canonical / enhance ====================

fn resolved(
    detail: CredentialDetail,
    attributes: std::collections::BTreeMap<String, String>,
) -> ResolvedCredential {
    ResolvedCredential::new("cred-test".to_string(), detail, attributes)
}

/// oauth 规范值 = access_token（预填缓存命中分支，D11 默认装配）
#[tokio::test]
async fn canonical_oauth_returns_access_token() {
    oauth_token_manager().seed_for_test(
        "cred-oauth",
        "at_abc",
        std::time::Duration::from_secs(300),
    );
    let c = ResolvedCredential::new(
        "cred-oauth".to_string(),
        CredentialDetail::OAuth {
            token_endpoint: "https://example.invalid/token".to_string(),
            client_id: "cid".to_string(),
            client_secret: "cs".to_string(),
            refresh_token: "rt".to_string(),
            scope: None,
        },
        Default::default(),
    );
    assert_eq!(c.canonical_value(None).await.unwrap(), "at_abc");
}

/// user_password 规范值 = "Basic " + base64(username:password)
#[tokio::test]
async fn canonical_user_password_returns_basic_string() {
    use base64::Engine;
    let detail = CredentialDetail::UserPassword {
        username: "alice".to_string(),
        password: "pw".to_string(),
    };
    let c = resolved(detail, Default::default());
    let expected = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("alice:pw")
    );
    assert_eq!(c.canonical_value(None).await.unwrap(), expected);
}

/// generic_token 规范值 = 原始 token
#[tokio::test]
async fn canonical_generic_token_returns_raw_token() {
    let detail = CredentialDetail::GenericToken {
        token: "ntn_xxx".to_string(),
    };
    let c = resolved(detail, Default::default());
    assert_eq!(c.canonical_value(None).await.unwrap(), "ntn_xxx");
}

/// 字段提取：lark_app + field=app_id → app_id 值
#[tokio::test]
async fn canonical_field_extraction() {
    let detail = CredentialDetail::LarkApp {
        app_id: "cli_a".to_string(),
        app_secret: "sec".to_string(),
        encrypt_key: None,
        verification_token: None,
    };
    let c = resolved(detail, Default::default());
    assert_eq!(c.canonical_value(Some("app_id")).await.unwrap(), "cli_a");
}

/// 字段提取查找链：detail miss → attributes 命中（D24）
#[tokio::test]
async fn canonical_field_falls_back_to_attributes() {
    let detail = CredentialDetail::LarkApp {
        app_id: "cli_a".to_string(),
        app_secret: "sec".to_string(),
        encrypt_key: None,
        verification_token: None,
    };
    let mut attrs = std::collections::BTreeMap::new();
    attrs.insert("identity_mode".to_string(), "tenant".to_string());
    let c = resolved(detail, attrs);
    assert_eq!(
        c.canonical_value(Some("identity_mode")).await.unwrap(),
        "tenant"
    );
    // 双 miss → Err
    assert!(c.canonical_value(Some("nope")).await.is_err());
}

/// BearerToken 包裹 oauth 规范值（D10：Bearer + access_token）
#[tokio::test]
async fn bearer_wraps_canonical() {
    oauth_token_manager().seed_for_test("cred-b", "at_zz", std::time::Duration::from_secs(300));
    let c = ResolvedCredential::new(
        "cred-b".to_string(),
        CredentialDetail::OAuth {
            token_endpoint: "https://example.invalid/token".to_string(),
            client_id: "cid".to_string(),
            client_secret: "cs".to_string(),
            refresh_token: "rt".to_string(),
            scope: None,
        },
        Default::default(),
    );
    assert_eq!(
        c.enhance(CredentialEnhancerKind::BearerToken)
            .await
            .unwrap(),
        CredentialEnhancedValue::Value("Bearer at_zz".to_string())
    );
}

/// BearerToken 包裹 generic_token：Bearer + ntn_xxx
#[tokio::test]
async fn bearer_wraps_generic_token() {
    let c = resolved(
        CredentialDetail::GenericToken {
            token: "ntn_xxx".to_string(),
        },
        Default::default(),
    );
    assert_eq!(
        c.enhance(CredentialEnhancerKind::BearerToken)
            .await
            .unwrap(),
        CredentialEnhancedValue::Value("Bearer ntn_xxx".to_string())
    );
}

/// supports 不匹配 → Err（generic_token + BasicAuth）
#[tokio::test]
async fn enhance_rejects_unsupported_kind() {
    let c = resolved(
        CredentialDetail::GenericToken {
            token: "ntn_xxx".to_string(),
        },
        Default::default(),
    );
    assert!(c.enhance(CredentialEnhancerKind::BasicAuth).await.is_err());
}

// ==================== resolve_requirements ====================

fn fetched(detail: CredentialDetail) -> FetchedCredential {
    FetchedCredential {
        credential_id: "cred-r".to_string(),
        detail,
        attributes: Default::default(),
        already_decrypted: false,
    }
}

/// 按序配对取值：generic_token 原文 + user_password Basic 串
#[tokio::test]
async fn resolve_requirements_pairs_and_derives_values() {
    let requirements = vec![
        req(
            CredentialKind::GenericToken,
            Some("notion"),
            None,
            None,
            header("X-Notion"),
        ),
        req(
            CredentialKind::UserPassword,
            Some("jira"),
            None,
            None,
            header("Authorization"),
        ),
    ];
    let fetched = vec![
        fetched(CredentialDetail::GenericToken {
            token: "secret-tok".to_string(),
        }),
        fetched(CredentialDetail::UserPassword {
            username: "bob".to_string(),
            password: "pw2".to_string(),
        }),
    ];
    let resolved = resolve_requirements(&requirements, &fetched).await.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].value, "secret-tok");
    assert!(resolved[1].value.starts_with("Basic "));
}

/// lark dal 明文生产路径（already_decrypted=true）：detail 直通不经过解密
#[tokio::test]
async fn resolve_requirements_passes_plaintext_detail_through() {
    let requirements = vec![req(
        CredentialKind::LarkApp,
        None,
        Some("app_id"),
        None,
        CredentialBinding::Internal {
            field: "credential".to_string(),
        },
    )];
    let fetched = vec![FetchedCredential {
        credential_id: "cred-r".to_string(),
        detail: CredentialDetail::LarkApp {
            app_id: "cli_plain".to_string(),
            app_secret: "plain-sec".to_string(),
            encrypt_key: None,
            verification_token: None,
        },
        attributes: Default::default(),
        already_decrypted: true,
    }];
    let resolved = resolve_requirements(&requirements, &fetched).await.unwrap();
    assert_eq!(resolved[0].value, "cli_plain");
}

/// 长度不匹配 → 防御性错误
#[tokio::test]
async fn resolve_requirements_rejects_length_mismatch() {
    let requirements = vec![
        req(
            CredentialKind::GenericToken,
            Some("notion"),
            None,
            None,
            header("X-Notion"),
        ),
        req(
            CredentialKind::GenericToken,
            Some("linear"),
            None,
            None,
            header("X-Linear"),
        ),
    ];
    let fetched = vec![fetched(CredentialDetail::GenericToken {
        token: "t".to_string(),
    })];
    assert!(resolve_requirements(&requirements, &fetched).await.is_err());
}

// ==================== decrypt_detail（既有测试沿用） ====================

/// 明文兼容直通：无 enc:v1: 前缀的敏感字段原样返回（非敏感字段不经解密函数）
#[test]
fn decrypt_detail_passes_plaintext_through() {
    let detail = CredentialDetail::UserPassword {
        username: "alice".to_string(),
        password: "plain-pw".to_string(),
    };
    let decrypted = decrypt_detail(detail).unwrap();
    assert_eq!(
        decrypted,
        CredentialDetail::UserPassword {
            username: "alice".to_string(),
            password: "plain-pw".to_string(),
        }
    );
}

/// 加密态全字段对称：encrypt_sensitive → decrypt_detail 还原明文
///（config::init 落默认 secret_key，encrypt/decrypt 同钥可逆）
#[test]
fn decrypt_detail_roundtrips_encrypted_fields() {
    let _ = crate::config::init();
    let plain = CredentialDetail::OAuth {
        token_endpoint: "https://oauth.example.com/token".to_string(),
        client_id: "cid".to_string(),
        client_secret: "csec".to_string(),
        refresh_token: "rtok".to_string(),
        scope: Some("read".to_string()),
    };
    let encrypted = plain
        .clone()
        .encrypt_sensitive(crate::pkg::crypto::encrypt_channel_secret)
        .unwrap();
    // 落库态确为密文
    let CredentialDetail::OAuth {
        client_secret,
        refresh_token,
        ..
    } = &encrypted
    else {
        panic!("kind 不变");
    };
    assert!(client_secret.starts_with("enc:v1:"));
    assert!(refresh_token.starts_with("enc:v1:"));

    let decrypted = decrypt_detail(encrypted).unwrap();
    assert_eq!(decrypted, plain);
}

/// 非敏感字段（token_endpoint/client_id/scope/username）不经加密函数
#[test]
fn decrypt_detail_keeps_non_sensitive_fields_untouched() {
    let _ = crate::config::init();
    let detail = CredentialDetail::LarkApp {
        app_id: "cli_a".to_string(),
        app_secret: "sec".to_string(),
        encrypt_key: None,
        verification_token: Some("vt".to_string()),
    };
    // 仅 app_secret 被加密，app_id / verification_token 保持明文
    let encrypted = detail
        .encrypt_sensitive(crate::pkg::crypto::encrypt_channel_secret)
        .unwrap();
    let CredentialDetail::LarkApp {
        app_id,
        verification_token,
        ..
    } = &encrypted
    else {
        panic!("kind 不变");
    };
    assert_eq!(app_id, "cli_a");
    assert_eq!(verification_token.as_deref(), Some("vt"));
}
