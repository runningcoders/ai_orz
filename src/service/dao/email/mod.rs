//! 邮件渠道 DAO 模块
//!
//! 出站：SMTP 推送（`smtp.rs`，lettre）；入站：IMAP 受管轮询（`imap.rs`，
//! 一个代理邮箱 = 一个轮询单元，收帧 publish AOP 事件 `EmailInboundEvent`）。
//! 对 SMTP/IMAP 协议的封装分别集中在两个文件（协议变更只影响对应文件）。
//!
//! 分层约束：DAO 不做凭证解析——出站与探活由 DAL 层按渠道
//! `email_credential_id` 引用解析出 [`smtp::EmailSmtpCredentials`] 后传入；
//! 入站轮询由 DAL 层按邮箱凭证引用解析出 [`imap::EmailImapCredentials`] 后传入
//! （对齐飞书 `LarkAppCredentials` / 微信 `IlinkChannelCredentials` 同构模式）。

use self::smtp::EmailSmtpCredentials;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

pub use self::imap::{EmailImapCredentials, resolve_imap_credentials};

/// 邮件渠道 DAO 接口
#[async_trait::async_trait]
pub trait EmailDao: Send + Sync {
    /// 推送消息到邮件（SMTP 出站：全文 `message.po.content` 发至渠道对端地址）
    ///
    /// # 参数
    /// - `ctx`: 请求上下文
    /// - `message`: 消息实体
    /// - `channel`: 消息渠道配置（`email_to_address` 为对端收件地址）
    /// - `credentials`: 已解析的 SMTP 运行凭证（授权码已解密）
    ///
    /// # 返回
    /// - `Ok(())`: 推送成功
    /// - `Err`: 推送失败（缺少对端地址 / 地址不合法 / 出站失败）
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
        credentials: &EmailSmtpCredentials,
    ) -> std::result::Result<(), common::error::Error>;

    /// 测试邮件渠道连接（凭证完整性 + 对端地址校验）
    ///
    /// 邮件推送硬性要求 `email_to_address`（一期无入站自动回填，
    /// 与微信 peer_id 首次入站回填不同），探活连带校验。
    async fn test_connection(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
        credentials: &EmailSmtpCredentials,
    ) -> std::result::Result<(), common::error::Error>;

    /// 确保指定邮箱凭证的 IMAP 入站轮询在运行（幂等；凭证指纹变化时停旧重建）
    ///
    /// 轮询单元 = 邮箱凭证（一个代理邮箱可能被 N 个渠道共用），
    /// 与微信按 channel 键控不同；路由匹配由消费侧 DAL 完成。
    async fn start_polling(
        &self,
        credentials: &EmailImapCredentials,
    ) -> std::result::Result<(), common::error::Error>;

    /// 停止指定邮箱凭证的 IMAP 入站轮询（未运行时幂等）
    async fn stop_polling(
        &self,
        credential_id: &str,
    ) -> std::result::Result<(), common::error::Error>;

    /// 停止全部 IMAP 入站轮询（优雅退出）
    async fn stop_all_polling(&self) -> std::result::Result<(), common::error::Error>;

    /// 指定邮箱凭证是否正在轮询
    async fn is_polling(&self, credential_id: &str) -> bool;
}

pub mod imap;
pub mod smtp;
pub use self::smtp::{dao, init, new};
