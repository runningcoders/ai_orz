//! OrganizationPairingCode DAO 模块
//!
//! 配对码的读写：签发插入 + 原子消费（单用途 + TTL 判定合一，评审稿 §4.1 / §6.3）。

use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::pkg::RequestContext;
use common::error::Result;

/// OrganizationPairingCode DAO 接口
#[async_trait::async_trait]
pub trait OrganizationPairingDao: Send + Sync {
    /// 插入一条配对码记录
    async fn insert(&self, ctx: RequestContext, code: &OrganizationPairingCodePo) -> Result<()>;

    /// 原子消费配对码
    ///
    /// 仅当 `code_hash` 存在、未消费（`consumed_at IS NULL`）、未过期（`expires_at > now`）
    /// 时置 `consumed_at` 并返回签发组织 ID。
    ///
    /// 任何不满足（无效码 / 已过期 / 已使用）均返回 `None`——上层统一转
    /// `Error::unauthorized`，不区分原因（防枚举探测，评审稿 §6.3）。
    async fn consume(
        &self,
        ctx: RequestContext,
        code_hash: &str,
        now: i64,
    ) -> Result<Option<String>>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
