//! 邮件渠道 DAO 实现（SMTP 出站 + IMAP 入站轮询装配）
//!
//! 出站链路：DAL 层按渠道 `email_credential_id` 解析 EmailBot 凭证行
//! （kind 校验 + 解密授权码）得到 [`EmailSmtpCredentials`] 后传入，
//! 本模块只做纯 SMTP 出站（lettre）：
//! - 端口 465 → 隐式 TLS（`relay`，QQ 邮箱 / 163 预设端口）
//! - 其余端口 → STARTTLS（`starttls_relay`，587 为主流）
//!
//! 入站链路：同文件承载 [`EmailDaoImpl`]，持有 [`ImapPollRegistry`]
//! （凭证指纹幂等启停，轮询细节在 `imap.rs`），DAL / Domain 层经 trait
//! 方法启停各邮箱凭证的入站轮询。
//!
//! 每次推送按凭证参数即时构建 transport（用户自建代理邮箱，非长驻连接）；
//! 主题从正文派生（MessagePo 无标题字段，对齐飞书/微信全文推送语义）。

use super::EmailDao;
use super::imap::{EmailImapCredentials, ImapPollRegistry};
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::error::{Result, err};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message as LettreMessage, Tokio1Executor};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static EMAIL_DAO: OnceLock<Arc<dyn EmailDao>> = OnceLock::new();

/// 创建一个全新的邮件 DAO 实例（用于测试）
pub fn new() -> Arc<dyn EmailDao> {
    Arc::new(EmailDaoImpl::new())
}

/// 获取 EmailDao 单例
pub fn dao() -> Arc<dyn EmailDao> {
    EMAIL_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = EMAIL_DAO.set(new());
}

// ==================== 运行凭证 ====================

/// 邮件渠道运行凭证（由 DAL 层按渠道 `email_credential_id` 引用解析后传入，
/// DAO 不做凭证解析——与飞书 `LarkAppCredentials` / 微信 `IlinkChannelCredentials` 同构）
#[derive(Clone)]
pub struct EmailSmtpCredentials {
    /// 代理邮箱地址（发件人，如 bot@qq.com）
    pub email_address: String,
    /// SMTP 主机（如 smtp.qq.com）
    pub smtp_host: String,
    /// SMTP 端口（465 = 隐式 TLS，587 = STARTTLS）
    pub smtp_port: u16,
    /// 登录账号（多数提供商与邮箱地址相同）
    pub username: String,
    /// 登录密码 / 授权码（已解密）
    pub password: String,
}

impl std::fmt::Debug for EmailSmtpCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailSmtpCredentials")
            .field("email_address", &self.email_address)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

/// 从凭证行解析邮件渠道 SMTP 运行凭证（纯函数，可测）
///
/// 解析规则：凭证行（已由调用方按渠道 `email_credential_id` 查得）→
/// 校验 kind=EmailBot → 解密授权码 → SMTP 必备参数完整性校验；
/// 任一环节失败返回引导性错误。
pub fn resolve_email_credentials(
    credential: &crate::models::user_credential::UserCredentialPo,
    channel: &MessageChannel,
) -> Result<EmailSmtpCredentials> {
    let credential_id = credential.id.as_str();
    if credential.kind != common::models::CredentialKind::EmailBot {
        return Err(err!(
            InvalidRequest,
            "邮件渠道引用的凭证类型不匹配 channel_id={} credential_id={}",
            channel.po.id,
            credential_id
        ));
    }
    let common::models::CredentialDetail::EmailBot {
        email_address,
        smtp_host,
        smtp_port,
        username,
        password,
        ..
    } = &credential.detail.0
    else {
        return Err(err!(
            InvalidRequest,
            "邮件渠道引用的凭证类型不匹配 channel_id={} credential_id={}",
            channel.po.id,
            credential_id
        ));
    };
    let password = crate::pkg::crypto::decrypt_channel_secret(password).map_err(|e| {
        err!(
            Internal,
            "邮件凭证授权码解密失败 channel_id={} credential_id={}: {}",
            channel.po.id,
            credential_id,
            e
        )
    })?;
    if email_address.is_empty()
        || smtp_host.is_empty()
        || *smtp_port == 0
        || username.is_empty()
        || password.is_empty()
    {
        return Err(err!(
            InvalidRequest,
            "邮件凭证缺少 SMTP 必备参数（发件地址/主机/端口/账号/授权码）channel_id={} credential_id={}，请到身份凭证页补全",
            channel.po.id,
            credential_id
        ));
    }
    Ok(EmailSmtpCredentials {
        email_address: email_address.clone(),
        smtp_host: smtp_host.clone(),
        smtp_port: *smtp_port,
        username: username.clone(),
        password,
    })
}

