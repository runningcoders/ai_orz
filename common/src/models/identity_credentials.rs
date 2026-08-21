//! 用户身份凭证类型契约（前后端共享）
//!
//! 凭证存储于独立表 `user_credentials`（一凭证一行）：
//! - `CredentialKind` / `CredentialVisibility` 为 TEXT 字符串枚举
//!   （DB 值 = API 值 = snake_case 字符串，映射只此一处）
//! - `CredentialDetail` 按 kind 区分字段集，secret 类字段在落库前经
//!   `pkg::crypto::encrypt_channel_secret` 加密（加密发生在 Domain 编排层）

use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

use crate::error::{Result, bail_err};

/// 凭证类型（可扩展，如后续 WechatApp）
///
/// 分类型枚举（映射外部系统）：TEXT 存储（`#[sqlx(rename_all = "snake_case")]`），
/// DB 值 = 'lark_app' / 'github_token' / 'tavily_key'——与 API/JSON 值空间一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", sqlx(rename_all = "snake_case"))]
pub enum CredentialKind {
    /// 飞书自建应用凭证
    LarkApp,
    /// GitHub 访问令牌（PAT / OAuth token）
    GithubToken,
    /// Tavily 搜索 API key（个人 key，tavily_search 工具身份）
    TavilyKey,
    /// 通用平台令牌（Notion/Linear PAT 等；platform 必填，匹配键含 platform 维度）
    GenericToken,
    /// OAuth 刷新凭据（platform 必填；refresh_token 换 access_token 由增强器执行）
    #[serde(rename = "oauth")]
    #[cfg_attr(feature = "sqlx", sqlx(rename = "oauth"))]
    OAuth,
    /// 用户名密码对（platform 必填；Basic 串由默认增强器组装）
    UserPassword,
}

impl CredentialKind {
    /// generic 类 kind：匹配键含 platform 维度（(kind, platform) 二元组）
    pub fn requires_platform(&self) -> bool {
        matches!(
            self,
            Self::GenericToken | Self::OAuth | Self::UserPassword
        )
    }

    /// 稳定字符串名（snake_case，与 serde/DB 值空间一致；引导文案与展示用）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LarkApp => "lark_app",
            Self::GithubToken => "github_token",
            Self::TavilyKey => "tavily_key",
            Self::GenericToken => "generic_token",
            Self::OAuth => "oauth",
            Self::UserPassword => "user_password",
        }
    }
}

/// 凭证可见性（访问语义枚举）：TEXT 存储
///
/// private = 仅所有者用户及其下游引用；public = 同 org 用户可显式引用。
/// 可见性同时派生默认标记的作用域（private=个人默认 / public=组织默认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", sqlx(rename_all = "snake_case"))]
pub enum CredentialVisibility {
    /// 'private'：仅所有者用户
    #[default]
    Private,
    /// 'public'：同 org 用户可显式引用
    Public,
}

/// 凭证脱敏快照（跨层展示值对象：集成状态聚合 / 引用名称渲染）
///
/// secret 恒不出现（detail 整体不外泄）；`is_default` 为所在作用域的默认标记
/// （private=个人默认 / public=组织默认，作用域由 visibility 派生）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialSnapshot {
    /// 凭证 ID（使用方引用键）
    pub credential_id: String,
    /// 用户自定义名称（仅展示，不参与解析）
    pub name: String,
    /// 凭证类型
    pub kind: CredentialKind,
    /// 可见性：private=仅所有者 / public=同 org 可显式引用
    pub visibility: CredentialVisibility,
    /// 所在作用域默认标记（作用域由 visibility 派生）
    pub is_default: bool,
    /// 凭证归属用户 ID（资产所有者）
    pub user_id: String,
    /// 外部主标识（lark app_id；无概念的类型为 None）
    pub primary_id: Option<String>,
}

/// 凭证详情（serde 内部 tag，按类型区分字段集）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialDetail {
    /// 飞书自建应用：app_id + app_secret（落库前加密）+ 可选事件回调配置
    LarkApp {
        /// 应用 ID
        app_id: String,
        /// 应用 Secret（落库前经 encrypt_channel_secret 加密）
        app_secret: String,
        /// Encrypt Key（可选，同样加密存储）
        encrypt_key: Option<String>,
        /// Verification Token（可选，事件回调校验用）
        verification_token: Option<String>,
    },
    /// GitHub 访问令牌（落库前经 encrypt_channel_secret 加密）
    GithubToken {
        /// 访问令牌（PAT / OAuth token，落库前加密）
        token: String,
    },
    /// Tavily 搜索 API key（落库前经 encrypt_channel_secret 加密）
    TavilyKey {
        /// API key（落库前加密）
        api_key: String,
    },
    /// 通用平台令牌（Notion/Linear PAT 等；platform 必填，token 落库前加密）
    GenericToken {
        /// 平台令牌（落库前经 encrypt_channel_secret 加密）
        token: String,
    },
    /// OAuth 刷新凭据（platform 必填；client_secret / refresh_token 落库前加密）
    #[serde(rename = "oauth")]
    OAuth {
        /// 刷新端点（https，刷新前过 SSRF 校验）
        token_endpoint: String,
        /// 客户端 ID
        client_id: String,
        /// 落库前加密
        client_secret: String,
        /// 落库前加密
        refresh_token: String,
        /// 授权范围（可选）
        scope: Option<String>,
    },
    /// 用户名密码对（platform 必填；password 落库前加密）
    UserPassword {
        /// 用户名
        username: String,
        /// 落库前加密
        password: String,
    },
}

