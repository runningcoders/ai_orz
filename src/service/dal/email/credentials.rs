//! 邮件凭据面 DAL 子 trait（EmailCredentialDal）
//!
//! 渠道定位查询 + IMAP 凭证解析（引用模式），消费方：
//! - finance domain 凭证删除联动（`find_channels_by_credential_id`）
//! - 邮件监听同步/重建（`resolve_imap_credentials_by_id`）
//!
//! 与微信的差异：SMTP 出站凭证解析已由 message_channel DAL 直连 DAO 完成
//! （`resolve_email_credentials`），本子 trait 只补 IMAP 入站面。

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::dao::email::EmailImapCredentials;
use common::error::Result;

/// 邮件凭据面 DAL 子 trait
#[async_trait::async_trait]
pub trait EmailCredentialDal: Send + Sync {
    /// 查找引用指定凭证的邮件渠道（供 Domain 凭证变更/删除联动编排）
    ///
    /// 内存过滤渠道 config_json 的 `email_credential_id`（渠道数量有限，可接受）；
    /// 已删除渠道（status=Deleted）不计入引用。
    async fn find_channels_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Vec<MessageChannel>>;

    /// 按凭证 ID 解析 IMAP 入站运行凭证（引用 ID → 凭证行，缺失/失败返回 None）
    ///
    /// 凭证查询复用调用方上下文的连接池（测试隔离友好）。
    async fn resolve_imap_credentials_by_id(
        &self,
        ctx: RequestContext,
        credential_id: &str,
    ) -> Option<EmailImapCredentials>;
}
