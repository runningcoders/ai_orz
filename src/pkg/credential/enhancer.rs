//! 凭据增强器：原始凭据衍生行为的封装（pkg 实现、自包含）
//!
//! 增强器只消费「解密后明文态凭据对象」（ResolvedCredential），
//! 多字段拼接上下文即凭据自身；可用性矩阵单点在
//! `common::models::enhancer_supports`（前端下拉过滤共用，D12）。

use crate::pkg::credential::ResolvedCredential;
use async_trait::async_trait;
use common::error::{Result, bail_err, err};
use common::models::{CredentialDetail, CredentialEnhancerKind, CredentialKind};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ==================== 凭据增强器 ====================

/// 增强结果（不同增强器返回形态不同，枚举拓展；v1 单值）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialEnhancedValue {
    /// 单值注入形态（header 值 / env 值等）
    Value(String),
}

/// 凭据增强器：原始凭据衍生行为的封装（pkg 实现、自包含）
#[async_trait]
pub trait CredentialEnhancer: Send + Sync {
    /// 增强器类型
    fn kind(&self) -> CredentialEnhancerKind;
    /// 可用性判定（矩阵单点 common::enhancer_supports，D12）
    fn supports(&self, kind: CredentialKind) -> bool {
        common::models::enhancer_supports(kind, self.kind())
    }
    /// 执行增强：入参凭据为解密后明文态（多字段拼接上下文即凭据自身）
    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue>;
}

/// "Bearer " + 规范可用值（包裹规则 D10）
pub struct BearerTokenEnhancer;

#[async_trait]
impl CredentialEnhancer for BearerTokenEnhancer {
    fn kind(&self) -> CredentialEnhancerKind {
        CredentialEnhancerKind::BearerToken
    }

    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue> {
        let canonical = credential.canonical_value(None).await?;
        Ok(CredentialEnhancedValue::Value(format!(
            "Bearer {}",
            canonical
        )))
    }
}

/// "Basic " + base64(username:password)
pub struct BasicAuthEnhancer;

#[async_trait]
impl CredentialEnhancer for BasicAuthEnhancer {
    fn kind(&self) -> CredentialEnhancerKind {
        CredentialEnhancerKind::BasicAuth
    }

    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue> {
        let CredentialDetail::UserPassword { username, password } = &credential.detail() else {
            bail_err!(InvalidRequest, "basic_auth 增强器仅支持 user_password 凭据");
        };
        use base64::Engine;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
        Ok(CredentialEnhancedValue::Value(format!("Basic {}", encoded)))
    }
}

/// OAuth refresh → access_token（§2.5.1 生命周期内聚）
pub struct AccessTokenEnhancer;

#[async_trait]
impl CredentialEnhancer for AccessTokenEnhancer {
    fn kind(&self) -> CredentialEnhancerKind {
        CredentialEnhancerKind::AccessToken
    }

    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue> {
        let CredentialDetail::OAuth { .. } = &credential.detail() else {
            bail_err!(InvalidRequest, "access_token 增强器仅支持 oauth 凭据");
        };
        let token = oauth_token_manager()
            .get_access_token(credential.credential_id(), credential.detail())
            .await?;
        Ok(CredentialEnhancedValue::Value(token))
    }
}

/// 增强器注册表（OnceLock；本模块内建三增强器，未来扩展在此注册）
pub(crate) fn enhancer_for(
    kind: CredentialEnhancerKind,
) -> Result<&'static dyn CredentialEnhancer> {
    static REGISTRY: OnceLock<Vec<&'static dyn CredentialEnhancer>> = OnceLock::new();
    let registry = REGISTRY.get_or_init(|| {
        vec![
            &BearerTokenEnhancer as &'static dyn CredentialEnhancer,
            &BasicAuthEnhancer,
            &AccessTokenEnhancer,
        ]
    });
    registry
        .iter()
        .find(|e| e.kind() == kind)
        .copied()
        .ok_or_else(|| err!(InvalidRequest, "未知凭据增强器类型"))
}

// ==================== OAuth refresh → access_token（TTL 缓存，D13） ====================

/// 缓存条目（提前 60s 过期写缓存，读取侧同样留 60s 安全余量）
struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// 缓存安全余量：命中要求剩余 > 60s，写入时提前 60s 过期
const TOKEN_CACHE_SAFETY_MARGIN: Duration = Duration::from_secs(60);
/// 刷新请求超时
const TOKEN_REFRESH_TIMEOUT: Duration = Duration::from_secs(30);

/// OAuthTokenManager：AccessToken 增强器内部引擎
///
/// 命中且剩余 > 60s 直返；miss/将过期则 SSRF 校验 → POST refresh → 缓存。
/// 刷新失败不缓存失败结果（D13）；错误信息不含任何 token 值。
pub struct OAuthTokenManager {
    cache: Mutex<HashMap<String, CachedToken>>,
}

pub(crate) fn oauth_token_manager() -> &'static OAuthTokenManager {
    static MANAGER: OnceLock<OAuthTokenManager> = OnceLock::new();
    MANAGER.get_or_init(|| OAuthTokenManager {
        cache: Mutex::new(HashMap::new()),
    })
}

impl OAuthTokenManager {
    /// 测试辅助：预填缓存（命中分支免网络）
    #[cfg(test)]
    pub(crate) fn seed_for_test(&self, credential_id: &str, token: &str, ttl: Duration) {
        self.cache.lock().unwrap().insert(
            credential_id.to_string(),
            CachedToken {
                token: token.to_string(),
                expires_at: Instant::now() + ttl,
            },
        );
    }