/// 凭证详情补丁（Domain 更新命令组件，明文输入；非 API DTO，无需 serde）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CredentialDetailPatch {
    /// 不变更 detail
    #[default]
    Unchanged,
    /// 飞书应用字段补丁（None 保持不变；verification_token 的 Some("") 表示清除）
    LarkApp {
        /// 应用 ID（None/空白保持不变）
        app_id: Option<String>,
        /// 应用 Secret（None/空白保持不变；提供时以明文传入，内部加密写入）
        app_secret: Option<String>,
        /// Encrypt Key（None/空白保持不变；提供时以明文传入，内部加密写入）
        encrypt_key: Option<String>,
        /// Verification Token（None 保持不变；Some 空白清除、非空覆盖）
        verification_token: Option<String>,
    },
    /// GitHub token 补丁
    GithubToken {
        /// 访问令牌（None/空白保持不变；提供时以明文传入，内部加密写入）
        token: Option<String>,
    },
    /// Tavily API key 补丁
    TavilyKey {
        /// API key（None/空白保持不变；提供时以明文传入，内部加密写入）
        api_key: Option<String>,
    },
    /// 通用平台令牌补丁
    GenericToken {
        /// 平台令牌（None/空白保持不变；提供时以明文传入，内部加密写入）
        token: Option<String>,
    },
    /// OAuth 刷新凭据补丁（敏感字段提供时明文传入，内部加密写入）
    OAuth {
        /// 刷新端点（None/空白保持不变）
        token_endpoint: Option<String>,
        /// 客户端 ID（None/空白保持不变）
        client_id: Option<String>,
        /// client secret（None/空白保持不变）
        client_secret: Option<String>,
        /// refresh token（None/空白保持不变）
        refresh_token: Option<String>,
        /// None 保持不变；Some 空白清除、非空覆盖
        scope: Option<String>,
    },
    /// 用户名密码对补丁
    UserPassword {
        /// 用户名（None/空白保持不变）
        username: Option<String>,
        /// 密码（None/空白保持不变；提供时以明文传入，内部加密写入）
        password: Option<String>,
    },
}

/// detail 变更影响摘要（Domain 据此决定联动动作，无需感知字段细节）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CredentialUpdateImpact {
    /// 敏感字段轮换（app_secret/encrypt_key/token 任一实际写入）
    pub secret_changed: bool,
}

impl CredentialDetail {
    /// 凭证类型
    pub fn kind(&self) -> CredentialKind {
        match self {
            Self::LarkApp { .. } => CredentialKind::LarkApp,
            Self::GithubToken { .. } => CredentialKind::GithubToken,
            Self::TavilyKey { .. } => CredentialKind::TavilyKey,
            Self::GenericToken { .. } => CredentialKind::GenericToken,
            Self::OAuth { .. } => CredentialKind::OAuth,
            Self::UserPassword { .. } => CredentialKind::UserPassword,
        }
    }

    /// 外部主标识（lark app_id；无概念的类型返回 None，供渠道移交对比）
    pub fn primary_id(&self) -> Option<&str> {
        match self {
            Self::LarkApp { app_id, .. } => Some(app_id.as_str()),
            Self::GithubToken { .. }
            | Self::TavilyKey { .. }
            | Self::GenericToken { .. }
            | Self::OAuth { .. }
            | Self::UserPassword { .. } => None,
        }
    }

    /// 主密钥字段引用（增强器/规范值的兜底取值；调用前须已解密）
    pub fn primary_secret(&self) -> &str {
        match self {
            Self::LarkApp { app_secret, .. } => app_secret,
            Self::GithubToken { token } | Self::GenericToken { token } => token,
            Self::TavilyKey { api_key } => api_key,
            Self::OAuth { refresh_token, .. } => refresh_token,
            Self::UserPassword { password, .. } => password,
        }
    }

