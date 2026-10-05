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
/// DB 值 = 'lark_app' / 'github_token' / 'generic_token'——与 API/JSON 值空间一致。
/// 单字段 API Key 类平台统一收敛到 `GenericToken` + `platform` 二元匹配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", sqlx(rename_all = "snake_case"))]
pub enum CredentialKind {
    /// 飞书自建应用凭证
    LarkApp,
    /// GitHub 访问令牌（PAT / OAuth token）
    GithubToken,
    /// 通用平台令牌（Notion/Linear PAT 等；platform 必填，匹配键含 platform 维度）
    GenericToken,
    /// OAuth 刷新凭据（platform 必填；refresh_token 换 access_token 由增强器执行）
    #[serde(rename = "oauth")]
    #[cfg_attr(feature = "sqlx", sqlx(rename = "oauth"))]
    OAuth,
    /// 用户名密码对（platform 必填；Basic 串由默认增强器组装）
    UserPassword,
    /// 微信 iLink（ClawBot）扫码凭据：confirmed 一次性产出，整组轮换
    WechatIlink,
    /// 邮箱机器人（用户自建代理邮箱）：platform = 邮箱提供商（qq/163…），
    /// 匹配键含 platform 维度（展示与默认隔离），连接参数以 detail 为准
    EmailBot,
}

impl CredentialKind {
    /// 匹配键含 platform 维度（(kind, platform) 二元组）：generic 类 kind
    /// （platform = 外部平台标识）与多提供商专用 kind（EmailBot 的 platform = 邮箱提供商）
    pub fn requires_platform(&self) -> bool {
        matches!(
            self,
            Self::GenericToken | Self::OAuth | Self::UserPassword | Self::EmailBot
        )
    }

    /// 稳定字符串名（snake_case，与 serde/DB 值空间一致；引导文案与展示用）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LarkApp => "lark_app",
            Self::GithubToken => "github_token",
            Self::GenericToken => "generic_token",
            Self::OAuth => "oauth",
            Self::UserPassword => "user_password",
            Self::WechatIlink => "wechat_ilink",
            Self::EmailBot => "email_bot",
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
    /// 微信 iLink 扫码凭据（confirmed 一次性产出，重新扫码 = 整组轮换）
    WechatIlink {
        /// bot 令牌（落库前经 encrypt_channel_secret 加密）
        bot_token: String,
        /// iLink bot 标识
        bot_id: String,
        /// bot 侧用户标识（部分登录响应未返回）
        user_id: Option<String>,
        /// 接入域（以登录响应为准，不硬编码）
        base_url: String,
    },
    /// 邮箱机器人凭据（用户自建代理邮箱；platform = 邮箱提供商如 qq/163，
    /// 仅作展示/默认隔离维度，SMTP/IMAP 连接参数以本 detail 为准）
    EmailBot {
        /// 代理邮箱地址（对外主标识，如 bot@example.com）
        email_address: String,
        /// SMTP 主机（如 smtp.qq.com）
        smtp_host: String,
        /// SMTP 端口（如 465）
        smtp_port: u16,
        /// IMAP 主机（如 imap.qq.com，二期入站使用）
        imap_host: String,
        /// IMAP 端口（如 993）
        imap_port: u16,
        /// 登录账号（多数提供商与邮箱地址相同）
        username: String,
        /// 登录密码 / 授权码（落库前经 encrypt_channel_secret 加密）
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
    /// 微信 iLink 扫码凭据补丁（重新扫码整组覆盖：提供即写入，None/空白保持）
    WechatIlink {
        /// bot 令牌（None/空白保持不变；提供时以明文传入，内部加密写入）
        bot_token: Option<String>,
        /// iLink bot 标识（None/空白保持不变）
        bot_id: Option<String>,
        /// bot 侧用户标识（None 保持不变；Some 空白清除、非空覆盖）
        user_id: Option<String>,
        /// 接入域（None/空白保持不变）
        base_url: Option<String>,
    },
    /// 邮箱机器人凭据补丁（None/空白保持不变；端口 None/0 保持不变）
    EmailBot {
        /// 代理邮箱地址（None/空白保持不变）
        email_address: Option<String>,
        /// SMTP 主机（None/空白保持不变）
        smtp_host: Option<String>,
        /// SMTP 端口（None/0 保持不变）
        smtp_port: Option<u16>,
        /// IMAP 主机（None/空白保持不变）
        imap_host: Option<String>,
        /// IMAP 端口（None/0 保持不变）
        imap_port: Option<u16>,
        /// 登录账号（None/空白保持不变）
        username: Option<String>,
        /// 登录密码 / 授权码（None/空白保持不变；提供时以明文传入，内部加密写入）
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
            Self::GenericToken { .. } => CredentialKind::GenericToken,
            Self::OAuth { .. } => CredentialKind::OAuth,
            Self::UserPassword { .. } => CredentialKind::UserPassword,
            Self::WechatIlink { .. } => CredentialKind::WechatIlink,
            Self::EmailBot { .. } => CredentialKind::EmailBot,
        }
    }

