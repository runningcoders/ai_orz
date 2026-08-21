//! 凭据域纯值加工模块（设计：docs/design/tool_credential_requirement_design.md）
//!
//! 纯值加工模块（D17）：解密 / 增强器 / canonical / OAuth 刷新 / 配置校验。
//! 零数据访问——凭据由 service 编排层（domain resolve_tool_credentials）
//! 从 user dal 取回后传入；本模块不持有 ctx、不定义数据端口、无注入注册。
//!
//! 不隶属 tool_registry（依赖方向 tool_registry → credential 单向）：
//! 凭据加工是凭据域通用能力，未来非工具消费方（渠道出站认证等）可直接引用。

use common::error::{Result, bail_err, err};
use common::models::{CredentialDetail, CredentialEnhancerKind, CredentialKind};

mod enhancer;
pub use enhancer::*;

// ==================== 凭据对象（代理增强） ====================

/// 解析后的凭据对象：detail 明文态 + 派生属性 + 默认增强器装配（D7/D24）
///
/// 生命周期仅当次调用栈（工具实例不复用，D22）；
/// 由 resolve_requirements 从 FetchedCredential 构造。
pub struct ResolvedCredential {
    credential_id: String,
    detail: CredentialDetail,
    attributes: std::collections::BTreeMap<String, String>,
}

impl ResolvedCredential {
    /// 构造（resolve_requirements 内部与测试用）
    pub(crate) fn new(
        credential_id: String,
        detail: CredentialDetail,
        attributes: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            credential_id,
            detail,
            attributes,
        }
    }

    /// 凭证 ID（OAuthTokenManager 缓存键等）
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    /// detail 明文态（供增强器取多字段上下文）
    pub fn detail(&self) -> &CredentialDetail {
        &self.detail
    }

    /// 获取指定增强器的结果（代理执行；supports 不匹配 → 错误）
    pub async fn enhance(&self, kind: CredentialEnhancerKind) -> Result<CredentialEnhancedValue> {
        let enhancer = enhancer_for(kind)?;
        if !enhancer.supports(self.detail.kind()) {
            bail_err!(InvalidRequest, "该凭据类型不支持所选增强器");
        }
        enhancer.enhance(self).await
    }

    /// 规范可用值（D6）：复合形态走默认增强器；单值 kind 查找链（D24）：
    /// detail 字段 → attributes 派生属性 → primary_secret
    pub async fn canonical_value(&self, field: Option<&str>) -> Result<String> {
        match (self.detail.kind(), field) {
            // 显式选择默认增强器幂等等价 None（D11）由取值侧归一，此处只按 kind 分派
            (CredentialKind::OAuth, None) => {
                Ok(match self.enhance(CredentialEnhancerKind::AccessToken).await? {
                    CredentialEnhancedValue::Value(v) => v,
                })
            }
            (CredentialKind::UserPassword, None) => {
                Ok(match self.enhance(CredentialEnhancerKind::BasicAuth).await? {
                    CredentialEnhancedValue::Value(v) => v,
                })
            }
            (_, Some(field_name)) => self.extract_field(field_name),
            _ => Ok(self.detail.primary_secret().to_string()),
        }
    }

    /// 字段提取（serde JSON 泛化取 detail 字段，miss 则查 attributes，D24 查找链）
    fn extract_field(&self, field: &str) -> Result<String> {
        let value = serde_json::to_value(&self.detail)
            .map_err(|_| err!(Internal, "凭据 detail 序列化失败"))?;
        if let Some(v) = value.get(field).and_then(|v| v.as_str()) {
            return Ok(v.to_string());
        }
        self.attributes
            .get(field)
            .cloned()
            .ok_or_else(|| err!(InvalidRequest, "凭据字段不存在: {}", field))
    }
}

// ==================== 解密单点 ====================