    /// 规范化明文字段（trim；空白的可选字段视为未提供）
    pub fn normalized(self) -> Self {
        match self {
            Self::LarkApp {
                app_id,
                app_secret,
                encrypt_key,
                verification_token,
            } => Self::LarkApp {
                app_id: app_id.trim().to_string(),
                app_secret: app_secret.trim().to_string(),
                encrypt_key: encrypt_key
                    .map(|v| v.trim().to_string())
                    .filter(|s| !s.is_empty()),
                verification_token: verification_token
                    .map(|v| v.trim().to_string())
                    .filter(|s| !s.is_empty()),
            },
            Self::GithubToken { token } => Self::GithubToken {
                token: token.trim().to_string(),
            },
            Self::TavilyKey { api_key } => Self::TavilyKey {
                api_key: api_key.trim().to_string(),
            },
            Self::GenericToken { token } => Self::GenericToken {
                token: token.trim().to_string(),
            },
            Self::OAuth {
                token_endpoint,
                client_id,
                client_secret,
                refresh_token,
                scope,
            } => Self::OAuth {
                token_endpoint: token_endpoint.trim().to_string(),
                client_id: client_id.trim().to_string(),
                client_secret: client_secret.trim().to_string(),
                refresh_token: refresh_token.trim().to_string(),
                scope: scope
                    .map(|v| v.trim().to_string())
                    .filter(|s| !s.is_empty()),
            },
            Self::UserPassword { username, password } => Self::UserPassword {
                username: username.trim().to_string(),
                password: password.trim().to_string(),
            },
        }
    }

    /// 类型必填校验（规范化后调用）
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::LarkApp {
                app_id, app_secret, ..
            } => {
                if app_id.is_empty() || app_secret.is_empty() {
                    bail_err!(InvalidRequest, "飞书应用 App ID / App Secret 不能为空");
                }
            }
            Self::GithubToken { token } => {
                if token.is_empty() {
                    bail_err!(InvalidRequest, "GitHub Token 不能为空");
                }
            }
            Self::TavilyKey { api_key } => {
                if api_key.is_empty() {
                    bail_err!(InvalidRequest, "Tavily API Key 不能为空");
                }
            }
            Self::GenericToken { token } => {
                if token.is_empty() {
                    bail_err!(InvalidRequest, "平台令牌不能为空");
                }
            }
            Self::OAuth {
                token_endpoint,
                client_id,
                client_secret,
                refresh_token,
                ..
            } => {
                if token_endpoint.is_empty()
                    || client_id.is_empty()
                    || client_secret.is_empty()
                    || refresh_token.is_empty()
                {
                    bail_err!(InvalidRequest, "OAuth 凭据的端点/客户端/刷新令牌均不能为空");
                }
            }
            Self::UserPassword { username, password } => {
                if username.is_empty() || password.is_empty() {
                    bail_err!(InvalidRequest, "用户名与密码均不能为空");
                }
            }
        }
        Ok(())
    }

    /// 对敏感字段应用加密（哪些字段敏感由模型自身决定；加密原语以闭包注入，
    /// common 不依赖后端 crypto 实现）
    pub fn encrypt_sensitive<F>(self, encrypt: F) -> Result<Self>
    where
        F: Fn(&str) -> Result<String>,
    {
        match self {
            Self::LarkApp {
                app_id,
                app_secret,
                encrypt_key,
                verification_token,
            } => Ok(Self::LarkApp {
                app_id,
                app_secret: encrypt(&app_secret)?,
                encrypt_key: match encrypt_key {
                    Some(v) => Some(encrypt(&v)?),
                    None => None,
                },
                verification_token,
            }),
            Self::GithubToken { token } => Ok(Self::GithubToken {
                token: encrypt(&token)?,
            }),
            Self::TavilyKey { api_key } => Ok(Self::TavilyKey {
                api_key: encrypt(&api_key)?,
            }),
            Self::GenericToken { token } => Ok(Self::GenericToken {
                token: encrypt(&token)?,
            }),
            Self::OAuth {
                token_endpoint,
                client_id,
                client_secret,
                refresh_token,
                scope,
            } => Ok(Self::OAuth {
                token_endpoint,
                client_id,
                client_secret: encrypt(&client_secret)?,
                refresh_token: encrypt(&refresh_token)?,
                scope,
            }),
            Self::UserPassword { username, password } => Ok(Self::UserPassword {
                username,
                password: encrypt(&password)?,
            }),
        }
    }

    /// 应用明文补丁：新敏感字段在内部加密后写入，返回变更影响摘要
    ///
    /// - 补丁变体与凭证类型不匹配时报错（防御跨类型误用）
    /// - 可选字段 None / 空白保持原值不变；`verification_token` 的显式空串清除
    pub fn apply_patch<F>(
        &mut self,
        patch: CredentialDetailPatch,
        encrypt: F,
    ) -> Result<CredentialUpdateImpact>
    where
        F: Fn(&str) -> Result<String>,
    {
        let mut impact = CredentialUpdateImpact::default();
        match patch {
            CredentialDetailPatch::Unchanged => {}
            CredentialDetailPatch::LarkApp {
                app_id,
                app_secret,
                encrypt_key,
                verification_token,
            } => {
                let Self::LarkApp {
                    app_id: app_id_slot,
                    app_secret: secret_slot,
                    encrypt_key: encrypt_slot,
                    verification_token: token_slot,
                } = self
                else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用飞书凭证补丁"
                    );
                };
                if let Some(v) = app_id
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *app_id_slot = v;
                }
                if let Some(v) = app_secret
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *secret_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
                if let Some(v) = encrypt_key
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *encrypt_slot = Some(encrypt(&v)?);
                    impact.secret_changed = true;
                }
                if let Some(v) = verification_token {
                    // 空白视为清除，非空覆盖
                    *token_slot = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                }
            }
            CredentialDetailPatch::GithubToken { token } => {
                let Self::GithubToken { token: token_slot } = self else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用 GitHub 凭证补丁"
                    );
                };
                if let Some(v) = token
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *token_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
            }
            CredentialDetailPatch::TavilyKey { api_key } => {
                let Self::TavilyKey { api_key: key_slot } = self else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用 Tavily 凭证补丁"
                    );
                };
                if let Some(v) = api_key
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *key_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
            }
            CredentialDetailPatch::GenericToken { token } => {
                let Self::GenericToken { token: token_slot } = self else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用通用令牌凭证补丁"
                    );
                };
                if let Some(v) = token
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *token_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
            }
            CredentialDetailPatch::OAuth {
                token_endpoint,
                client_id,
                client_secret,
                refresh_token,
                scope,
            } => {
                let Self::OAuth {
                    token_endpoint: endpoint_slot,
                    client_id: client_id_slot,
                    client_secret: secret_slot,
                    refresh_token: refresh_slot,
                    scope: scope_slot,
                } = self
                else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用 OAuth 凭证补丁"
                    );
                };
                if let Some(v) = token_endpoint
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *endpoint_slot = v;
                }
                if let Some(v) = client_id
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *client_id_slot = v;
                }
                if let Some(v) = client_secret
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *secret_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
                if let Some(v) = refresh_token
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *refresh_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
                if let Some(v) = scope {
                    *scope_slot = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                }
            }
            CredentialDetailPatch::UserPassword { username, password } => {
                let Self::UserPassword {
                    username: username_slot,
                    password: password_slot,
                } = self
                else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用用户名密码凭证补丁"
                    );
                };
                if let Some(v) = username
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *username_slot = v;
                }
                if let Some(v) = password
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *password_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
            }
        }
        Ok(impact)
    }
}