// ==================== 内部辅助（纯函数，可测） ====================

/// 隐式 TLS 端口（QQ 邮箱 / 163 预设）
const IMPLICIT_TLS_PORT: u16 = 465;

/// 主题生成：取正文首个非空行，超长截断；全空白回落固定主题
///
/// MessagePo 无标题字段（对齐飞书/微信全文推送语义，正文即全部信息），
/// 但邮件必须有主题，从正文派生最贴近收件人阅读习惯。
fn compose_subject(content: &str) -> String {
    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    const MAX_SUBJECT_CHARS: usize = 64;
    let mut chars = first_line.chars();
    let mut subject: String = (&mut chars).take(MAX_SUBJECT_CHARS).collect();
    if chars.next().is_some() {
        subject.push('…');
    }
    if subject.is_empty() {
        subject = "AI Orz 消息推送".to_string();
    }
    subject
}

/// 对端收件地址提取与校验（纯函数，可测）
///
/// 渠道 `email_to_address` 缺失 / 空白返回引导性错误；格式合法性由
/// push 时的 Mailbox 解析兜底。
fn require_to_address(channel: &MessageChannel) -> Result<String> {
    channel
        .config()
        .email_to_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "邮件渠道缺少对端收件地址 email_to_address channel_id={}，请先在渠道配置中填写",
                channel.po.id
            )
        })
}

/// 构建出站 transport：465 隐式 TLS（relay），其余端口 STARTTLS（starttls_relay）
fn build_transport(
    credentials: &EmailSmtpCredentials,
) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let builder = if credentials.smtp_port == IMPLICIT_TLS_PORT {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&credentials.smtp_host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&credentials.smtp_host)
    }
    .map_err(|e| {
        err!(
            Internal,
            "邮件 SMTP 传输构建失败 host={} port={}: {}",
            credentials.smtp_host,
            credentials.smtp_port,
            e
        )
    })?;
    Ok(builder
        .port(credentials.smtp_port)
        .credentials(Credentials::new(
            credentials.username.clone(),
            credentials.password.clone(),
        ))
        .build())
}

// ==================== 实现 ====================

struct EmailDaoImpl {
    /// IMAP 入站轮询注册表（credential_id 键控，凭证指纹幂等启停）
    poll_loops: ImapPollRegistry,
}

impl EmailDaoImpl {
    fn new() -> Self {
        Self {
            poll_loops: ImapPollRegistry::new(),
        }
    }
}