    /// 外部主标识（lark app_id / 邮箱机器人邮箱地址；无概念的类型返回 None，供渠道移交对比）
    pub fn primary_id(&self) -> Option<&str> {
        match self {
            Self::LarkApp { app_id, .. } => Some(app_id.as_str()),
            Self::WechatIlink { bot_id, .. } => Some(bot_id.as_str()),
            Self::EmailBot { email_address, .. } => Some(email_address.as_str()),
            Self::GithubToken { .. }
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
            Self::OAuth { refresh_token, .. } => refresh_token,
            Self::UserPassword { password, .. } => password,
            Self::WechatIlink { bot_token, .. } => bot_token,
            Self::EmailBot { password, .. } => password,
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
            Self::WechatIlink {
                bot_token,
                bot_id,
                user_id,
                base_url,
            } => Self::WechatIlink {
                bot_token: bot_token.trim().to_string(),
                bot_id: bot_id.trim().to_string(),
                user_id: user_id
                    .map(|v| v.trim().to_string())
                    .filter(|s| !s.is_empty()),
                base_url: base_url.trim().trim_end_matches('/').to_string(),
            },
            Self::EmailBot {
                email_address,
                smtp_host,
                smtp_port,
                imap_host,
                imap_port,
                username,
                password,
            } => Self::EmailBot {
                email_address: email_address.trim().to_string(),
                smtp_host: smtp_host.trim().to_string(),
                smtp_port,
                imap_host: imap_host.trim().to_string(),
                imap_port,
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
            Self::WechatIlink {
                bot_token,
                bot_id,
                base_url,
                ..
            } => {
                if bot_token.is_empty() || bot_id.is_empty() || base_url.is_empty() {
                    bail_err!(
                        InvalidRequest,
                        "微信 iLink 凭据的 bot_token / bot_id / base_url 均不能为空"
                    );
                }
                if !base_url.starts_with("https://") {
                    bail_err!(InvalidRequest, "微信 iLink 的 base_url 必须是 https 地址");
                }
            }
            Self::EmailBot {
                email_address,
                smtp_host,
                smtp_port,
                imap_host,
                imap_port,
                username,
                password,
            } => {
                if email_address.is_empty()
                    || smtp_host.is_empty()
                    || imap_host.is_empty()
                    || username.is_empty()
                    || password.is_empty()
                {
                    bail_err!(
                        InvalidRequest,
                        "邮箱机器人凭据的邮箱地址 / SMTP / IMAP / 账号 / 密码均不能为空"
                    );
                }
                if !email_address.contains('@') {
                    bail_err!(InvalidRequest, "邮箱机器人凭据的邮箱地址格式不正确");
                }
                if *smtp_port == 0 || *imap_port == 0 {
                    bail_err!(
                        InvalidRequest,
                        "邮箱机器人凭据的 SMTP / IMAP 端口必须大于 0"
                    );
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
            Self::WechatIlink {
                bot_token,
                bot_id,
                user_id,
                base_url,
            } => Ok(Self::WechatIlink {
                bot_token: encrypt(&bot_token)?,
                bot_id,
                user_id,
                base_url,
            }),
            Self::EmailBot {
                email_address,
                smtp_host,
                smtp_port,
                imap_host,
                imap_port,
                username,
                password,
            } => Ok(Self::EmailBot {
                email_address,
                smtp_host,
                smtp_port,
                imap_host,
                imap_port,
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
            CredentialDetailPatch::WechatIlink {
                bot_token,
                bot_id,
                user_id,
                base_url,
            } => {
                let Self::WechatIlink {
                    bot_token: token_slot,
                    bot_id: bot_id_slot,
                    user_id: user_id_slot,
                    base_url: base_url_slot,
                } = self
                else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用微信 iLink 凭证补丁"
                    );
                };
                // 重新扫码 = 整组轮换：提供即覆盖，None/空白保持
                if let Some(v) = bot_token
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *token_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
                if let Some(v) = bot_id
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *bot_id_slot = v;
                }
                if let Some(v) = user_id {
                    // 空白视为清除，非空覆盖
                    *user_id_slot = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                }
                if let Some(v) = base_url
                    .map(|s| s.trim().trim_end_matches('/').to_string())
                    .filter(|s| !s.is_empty())
                {
                    *base_url_slot = v;
                }
            }
            CredentialDetailPatch::EmailBot {
                email_address,
                smtp_host,
                smtp_port,
                imap_host,
                imap_port,
                username,
                password,
            } => {
                let Self::EmailBot {
                    email_address: email_slot,
                    smtp_host: smtp_host_slot,
                    smtp_port: smtp_port_slot,
                    imap_host: imap_host_slot,
                    imap_port: imap_port_slot,
                    username: username_slot,
                    password: password_slot,
                } = self
                else {
                    bail_err!(
                        InvalidRequest,
                        "补丁类型与凭证类型不匹配，无法应用邮箱机器人凭证补丁"
                    );
                };
                if let Some(v) = email_address
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *email_slot = v;
                }
                if let Some(v) = smtp_host
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *smtp_host_slot = v;
                }
                if let Some(v) = smtp_port.filter(|p| *p > 0) {
                    *smtp_port_slot = v;
                }
                if let Some(v) = imap_host
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *imap_host_slot = v;
                }
                if let Some(v) = imap_port.filter(|p| *p > 0) {
                    *imap_port_slot = v;
                }
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
        (
            CredentialKind::GenericToken,
            CredentialEnhancerKind::BearerToken
        ) | (CredentialKind::OAuth, CredentialEnhancerKind::BearerToken)
            | (CredentialKind::OAuth, CredentialEnhancerKind::AccessToken)
            | (
                CredentialKind::UserPassword,
                CredentialEnhancerKind::BasicAuth
            )
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

/// 敏感凭据注入名判定（单点；后端 tool_security 校验与前端表单预检共用，双端同源零漂移）
///
/// 规则：authorization / cookie / set-cookie 精确匹配，api-key / token / secret / password
/// 子串匹配（注意 api-key 为连字符形态，api_key 下划线不命中），忽略大小写。
/// 命中的 header/query/env 名只能经 credential_requirements 注入，禁止静态配置。
pub fn is_sensitive_credential_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized == "authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized.contains("api-key")
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
}

impl CredentialEnhancerKind {
    /// serde snake_case 值（与 DB 值域 / 前端下拉值一致）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BearerToken => "bearer_token",
            Self::BasicAuth => "basic_auth",
            Self::AccessToken => "access_token",
        }
    }
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

/// binding ↔ 作用域匹配矩阵（配置期校验单点）
pub fn binding_allowed(binding: &CredentialBinding, scope: CredentialRequirementScope) -> bool {
    matches!(
        (binding, scope),
        (
            CredentialBinding::Env { .. },
            CredentialRequirementScope::McpStdio
        ) | (
            CredentialBinding::Header { .. },
            CredentialRequirementScope::McpHttp
        ) | (
            CredentialBinding::Header { .. },
            CredentialRequirementScope::HttpTool
        ) | (
            CredentialBinding::Query { .. },
            CredentialRequirementScope::HttpTool
        ) | (
            CredentialBinding::Internal { .. },
            CredentialRequirementScope::Builtin
        )
    )
}

/// MCP 传输方式 → 凭据需求作用域（stdio → McpStdio / streamable_http → McpHttp）
pub fn mcp_transport_scope(transport: crate::enums::McpTransport) -> CredentialRequirementScope {
    match transport {
        crate::enums::McpTransport::Stdio => CredentialRequirementScope::McpStdio,
        crate::enums::McpTransport::StreamableHttp => CredentialRequirementScope::McpHttp,
    }
}

/// requirements 配置期校验（六规则单点；后端 handler 校验与前端表单预校验共用）
///
/// 规则：binding↔scope 矩阵 / 注入名非空 / platform↔kind（generic 类必填专用类必空）/
/// field↔enhancer 互斥 / enhancer↔kind supports 矩阵 / (kind, platform, 注入名) 三元组去重。
/// 返回具体错误文案（Err(String)），由各端自行包装为本地错误类型。
pub fn validate_requirements(
    requirements: &[CredentialRequirement],
    scope: CredentialRequirementScope,
) -> std::result::Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for req in requirements {
        // 1. binding ↔ scope
        if !binding_allowed(&req.binding, scope) {
            return Err(match scope {
                CredentialRequirementScope::McpStdio | CredentialRequirementScope::McpHttp => {
                    "凭据注入点与传输方式不匹配（Stdio 仅支持环境变量注入，StreamableHttp 仅支持请求头注入）".to_string()
                }
                CredentialRequirementScope::HttpTool => {
                    "凭据注入点与工具协议不匹配（HTTP 工具仅支持请求头或查询参数注入）".to_string()
                }
                CredentialRequirementScope::Builtin => {
                    "凭据注入点与工具协议不匹配（内置工具仅支持实例字段注入）".to_string()
                }
            });
        }
        // 2. 注入名非空
        if binding_name(&req.binding).trim().is_empty() {
            return Err("凭据注入点名不能为空".to_string());
        }
        // 3. platform ↔ kind
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
                enhancer.as_str()
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
    // 显式选择默认增强器幂等允许（D11），无校验分支
    Ok(())
}
#[cfg(test)]
#[path = "identity_credentials_tests.rs"]
mod tests;