// ==================== 共享工具凭据需求声明契约 ====================

use schemars::JsonSchema;

/// 共享工具的凭据需求声明（类型级声明，非实例级引用；全部字段非敏感）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CredentialRequirement {
    /// 需要的凭据类型
    pub kind: CredentialKind,
    /// 平台标识（generic 类 kind 必填，专用 kind 必空；匹配键二元组第二维）
    pub platform: Option<String>,
    /// 提取字段：None = 规范可用值；Some = detail 指定字段；与 enhancer 互斥
    pub field: Option<String>,
    /// 增强器类型：None = 规范可用值；显式选择默认增强器幂等等价于 None；与 field 互斥
    pub enhancer: Option<CredentialEnhancerKind>,
    /// 注入点（纯放置，零变换）
    pub binding: CredentialBinding,
}

/// 凭据增强器类型（配置声明与取值共用同一值域）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialEnhancerKind {
    /// "Bearer " + 规范可用值
    BearerToken,
    /// "Basic " + base64(username:password)
    BasicAuth,
    /// OAuth refresh → access_token（oauth 默认装配）
    AccessToken,
}

/// 注入点（纯放置，零变换）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialBinding {
    /// 注入子进程环境变量（stdio MCP）
    Env {
        /// 环境变量名
        name: String,
    },
    /// 注入 HTTP 请求头（http MCP / HTTP 工具）
    Header {
        /// 请求头名
        name: String,
    },
    /// 注入 URL 查询参数（HTTP 工具）
    Query {
        /// 查询参数名
        name: String,
    },
    /// 存工具实例字段（内置工具消费形态；field 为实例字段名）
    Internal {
        /// 工具实例字段名
        field: String,
    },
}