    /// 取 access_token：缓存命中直返，否则走 refresh_token 换取
    pub async fn get_access_token(
        &self,
        credential_id: &str,
        detail: &CredentialDetail,
    ) -> Result<String> {
        let CredentialDetail::OAuth {
            token_endpoint,
            client_id,
            client_secret,
            refresh_token,
            scope,
        } = detail
        else {
            bail_err!(InvalidRequest, "oauth token 刷新仅支持 oauth 凭据");
        };

        // 缓存命中且剩余 > 60s 直返（不发起任何网络请求）
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(credential_id)
                && cached.expires_at > Instant::now() + TOKEN_CACHE_SAFETY_MARGIN
            {
                return Ok(cached.token.clone());
            }
        }

        // SSRF 校验 + DNS pin（复用 http.rs 同款模式；拒绝内网/环路）
        let url = reqwest::Url::parse(token_endpoint)
            .map_err(|_| err!(InvalidRequest, "oauth token_endpoint 不是合法 URL"))?;
        let pinned =
            crate::pkg::tool_registry::tool_security::validate_target_url(None, None, None, &url)
                .await?;
        let host = url
            .host_str()
            .ok_or_else(|| err!(InvalidRequest, "oauth token_endpoint 缺少 host"))?
            .to_string();

        let mut form = vec![
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
        ];
        if let Some(scope) = scope {
            form.push(("scope", scope.as_str()));
        }

        let client = reqwest::Client::builder()
            .timeout(TOKEN_REFRESH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(&host, &pinned)
            .build()
            .map_err(|_| err!(Internal, "oauth refresh client 构建失败"))?;

        let response = client
            .post(url)
            .form(&form)
            .send()
            .await
            .map_err(|_| err!(Internal, "oauth token 刷新请求失败"))?;
        if !response.status().is_success() {
            // 刷新失败不缓存失败结果（D13）
            bail_err!(Internal, "oauth token 刷新返回非成功状态");
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|_| err!(Internal, "oauth token 刷新响应解析失败"))?;
        let token = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| err!(Internal, "oauth token 刷新响应缺少 access_token"))?;
        let expires_in = body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(0);

        // 提前 60s 过期写缓存
        let expires_at = Instant::now()
            + Duration::from_secs(expires_in).saturating_sub(TOKEN_CACHE_SAFETY_MARGIN);
        self.cache.lock().unwrap().insert(
            credential_id.to_string(),
            CachedToken {
                token: token.clone(),
                expires_at,
            },
        );
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth_detail(endpoint: &str) -> CredentialDetail {
        CredentialDetail::OAuth {
            token_endpoint: endpoint.to_string(),
            client_id: "cid".to_string(),
            client_secret: "cs".to_string(),
            refresh_token: "rt".to_string(),
            scope: None,
        }
    }

    /// 缓存命中：剩余 > 60s 直返，不发起网络请求（endpoint 不可达佐证）
    #[tokio::test]
    async fn token_manager_cache_hit_returns_without_refresh() {
        let mgr = OAuthTokenManager {
            cache: Mutex::new(HashMap::new()),
        };
        mgr.cache.lock().unwrap().insert(
            "c1".to_string(),
            CachedToken {
                token: "at_x".to_string(),
                expires_at: Instant::now() + Duration::from_secs(120),
            },
        );
        let detail = oauth_detail("https://example.invalid/token");
        assert_eq!(mgr.get_access_token("c1", &detail).await.unwrap(), "at_x");
    }

    /// 缓存过期/将过期 → 走刷新：不可达外网地址（DNS 失败）→ Err 且不覆盖缓存
    #[tokio::test]
    async fn token_manager_expired_cache_triggers_refresh() {
        let mgr = OAuthTokenManager {
            cache: Mutex::new(HashMap::new()),
        };
        // 剩余 < 60s 安全余量 → 视为将过期
        mgr.cache.lock().unwrap().insert(
            "c2".to_string(),
            CachedToken {
                token: "stale".to_string(),
                expires_at: Instant::now() + Duration::from_secs(10),
            },
        );
        let detail = oauth_detail("https://example.invalid/token");
        assert!(mgr.get_access_token("c2", &detail).await.is_err());
        // 失败不缓存：旧条目仍在（值未被替换）
        assert_eq!(mgr.cache.lock().unwrap().get("c2").unwrap().token, "stale");
    }

    /// SSRF 拒绝：内网 endpoint 直接 Err（不发起请求）
    #[tokio::test]
    async fn token_manager_rejects_local_network_endpoint() {
        let mgr = OAuthTokenManager {
            cache: Mutex::new(HashMap::new()),
        };
        let detail = oauth_detail("http://127.0.0.1:1/token");
        let err = mgr.get_access_token("c3", &detail).await.unwrap_err();
        assert!(
            err.to_string().contains("local network"),
            "expect local network rejection, got: {}",
            err
        );
    }

    /// 增强器 kind / supports 矩阵委托（单点在 common::enhancer_supports）
    #[test]
    fn enhancer_supports_matrix() {
        assert!(BearerTokenEnhancer.supports(CredentialKind::GenericToken));
        assert!(BearerTokenEnhancer.supports(CredentialKind::OAuth));
        assert!(!BearerTokenEnhancer.supports(CredentialKind::UserPassword));
        assert!(BasicAuthEnhancer.supports(CredentialKind::UserPassword));
        assert!(!BasicAuthEnhancer.supports(CredentialKind::OAuth));
        assert!(AccessTokenEnhancer.supports(CredentialKind::OAuth));
        // 专用 kind 零支持
        assert!(!BearerTokenEnhancer.supports(CredentialKind::GithubToken));
        // 注册表按 kind 取增强器
        let bearer = enhancer_for(CredentialEnhancerKind::BearerToken).unwrap();
        assert_eq!(bearer.kind(), CredentialEnhancerKind::BearerToken);
    }
}