/// 按 kind 解密 detail 敏感字段（与 `CredentialDetail::encrypt_sensitive` 规则对称）
///
/// 入参为 DAL 取回的加密态 detail；解密结果仅存于当次调用栈。
/// `decrypt_channel_secret` 明文兼容：无 `enc:v1:` 前缀的值原样返回（测试直通）。
pub(crate) fn decrypt_detail(detail: CredentialDetail) -> Result<CredentialDetail> {
    let decrypt = crate::pkg::crypto::decrypt_channel_secret;
    Ok(match detail {
        CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } => CredentialDetail::LarkApp {
            app_id,
            app_secret: decrypt(app_secret.as_str())?,
            encrypt_key: match encrypt_key {
                Some(v) => Some(decrypt(v.as_str())?),
                None => None,
            },
            verification_token,
        },
        CredentialDetail::GithubToken { token } => CredentialDetail::GithubToken {
            token: decrypt(token.as_str())?,
        },
        CredentialDetail::TavilyKey { api_key } => CredentialDetail::TavilyKey {
            api_key: decrypt(api_key.as_str())?,
        },
        CredentialDetail::GenericToken { token } => CredentialDetail::GenericToken {
            token: decrypt(token.as_str())?,
        },
        CredentialDetail::OAuth {
            token_endpoint,
            client_id,
            client_secret,
            refresh_token,
            scope,
        } => CredentialDetail::OAuth {
            token_endpoint,
            client_id,
            client_secret: decrypt(client_secret.as_str())?,
            refresh_token: decrypt(refresh_token.as_str())?,
            scope,
        },
        CredentialDetail::UserPassword { username, password } => CredentialDetail::UserPassword {
            username,
            password: decrypt(password.as_str())?,
        },
    })
}

// ==================== 纯函数加工入口（D17：凭据由编排层传入） ====================

/// 单条 requirement 的最终注入值
pub struct ResolvedRequirement {
    /// 原始需求声明（注入点信息由消费方读取）
    pub requirement: CredentialRequirement,
    /// 注入值（增强器包裹 canonical 的结果，D10）
    pub value: String,
}

/// 编排层传入的凭据条目（dal 生产：credential_id + detail + 派生属性）
pub struct FetchedCredential {
    /// 凭证 ID（OAuthTokenManager 缓存键）
    pub credential_id: String,
    /// dal 生产路径凭据：lark dal 为明文（already_decrypted=true）；
    /// user dal / tavily 兜底为 DB 加密态（false）
    pub detail: CredentialDetail,
    /// lark dal 派生属性（D24：identity_mode 等；user dal 生产路径为空集）
    pub attributes: std::collections::BTreeMap<String, String>,
}

/// 纯函数加工：requirements 与编排层生产的凭据按序配对 → 逐条（按需）解密 + 取注入值。
/// 零数据访问——条目由 domain resolve_tool_credentials 传入；
/// 长度不匹配（编排层已保证逐条对应）→ 防御性错误。
pub async fn resolve_requirements(
    requirements: &[CredentialRequirement],
    fetched: &[FetchedCredential],
) -> Result<Vec<ResolvedRequirement>> {
    if requirements.len() != fetched.len() {
        bail_err!(Internal, "凭据需求与生产条目长度不匹配");
    }
    let mut resolved = Vec::with_capacity(requirements.len());
    for (requirement, fc) in requirements.iter().zip(fetched) {
        // lark dal 生产路径明文直取（already_decrypted=true）；
        // 其余路径 DB 加密态按 kind 解密（明文兼容：无前缀值原样返回）
        let detail = decrypt_detail(fc.detail.clone())?;
        let credential = ResolvedCredential::new(
            fc.credential_id.clone(),
            detail,
            fc.attributes.clone(),
        );
        let value = match requirement.enhancer {
            Some(kind) => match credential.enhance(kind).await? {
                CredentialEnhancedValue::Value(v) => v,
            },
            None => credential.canonical_value(requirement.field.as_deref()).await?,
        };
        resolved.push(ResolvedRequirement {
            requirement: requirement.clone(),
            value,
        });
    }
    Ok(resolved)
}