#[async_trait::async_trait]
impl EmailDao for EmailDaoImpl {
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
        credentials: &EmailSmtpCredentials,
    ) -> Result<()> {
        let to_address = require_to_address(channel)?;
        let from: Mailbox = credentials.email_address.parse().map_err(|e| {
            err!(
                InvalidRequest,
                "邮件凭证发件地址不合法 address={}: {}",
                credentials.email_address,
                e
            )
        })?;
        let to: Mailbox = to_address.parse().map_err(|e| {
            err!(
                InvalidRequest,
                "对端收件地址不合法 address={}: {}",
                to_address,
                e
            )
        })?;
        let subject = compose_subject(&message.po.content);
        let email = LettreMessage::builder()
            .from(from)
            .to(to)
            .subject(&subject)
            .header(lettre::message::header::ContentType::TEXT_PLAIN)
            .body(message.po.content.clone())
            .map_err(|e| {
                err!(
                    Internal,
                    "邮件消息构建失败 message_id={}: {}",
                    message.po.id,
                    e
                )
            })?;

        let transport = build_transport(credentials)?;
        transport.send(email).await.map_err(|e| {
            err!(
                Internal,
                "邮件出站失败 message_id={} from={} to={} host={} port={}: {}",
                message.po.id,
                credentials.email_address,
                to_address,
                credentials.smtp_host,
                credentials.smtp_port,
                e
            )
        })?;
        log_info!(
            &ctx,
            "email_push",
            "邮件出站成功 message_id={} to={} subject={}",
            message.po.id,
            to_address,
            subject
        );
        Ok(())
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
        credentials: &EmailSmtpCredentials,
    ) -> Result<()> {
        // 邮件推送硬性要求对端地址（一期无入站自动回填），探活连带校验
        require_to_address(channel)?;
        // 与微信探活同款语义：无廉价无副作用的 SMTP 探针（lettre 不单独暴露
        // connect/auth/quit，真发信成本高且污染对端收件箱），阶段一做凭证
        // 完整性校验；真实连通性由首次推送运行态观察。
        if credentials.email_address.is_empty()
            || credentials.smtp_host.is_empty()
            || credentials.smtp_port == 0
            || credentials.username.is_empty()
            || credentials.password.is_empty()
        {
            return Err(err!(
                InvalidRequest,
                "邮件凭证缺少 SMTP 必备参数（发件地址/主机/端口/账号/授权码），请到身份凭证页补全"
            ));
        }
        log_info!(
            &ctx,
            "email_test_connection",
            "邮件凭证校验通过 email={} smtp={}:{}",
            credentials.email_address,
            credentials.smtp_host,
            credentials.smtp_port
        );
        Ok(())
    }

    async fn start_polling(&self, credentials: &EmailImapCredentials) -> Result<()> {
        self.poll_loops.ensure(credentials).await
    }

    async fn stop_polling(&self, credential_id: &str) -> Result<()> {
        // 幂等语义：未在运行的凭证停止时静默成功（对齐微信 stop_polling）
        let _ = self.poll_loops.stop(credential_id).await;
        Ok(())
    }

    async fn stop_all_polling(&self) -> Result<()> {
        self.poll_loops.stop_all().await;
        Ok(())
    }

    async fn is_polling(&self, credential_id: &str) -> bool {
        self.poll_loops.is_running(credential_id).await
    }

    /// 消费确认后推进 UID 游标（P2）—— 详见 `imap::CONFIRMED_UIDS` 的说明
    ///
    /// ⚠️ **必须幂等**：回调先于 `queue.ack`，崩溃/重投时会重复触发 ——
    /// `confirm_uid` 是 `max` 语义，同值重复写无副作用（§4.3-1）。
    async fn advance_inbound_cursor(
        &self,
        ctx: RequestContext,
        credential_id: &str,
        uid: u32,
    ) -> Result<()> {
        super::imap::confirm_uid(credential_id, uid);
        log_debug!(
            &ctx,
            "email_inbound",
            "inbound cursor advanced (consumption confirmed): credential_id={} uid={}",
            credential_id,
            uid
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message_channel::{ChannelConfig, MessageChannelPo};
    use common::models::CredentialDetail;

    fn channel(to_address: Option<&str>) -> MessageChannel {
        MessageChannel::from_po(MessageChannelPo::new(
            "ch_email_1".to_string(),
            "org_1".to_string(),
            "user_1".to_string(),
            None,
            common::enums::ChannelType::Email,
            "我的邮箱".to_string(),
            None,
            None,
            None,
            ChannelConfig {
                email_to_address: to_address.map(|s| s.to_string()),
                ..Default::default()
            },
            "user_1".to_string(),
        ))
    }

    fn credential_row(
        kind: common::models::CredentialKind,
        detail: CredentialDetail,
    ) -> crate::models::user_credential::UserCredentialPo {
        crate::models::user_credential::UserCredentialPo::new(
            "cred_1".to_string(),
            "org_1".to_string(),
            "user_1".to_string(),
            kind,
            "邮箱机器人".to_string(),
            detail,
            common::models::CredentialVisibility::Private,
            "user_1".to_string(),
        )
    }

    fn email_detail() -> CredentialDetail {
        // 明文直存：decrypt_channel_secret 对无 enc:v1: 前缀的值透传（不依赖 master_key 配置）
        CredentialDetail::EmailBot {
            email_address: "bot@qq.com".to_string(),
            smtp_host: "smtp.qq.com".to_string(),
            smtp_port: 465,
            imap_host: "imap.qq.com".to_string(),
            imap_port: 993,
            username: "bot@qq.com".to_string(),
            password: "authcode123".to_string(),
        }
    }

    /// 凭证解析：kind 校验 + 授权码解密透传 + SMTP 参数完整性
    #[test]
    fn test_resolve_email_credentials() {
        let ch = channel(Some("peer@example.com"));
        let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
        let resolved = resolve_email_credentials(&row, &ch).unwrap();
        assert_eq!(resolved.email_address, "bot@qq.com");
        assert_eq!(resolved.smtp_host, "smtp.qq.com");
        assert_eq!(resolved.smtp_port, 465);
        assert_eq!(resolved.username, "bot@qq.com");
        assert_eq!(resolved.password, "authcode123");

        // kind 不匹配：报错
        let row = credential_row(
            common::models::CredentialKind::GithubToken,
            CredentialDetail::GithubToken {
                token: "tok".to_string(),
            },
        );
        assert!(resolve_email_credentials(&row, &ch).is_err());

        // SMTP 必备参数缺失（端口 0）：报错
        let bad = CredentialDetail::EmailBot {
            email_address: "bot@qq.com".to_string(),
            smtp_host: "smtp.qq.com".to_string(),
            smtp_port: 0,
            imap_host: "imap.qq.com".to_string(),
            imap_port: 993,
            username: "bot@qq.com".to_string(),
            password: "authcode123".to_string(),
        };
        let row = credential_row(common::models::CredentialKind::EmailBot, bad);
        assert!(resolve_email_credentials(&row, &ch).is_err());
    }

    /// Debug 掩码：授权码不得以明文出现在日志形态中
    #[test]
    fn test_credentials_debug_masks_password() {
        let ch = channel(Some("peer@example.com"));
        let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
        let resolved = resolve_email_credentials(&row, &ch).unwrap();
        let debug = format!("{:?}", resolved);
        assert!(!debug.contains("authcode123"));
        assert!(debug.contains("***"));
    }

    /// 主题生成：取正文首个非空行 / 超长截断带省略号 / 全空白回落固定主题
    #[test]
    fn test_compose_subject() {
        assert_eq!(compose_subject("任务完成\n正文第二行"), "任务完成");
        assert_eq!(compose_subject("\n\n  缩进行  \n正文"), "缩进行");

        let long = "长".repeat(100);
        let subject = compose_subject(&long);
        assert_eq!(subject.chars().count(), 65); // 64 字符 + 省略号
        assert!(subject.ends_with('…'));

        assert_eq!(compose_subject(""), "AI Orz 消息推送");
        assert_eq!(compose_subject("  \n \n"), "AI Orz 消息推送");
    }

    /// 对端地址校验：缺失 / 空白报引导性错误，合法值透传
    #[test]
    fn test_require_to_address() {
        assert_eq!(
            require_to_address(&channel(Some("peer@example.com"))).unwrap(),
            "peer@example.com"
        );
        assert!(require_to_address(&channel(None)).is_err());
        assert!(require_to_address(&channel(Some("  "))).is_err());
    }

    /// transport 构建：465 走隐式 TLS / 587 走 STARTTLS（仅验证构建不报错，不发网络请求）
    #[test]
    fn test_build_transport() {
        let ch = channel(Some("peer@example.com"));
        let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
        let resolved = resolve_email_credentials(&row, &ch).unwrap();
        assert!(build_transport(&resolved).is_ok());

        let starttls = EmailSmtpCredentials {
            smtp_port: 587,
            ..resolved.clone()
        };
        assert!(build_transport(&starttls).is_ok());
    }

    /// P2：已确认游标 —— 单调不减 / `0` 是"未确认"哨兵（忽略）/ credential 隔离
    ///
    /// ⚠️ `CONFIRMED_UIDS` 是**进程级静态**：这里用专属 credential_id，
    /// 避免与其他用例串扰（这是选用静态的已知代价，见 `imap.rs` 的说明）。
    #[test]
    fn test_confirmed_uid_is_monotonic_and_scoped() {
        use super::super::imap::{confirm_uid, confirmed_uid};

        let cred = "cred_test_p2_cursor_a";
        assert_eq!(confirmed_uid(cred), 0, "未确认时起点为 0");

        confirm_uid(cred, 10);
        assert_eq!(confirmed_uid(cred), 10);

        // 单调不减：乱序回调（较小的 UID 后到）不得回退游标 —— 否则会重复拉取
        confirm_uid(cred, 3);
        assert_eq!(confirmed_uid(cred), 10);

        // 0 = "未确认"哨兵（IMAP UID 从 1 起）→ 忽略
        confirm_uid(cred, 0);
        assert_eq!(confirmed_uid(cred), 10);

        // credential 隔离：不同邮箱互不影响
        assert_eq!(confirmed_uid("cred_test_p2_cursor_b"), 0);
    }
}