/// 需求声明的作用域（binding ↔ 协议匹配校验用；前端预校验与后端共用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialRequirementScope {
    /// stdio MCP Server：仅 Env binding
    McpStdio,
    /// streamable HTTP MCP Server：仅 Header binding
    McpHttp,
    /// HTTP 工具：Header / Query binding
    HttpTool,
    /// 内置工具（DB 不持久化，静态声明自校验用）：仅 Internal binding
    Builtin,
}

/// 增强器 ↔ 凭据类型可用性矩阵（单点；pkg supports 与前端下拉过滤共用）
pub fn enhancer_supports(kind: CredentialKind, enhancer: CredentialEnhancerKind) -> bool {
    matches!(
        (kind, enhancer),
        (CredentialKind::GenericToken, CredentialEnhancerKind::BearerToken)
            | (CredentialKind::OAuth, CredentialEnhancerKind::BearerToken)
            | (CredentialKind::OAuth, CredentialEnhancerKind::AccessToken)
            | (CredentialKind::UserPassword, CredentialEnhancerKind::BasicAuth)
    )
}

/// 复合形态凭据的默认增强器（oauth→AccessToken、user_password→BasicAuth）
pub fn default_enhancer(kind: CredentialKind) -> Option<CredentialEnhancerKind> {
    match kind {
        CredentialKind::OAuth => Some(CredentialEnhancerKind::AccessToken),
        CredentialKind::UserPassword => Some(CredentialEnhancerKind::BasicAuth),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_kind_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(CredentialKind::LarkApp).unwrap(),
            "lark_app"
        );
        assert_eq!(
            serde_json::to_value(CredentialKind::GithubToken).unwrap(),
            "github_token"
        );
        assert_eq!(
            serde_json::to_value(CredentialKind::TavilyKey).unwrap(),
            "tavily_key"
        );
        // 往返一致（DB TEXT 值 = serde 值）
        assert_eq!(
            serde_json::from_value::<CredentialKind>(serde_json::json!("lark_app")).unwrap(),
            CredentialKind::LarkApp
        );
    }

    #[test]
    fn test_credential_visibility_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(CredentialVisibility::Private).unwrap(),
            "private"
        );
        assert_eq!(
            serde_json::to_value(CredentialVisibility::Public).unwrap(),
            "public"
        );
        assert_eq!(
            CredentialVisibility::default(),
            CredentialVisibility::Private
        );
    }

    #[test]
    fn test_detail_serde_tag() {
        let detail = CredentialDetail::LarkApp {
            app_id: "cli_a1b2c3".to_string(),
            app_secret: "enc:v1:secret".to_string(),
            encrypt_key: Some("enc:v1:key".to_string()),
            verification_token: None,
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["type"], "lark_app");
        assert_eq!(json["app_id"], "cli_a1b2c3");
        // 往返一致
        let parsed: CredentialDetail = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, detail);
    }

    #[test]
    fn test_github_credential_serde_tag() {
        let detail = CredentialDetail::GithubToken {
            token: "enc:v1:gh-token".to_string(),
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["type"], "github_token");
        assert_eq!(json["token"], "enc:v1:gh-token");
        let parsed: CredentialDetail = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, detail);
    }

    #[test]
    fn test_tavily_detail_lifecycle() {
        // 规范化 + 校验
        let plain = CredentialDetail::TavilyKey {
            api_key: " tvly-xxx ".to_string(),
        }
        .normalized();
        assert!(matches!(&plain, CredentialDetail::TavilyKey { api_key } if api_key == "tvly-xxx"));
        assert!(plain.validate().is_ok());
        assert!(
            CredentialDetail::TavilyKey {
                api_key: String::new()
            }
            .validate()
            .is_err()
        );

        // 加密敏感字段
        let enc = CredentialDetail::TavilyKey {
            api_key: "tvly-plain".to_string(),
        }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
        assert!(
            matches!(&enc, CredentialDetail::TavilyKey { api_key } if api_key == "enc:tvly-plain")
        );

        // 补丁：空白不变、非空轮换、类型不匹配报错
        let mut detail = CredentialDetail::TavilyKey {
            api_key: "enc:v1:old".to_string(),
        };
        let impact = detail
            .apply_patch(
                CredentialDetailPatch::TavilyKey {
                    api_key: Some("  ".to_string()),
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert!(!impact.secret_changed);
        let impact = detail
            .apply_patch(
                CredentialDetailPatch::TavilyKey {
                    api_key: Some("tvly-new".to_string()),
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert!(impact.secret_changed);
        assert!(
            matches!(&detail, CredentialDetail::TavilyKey { api_key } if api_key == "enc:tvly-new")
        );
        assert!(
            detail
                .apply_patch(
                    CredentialDetailPatch::GithubToken {
                        token: Some("t".to_string()),
                    },
                    |s| Ok(s.to_string()),
                )
                .is_err()
        );
    }

    // ==================== CredentialDetail 行为 ====================

    #[test]
    fn test_detail_kind_and_primary_id() {
        let lark = CredentialDetail::LarkApp {
            app_id: "cli_a1b2c3".to_string(),
            app_secret: "s".to_string(),
            encrypt_key: None,
            verification_token: None,
        };
        assert_eq!(lark.kind(), CredentialKind::LarkApp);
        assert_eq!(lark.primary_id(), Some("cli_a1b2c3"));

        let gh = CredentialDetail::GithubToken {
            token: "t".to_string(),
        };
        assert_eq!(gh.kind(), CredentialKind::GithubToken);
        assert_eq!(gh.primary_id(), None);
    }

    #[test]
    fn test_detail_normalized_trims_and_drops_empty_optionals() {
        let plain = CredentialDetail::LarkApp {
            app_id: "  cli_x  ".to_string(),
            app_secret: " s1 ".to_string(),
            encrypt_key: Some("   ".to_string()),
            verification_token: Some(" vt ".to_string()),
        };
        let normalized = plain.normalized();
        let CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } = normalized
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_x");
        assert_eq!(app_secret, "s1");
        assert_eq!(encrypt_key, None, "空白可选字段视为未提供");
        assert_eq!(verification_token.as_deref(), Some("vt"));

        let gh = CredentialDetail::GithubToken {
            token: " ghp_x\n ".to_string(),
        }
        .normalized();
        assert!(matches!(gh, CredentialDetail::GithubToken { ref token } if token == "ghp_x"));
    }

    #[test]
    fn test_detail_validate_required_fields() {
        // 合法
        assert!(
            CredentialDetail::LarkApp {
                app_id: "cli_x".to_string(),
                app_secret: "s".to_string(),
                encrypt_key: None,
                verification_token: None,
            }
            .validate()
            .is_ok()
        );
        assert!(
            CredentialDetail::GithubToken {
                token: "t".to_string()
            }
            .validate()
            .is_ok()
        );
        // lark 缺 app_id
        let bad = CredentialDetail::LarkApp {
            app_id: " ".to_string(),
            app_secret: "s".to_string(),
            encrypt_key: None,
            verification_token: None,
        }
        .normalized();
        assert!(bad.validate().is_err());
        // github 缺 token
        let bad_gh = CredentialDetail::GithubToken {
            token: String::new(),
        };
        assert!(bad_gh.validate().is_err());
    }

    #[test]
    fn test_encrypt_sensitive_only_secret_fields() {
        let plain = CredentialDetail::LarkApp {
            app_id: "cli_x".to_string(),
            app_secret: "s1".to_string(),
            encrypt_key: Some("k1".to_string()),
            verification_token: Some("vt".to_string()),
        };
        let enc = plain
            .clone()
            .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
            .unwrap();
        let CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } = enc
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_x", "app_id 非敏感原样保留");
        assert_eq!(app_secret, "enc:s1");
        assert_eq!(encrypt_key.as_deref(), Some("enc:k1"));
        assert_eq!(
            verification_token.as_deref(),
            Some("vt"),
            "verification_token 非加密字段"
        );

        // encrypt_key=None 不调用加密器
        let no_key = CredentialDetail::LarkApp {
            app_id: "a".to_string(),
            app_secret: "s".to_string(),
            encrypt_key: None,
            verification_token: None,
        };
        assert!(no_key.encrypt_sensitive(|_| Err(bail_err_test())).is_err());

        // github token 加密
        let gh_enc = CredentialDetail::GithubToken {
            token: "ghp_x".to_string(),
        }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
        assert!(
            matches!(gh_enc, CredentialDetail::GithubToken { ref token } if token == "enc:ghp_x")
        );
    }

    /// 测试用错误构造（避免测试依赖具体 error 变体）
    fn bail_err_test() -> crate::error::Error {
        crate::error::Error::internal("test")
    }

    // ==================== 补丁应用 ====================

    fn plain_lark_detail() -> CredentialDetail {
        CredentialDetail::LarkApp {
            app_id: "cli_old".to_string(),
            app_secret: "enc:v1:old-secret".to_string(),
            encrypt_key: None,
            verification_token: Some("vt-old".to_string()),
        }
    }

    #[test]
    fn test_apply_patch_unchanged_is_noop() {
        let mut detail = plain_lark_detail();
        let before = detail.clone();
        let impact = detail
            .apply_patch(CredentialDetailPatch::Unchanged, |s| {
                Ok(format!("enc:{}", s))
            })
            .unwrap();
        assert_eq!(detail, before);
        assert!(!impact.secret_changed);
    }

    #[test]
    fn test_apply_patch_lark_fields() {
        let mut detail = plain_lark_detail();
        let impact = detail
            .apply_patch(
                CredentialDetailPatch::LarkApp {
                    app_id: Some("cli_new".to_string()),
                    app_secret: Some("new-secret".to_string()),
                    encrypt_key: Some("k1".to_string()),
                    verification_token: Some("  ".to_string()), // 空白清除
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        let CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } = &detail
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_new");
        assert_eq!(app_secret, "enc:new-secret");
        assert_eq!(encrypt_key.as_deref(), Some("enc:k1"));
        assert_eq!(*verification_token, None, "空白 verification_token 清除");
        assert!(impact.secret_changed, "secret/encrypt_key 任一变更即轮换");
    }

    #[test]
    fn test_apply_patch_empty_optional_keeps_value() {
        // 全 None（含空串）→ detail 不变、无 secret 轮换
        let mut detail = plain_lark_detail();
        let before = detail.clone();
        let impact = detail
            .apply_patch(
                CredentialDetailPatch::LarkApp {
                    app_id: None,
                    app_secret: Some("   ".to_string()),
                    encrypt_key: None,
                    verification_token: None,
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert_eq!(detail, before);
        assert!(!impact.secret_changed);
    }

    #[test]
    fn test_apply_patch_kind_mismatch_rejected() {
        // github 补丁打到 lark 凭证 → 报错
        let mut detail = plain_lark_detail();
        assert!(
            detail
                .apply_patch(
                    CredentialDetailPatch::GithubToken {
                        token: Some("t".to_string()),
                    },
                    |s| Ok(s.to_string()),
                )
                .is_err()
        );
        // lark 补丁打到 github 凭证 → 报错
        let mut gh = CredentialDetail::GithubToken {
            token: "enc:v1:t".to_string(),
        };
        assert!(
            gh.apply_patch(
                CredentialDetailPatch::LarkApp {
                    app_id: None,
                    app_secret: None,
                    encrypt_key: None,
                    verification_token: None,
                },
                |s| Ok(s.to_string()),
            )
            .is_err()
        );
    }

    #[test]
    fn test_apply_patch_github_token() {
        let mut gh = CredentialDetail::GithubToken {
            token: "enc:v1:old".to_string(),
        };
        let impact = gh
            .apply_patch(
                CredentialDetailPatch::GithubToken {
                    token: Some("ghp_new".to_string()),
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert!(matches!(&gh, CredentialDetail::GithubToken { token } if token == "enc:ghp_new"));
        assert!(impact.secret_changed);
    }

    // ==================== GenericToken / OAuth / UserPassword ====================

    #[test]
    fn test_new_kinds_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(CredentialKind::GenericToken).unwrap(),
            "generic_token"
        );
        assert_eq!(
            serde_json::to_value(CredentialKind::OAuth).unwrap(),
            "oauth"
        );
        assert_eq!(
            serde_json::to_value(CredentialKind::UserPassword).unwrap(),
            "user_password"
        );
    }

    #[test]
    fn test_generic_token_detail_lifecycle() {
        let plain = CredentialDetail::GenericToken {
            token: " ntn_xxx ".to_string(),
        }
        .normalized();
        assert!(matches!(&plain, CredentialDetail::GenericToken { token } if token == "ntn_xxx"));
        assert!(plain.validate().is_ok());
        let enc = CredentialDetail::GenericToken {
            token: "plain".to_string(),
        }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
        assert!(matches!(&enc, CredentialDetail::GenericToken { token } if token == "enc:plain"));
        let mut detail = CredentialDetail::GenericToken {
            token: "enc:v1:old".to_string(),
        };
        detail
            .apply_patch(
                CredentialDetailPatch::GenericToken {
                    token: Some("new".to_string()),
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert!(matches!(&detail, CredentialDetail::GenericToken { token } if token == "enc:new"));
    }

    #[test]
    fn test_oauth_detail_validate_and_secret() {
        let detail = CredentialDetail::OAuth {
            token_endpoint: "https://example.invalid/oauth/token".to_string(),
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
            refresh_token: "rt".to_string(),
            scope: Some("read".to_string()),
        };
        assert!(detail.clone().normalized().validate().is_ok());
        // 缺 token_endpoint → 校验失败
        let bad = CredentialDetail::OAuth {
            token_endpoint: String::new(),
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: "r".into(),
            scope: None,
        };
        assert!(bad.normalized().validate().is_err());
        assert_eq!(detail.primary_secret(), "rt");
        // client_secret / refresh_token 加密，client_id / token_endpoint 不加密
        let enc = detail
            .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
            .unwrap();
        let CredentialDetail::OAuth {
            client_id,
            client_secret,
            refresh_token,
            ..
        } = enc
        else {
            panic!("kind 不变");
        };
        assert_eq!(client_id, "cid");
        assert_eq!(client_secret, "enc:csec");
        assert_eq!(refresh_token, "enc:rt");
    }

    #[test]
    fn test_user_password_detail() {
        let detail = CredentialDetail::UserPassword {
            username: "alice".to_string(),
            password: " pw ".to_string(),
        };
        let normalized = detail.normalized();
        assert!(
            matches!(&normalized, CredentialDetail::UserPassword { username, password } if username == "alice" && password == "pw")
        );
        assert!(normalized.validate().is_ok());
        let enc = CredentialDetail::UserPassword {
            username: "alice".into(),
            password: "p".into(),
        }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
        // password 加密、username 不加密
        assert!(
            matches!(&enc, CredentialDetail::UserPassword { username, password } if username == "alice" && password == "enc:p")
        );
    }

    #[test]
    fn test_new_kinds_primary_id_none_and_requires_platform() {
        let kinds = [
            CredentialKind::GenericToken,
            CredentialKind::OAuth,
            CredentialKind::UserPassword,
        ];
        for kind in kinds {
            assert!(kind.requires_platform(), "generic 类 kind platform 必填");
        }
        for kind in [
            CredentialKind::LarkApp,
            CredentialKind::GithubToken,
            CredentialKind::TavilyKey,
        ] {
            assert!(!kind.requires_platform(), "专用 kind platform 必空");
        }
        // 三新变体 primary_id 均 None
        assert_eq!(
            CredentialDetail::GenericToken { token: "t".into() }.primary_id(),
            None
        );
        assert_eq!(
            CredentialDetail::UserPassword {
                username: "u".into(),
                password: "p".into()
            }
            .primary_id(),
            None
        );
    }

    // ==================== CredentialRequirement 契约 ====================

    #[test]
    fn test_requirement_serde_roundtrip() {
        let req = CredentialRequirement {
            kind: CredentialKind::OAuth,
            platform: Some("linear".to_string()),
            field: None,
            enhancer: Some(CredentialEnhancerKind::BearerToken),
            binding: CredentialBinding::Header {
                name: "authorization".to_string(),
            },
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["kind"], "oauth");
        assert_eq!(json["platform"], "linear");
        assert_eq!(json["enhancer"], "bearer_token");
        assert_eq!(json["binding"]["type"], "header");
        assert_eq!(json["binding"]["name"], "authorization");
        let parsed: CredentialRequirement = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_binding_serde_snake_case() {
        let env = CredentialBinding::Env {
            name: "API_TOKEN".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&env).unwrap()["type"],
            "env"
        );
        let query = CredentialBinding::Query {
            name: "api_key".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&query).unwrap()["type"],
            "query"
        );
        let internal = CredentialBinding::Internal {
            field: "app_id".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&internal).unwrap()["type"],
            "internal"
        );
    }

    #[test]
    fn test_enhancer_supports_matrix() {
        use CredentialEnhancerKind as E;
        // 精确表
        assert!(enhancer_supports(
            CredentialKind::GenericToken,
            E::BearerToken
        ));
        assert!(enhancer_supports(CredentialKind::OAuth, E::BearerToken));
        assert!(enhancer_supports(CredentialKind::OAuth, E::AccessToken));
        assert!(enhancer_supports(
            CredentialKind::UserPassword,
            E::BasicAuth
        ));
        // 专用 kind 零支持
        for kind in [
            CredentialKind::LarkApp,
            CredentialKind::GithubToken,
            CredentialKind::TavilyKey,
        ] {
            assert!(!enhancer_supports(kind, E::BearerToken));
            assert!(!enhancer_supports(kind, E::BasicAuth));
            assert!(!enhancer_supports(kind, E::AccessToken));
        }
        // 反向组合拒绝
        assert!(!enhancer_supports(
            CredentialKind::UserPassword,
            E::BearerToken
        ));
        assert!(!enhancer_supports(
            CredentialKind::GenericToken,
            E::AccessToken
        ));
        assert!(!enhancer_supports(
            CredentialKind::GenericToken,
            E::BasicAuth
        ));
    }

    #[test]
    fn test_default_enhancer_assembly() {
        use CredentialEnhancerKind as E;
        assert_eq!(
            default_enhancer(CredentialKind::OAuth),
            Some(E::AccessToken)
        );
        assert_eq!(
            default_enhancer(CredentialKind::UserPassword),
            Some(E::BasicAuth)
        );
        assert_eq!(default_enhancer(CredentialKind::GenericToken), None);
        assert_eq!(default_enhancer(CredentialKind::LarkApp), None);
    }
}