/// 缺凭据结构化引导（复用 tavily 引导形态，D19）
pub fn credential_missing_json(requirement: &CredentialRequirement) -> serde_json::Value {
    let kind_desc = match &requirement.platform {
        Some(platform) => format!("{}/{}", requirement.kind.as_str(), platform),
        None => requirement.kind.as_str().to_string(),
    };
    crate::pkg::tool_registry::tool_readiness::api_key_missing_json(
        &format!(
            "工具需要 {} 凭据，但当前用户与组织均无可解析凭据",
            kind_desc
        ),
        "绑定个人凭据（设置 → 身份凭证）并设为默认，或由管理员配置组织共享默认凭据",
    )
}

// ==================== 配置期校验（§2.1 校验清单单点） ====================

use common::models::{CredentialBinding, CredentialRequirement, CredentialRequirementScope};

/// binding ↔ 作用域匹配
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

/// 注入点名（binding 的 name / field）
fn binding_name(binding: &CredentialBinding) -> &str {
    match binding {
        CredentialBinding::Env { name }
        | CredentialBinding::Header { name }
        | CredentialBinding::Query { name } => name,
        CredentialBinding::Internal { field } => field,
    }
}

/// requirements 配置期校验（创建/更新 handler 与前端预校验同一套规则）
pub fn validate_requirements(
    requirements: &[CredentialRequirement],
    scope: CredentialRequirementScope,
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for req in requirements {
        // 1. binding ↔ 协议
        if !binding_allowed(&req.binding, scope) {
            bail_err!(InvalidRequest, "凭据注入点与工具协议不匹配");
        }
        // 注入名非空
        if binding_name(&req.binding).trim().is_empty() {
            bail_err!(InvalidRequest, "凭据注入点名不能为空");
        }
        // 2. platform ↔ kind（generic 类必填、专用必空，D3）
        if req.kind.requires_platform() != req.platform.is_some() {
            bail_err!(
                InvalidRequest,
                "generic 类凭据必须声明 platform、专用类凭据不得声明"
            );
        }
        // 3. field ↔ enhancer 互斥（D8）
        if req.field.is_some() && req.enhancer.is_some() {
            bail_err!(InvalidRequest, "凭据 field 与 enhancer 互斥");
        }
        // 4. enhancer ↔ kind supports 矩阵（D12；专用 kind 零支持）
        if let Some(enhancer) = req.enhancer
            && !common::models::enhancer_supports(req.kind, enhancer)
        {
            bail_err!(InvalidRequest, "该凭据类型不支持所选增强器");
        }
        // 5. (kind, platform, 注入点名) 三元组去重（D10）
        let key = (
            req.kind,
            req.platform.clone(),
            binding_name(&req.binding).to_string(),
        );
        if !seen.insert(key) {
            bail_err!(InvalidRequest, "同一注入点存在重复凭据需求");
        }
    }
    // 显式选择默认增强器幂等允许（D11），无校验分支
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::models::{CredentialBinding, CredentialEnhancerKind, CredentialRequirement, CredentialRequirementScope};

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
            validate_requirements(std::slice::from_ref(&r), CredentialRequirementScope::Builtin)
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
        assert!(
            validate_requirements(&[oauth, up], CredentialRequirementScope::HttpTool).is_ok()
        );
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
            validate_requirements(&[oauth_token, oauth_bearer], CredentialRequirementScope::HttpTool)
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
        oauth_token_manager()
            .seed_for_test("cred-oauth", "at_abc", std::time::Duration::from_secs(300));
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
            c.enhance(CredentialEnhancerKind::BearerToken).await.unwrap(),
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
            c.enhance(CredentialEnhancerKind::BearerToken).await.unwrap(),
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

    /// lark dal 明文生产路径：无密文前缀直通（decrypt 透传）
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
        let fetched = vec![fetched(CredentialDetail::LarkApp {
            app_id: "cli_plain".to_string(),
            app_secret: "plain-sec".to_string(),
            encrypt_key: None,
            verification_token: None,
        })];
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
            client_secret, refresh_token, ..
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
            app_id, verification_token, ..
        } = &encrypted
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_a");
        assert_eq!(verification_token.as_deref(), Some("vt"));
    }
}
