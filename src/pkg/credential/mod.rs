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
            (CredentialKind::OAuth, None) => Ok(
                match self.enhance(CredentialEnhancerKind::AccessToken).await? {
                    CredentialEnhancedValue::Value(v) => v,
                },
            ),
            (CredentialKind::UserPassword, None) => Ok(
                match self.enhance(CredentialEnhancerKind::BasicAuth).await? {
                    CredentialEnhancedValue::Value(v) => v,
                },
            ),
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
        CredentialDetail::WechatIlink {
            bot_token,
            bot_id,
            user_id,
            base_url,
        } => CredentialDetail::WechatIlink {
            bot_token: decrypt(bot_token.as_str())?,
            bot_id,
            user_id,
            base_url,
        },
        CredentialDetail::EmailBot {
            email_address,
            smtp_host,
            smtp_port,
            imap_host,
            imap_port,
            username,
            password,
        } => CredentialDetail::EmailBot {
            email_address,
            smtp_host,
            smtp_port,
            imap_host,
            imap_port,
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
    /// dal 生产路径取回的凭据形态
    pub detail: CredentialDetail,
    /// lark dal 派生属性（D24：identity_mode 等；user dal 生产路径为空集）
    pub attributes: std::collections::BTreeMap<String, String>,
    /// 明文标记：lark dal 为明文（true，resolve_credentials_for_user 内部已解密）；
    /// user dal 为 DB 加密态（false）；无其他生产路径（单字段 key 统一走 GenericToken，D27）
    pub already_decrypted: bool,
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
        let detail = if fc.already_decrypted {
            fc.detail.clone()
        } else {
            decrypt_detail(fc.detail.clone())?
        };
        let credential =
            ResolvedCredential::new(fc.credential_id.clone(), detail, fc.attributes.clone());
        let value = match requirement.enhancer {
            Some(kind) => match credential.enhance(kind).await? {
                CredentialEnhancedValue::Value(v) => v,
            },
            None => {
                credential
                    .canonical_value(requirement.field.as_deref())
                    .await?
            }
        };
        resolved.push(ResolvedRequirement {
            requirement: requirement.clone(),
            value,
        });
    }
    Ok(resolved)
}

/// 缺凭据结构化引导（GenericToken 平台化引导，D19）
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

use common::models::{CredentialRequirement, CredentialRequirementScope};

/// requirements 配置期校验（创建/更新 handler 与前端预校验同一套规则）
///
/// 六规则本体单点在 common `validate_requirements`（前后端共用，错误文案按 scope 细分），
/// 此处仅包装为 Error
pub fn validate_requirements(
    requirements: &[CredentialRequirement],
    scope: CredentialRequirementScope,
) -> Result<()> {
    common::models::validate_requirements(requirements, scope)
        .map_err(|msg| err!(InvalidRequest, "{}", msg))
}
#[cfg(test)]
#[path = "credential_tests.rs"]
mod tests;
