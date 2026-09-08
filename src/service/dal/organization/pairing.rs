//! 组网配对码 DAL——organization_pairing_codes 表
//!
//! 封装 `OrganizationPairingDao`，供 Organization Domain 消费。

use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::pkg::RequestContext;
use crate::service::dao::organization_pairing::OrganizationPairingDao;
use common::error::Result;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static PAIRING_DAL: OnceLock<Arc<dyn OrganizationPairingDal + Send + Sync>> = OnceLock::new();

/// 获取配对码 DAL 单例
pub fn dal() -> Arc<dyn OrganizationPairingDal + Send + Sync> {
    PAIRING_DAL.get().cloned().unwrap()
}

/// 初始化配对码 DAL 单例
pub fn init() {
    let _ = PAIRING_DAL.set(new(crate::service::dao::organization_pairing::dao()));
}

/// 创建配对码 DAL（返回 trait 对象）
pub fn new(
    pairing_dao: Arc<dyn OrganizationPairingDao + Send + Sync>,
) -> Arc<dyn OrganizationPairingDal + Send + Sync> {
    Arc::new(OrganizationPairingDalImpl { pairing_dao })
}

// ==================== DAL 接口 ====================

/// 组网配对码 DAL 接口
#[async_trait::async_trait]
pub trait OrganizationPairingDal: Send + Sync {
    /// 插入配对码记录（明文不入库，仅存哈希）
    async fn insert(&self, ctx: RequestContext, code: &OrganizationPairingCodePo) -> Result<()>;

    /// 原子消费配对码
    ///
    /// 仅当 `code_hash` 存在、未消费、未过期时置 `consumed_at` 并返回签发组织 ID；
    /// 任何不满足（无效码 / 已过期 / 已使用）均返回 `None`——上层统一转
    /// `Error::unauthorized`，不区分原因（防枚举探测，评审稿 §6.3）。
    async fn consume(
        &self,
        ctx: RequestContext,
        code_hash: &str,
        now: i64,
    ) -> Result<Option<String>>;
}

// ==================== DAL 实现 ====================

/// 组网配对码 DAL 实现
struct OrganizationPairingDalImpl {
    /// 配对码 DAO（私有）
    pairing_dao: Arc<dyn OrganizationPairingDao + Send + Sync>,
}

#[async_trait::async_trait]
impl OrganizationPairingDal for OrganizationPairingDalImpl {
    async fn insert(&self, ctx: RequestContext, code: &OrganizationPairingCodePo) -> Result<()> {
        self.pairing_dao.insert(ctx, code).await
    }

    async fn consume(
        &self,
        ctx: RequestContext,
        code_hash: &str,
        now: i64,
    ) -> Result<Option<String>> {
        self.pairing_dao.consume(ctx, code_hash, now).await
    }
}
