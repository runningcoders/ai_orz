//! Organization DAL 模块
//!
//! 职责：Organization 领域的数据访问层，封装 OrganizationDao 提供统一的查询接口
//! 注意：User 相关操作已移至 User DAL，跨领域编排在 Domain 层完成

use crate::models::events::OrganizationChangedEvent;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dao::organization;
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery, PeerOrgUpsert};
use common::api::OrganizationConfig;
use common::error::Result;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static ORGANIZATION_DAL: OnceLock<Arc<dyn OrganizationDal + Send + Sync>> = OnceLock::new();

/// 获取 Organization DAL 单例
pub fn dal() -> Arc<dyn OrganizationDal + Send + Sync> {
    ORGANIZATION_DAL.get().cloned().unwrap()
}

/// 初始化 Organization DAL
pub fn init() {
    let _ = ORGANIZATION_DAL.set(new(
        organization::dao(),
        crate::service::dao::organization_link::dao(),
    ));
}

/// 创建 Organization DAL（返回 trait 对象）
pub fn new(
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
    link_dao: Arc<dyn crate::service::dao::organization_link::OrganizationLinkDao + Send + Sync>,
) -> Arc<dyn OrganizationDal + Send + Sync> {
    Arc::new(OrganizationDalImpl {
        organization_dao,
        link_dao,
    })
}

// ==================== DAL 接口 ====================

/// Organization DAL 接口
#[async_trait::async_trait]
pub trait OrganizationDal: Send + Sync {
    /// 检查系统是否已经初始化
    ///
    /// 通过检查 organizations 表是否有记录判断
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool>;

    /// 根据 ID 获取组织
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>>;

    /// 根据邀请码获取组织（仅返回未删除的有效组织）
    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>>;

    /// 读取组织级配置（透传 DAO，带缓存）
    async fn get_org_config(&self, ctx: RequestContext, org_id: &str)
    -> Result<OrganizationConfig>;

    /// 写入组织级配置（透传 DAO，写穿缓存）
    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()>;

    /// 创建组织
    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>>;

    /// 获取所有组织
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>>;

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()>;

    /// 统计组织总数
    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64>;

    /// 统计符合查询条件的组织数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64>;

    // ==================== 联邦影子（静默写入：不发布 organization.changed）==========
    //
    // organizations 表承载两类数据：业务组织（Local/Linked，上方方法管，写后发事件）
    // 与远端影子（Remote，复制同步所得，下方方法管，静默写入）。
    // 影子同步是**复制，不是业务变更**——不发布事件是刻意的：若发布，
    // 对端推送 → 写影子 → 发事件 → consumer 再推送 → 对端又推回，形成事件风暴。
    // 递归防护是结构性的：影子写入路径不经过事件发布点，不依赖任何运行时开关。
    // 新增副作用（缓存/审计）时：业务方法加在 create/update/delete；影子方法另行评估。

    /// 目录同步所得 Remote 影子 upsert（新者胜 / 不动 scope / 护 Local，评审稿 §5.2）
    ///
    /// 返回是否发生写入（false = 跳过），供上层审计。
    async fn upsert_remote_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 直接建联的对端影子 upsert（强制 Linked，评审稿 R5 护 Local）
    ///
    /// 返回是否发生写入，供上层审计。
    async fn upsert_linked_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 断联组合操作：连接置 Revoked + 对端影子 Linked → Remote
    ///
    /// 组合 link DAO（links 表）与 organization DAO（organizations 表）：
    /// 先断链后降影，非同一事务（跨 DAO 不开分布式事务，YAGNI）。两步各自幂等，
    /// 第二步失败时重试本方法即可修复（重放断链无害）。不发布事件（断联是
    /// 链接层状态变更，不改变本地组织元信息，无需触发目录变更推送）。
    async fn revoke_link(
        &self,
        ctx: RequestContext,
        link_id: &str,
        peer_org_id: &str,
    ) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Organization DAL 实现
struct OrganizationDalImpl {
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
    link_dao: Arc<dyn crate::service::dao::organization_link::OrganizationLinkDao + Send + Sync>,
}

#[async_trait::async_trait]
impl OrganizationDal for OrganizationDalImpl {
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool> {
        let count = self.organization_dao.count_all(ctx).await?;
        Ok(count > 0)
    }

    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>> {
        self.organization_dao.find_by_id(ctx, org_id).await
    }

    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>> {
        self.organization_dao
            .find_by_invite_code(ctx, invite_code)
            .await
    }

    async fn get_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<OrganizationConfig> {
        self.organization_dao.get_org_config(ctx, org_id).await
    }

    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()> {
        self.organization_dao
            .set_org_config(ctx, org_id, config)
            .await
    }

    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        self.organization_dao.insert(ctx.clone(), org).await?;
        aop::publish(&ctx, OrganizationChangedEvent::new(&org.id, "created")).await;
        Ok(())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>> {
        self.organization_dao.query(ctx, query).await
    }

    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>> {
        self.query(ctx, OrganizationQuery::default()).await
    }

    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        self.organization_dao.update(ctx.clone(), org).await?;
        aop::publish(&ctx, OrganizationChangedEvent::new(&org.id, "updated")).await;
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()> {
        self.organization_dao.delete(ctx.clone(), org_id).await?;
        aop::publish(&ctx, OrganizationChangedEvent::new(org_id, "deleted")).await;
        Ok(())
    }

    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(ctx, OrganizationQuery::default()).await
    }

    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64> {
        self.organization_dao.count(ctx, query).await
    }

    async fn upsert_remote_shadow(
        &self,
        ctx: RequestContext,
        peer: &PeerOrgUpsert,
    ) -> Result<bool> {
        self.organization_dao.upsert_remote_shadow(ctx, peer).await
    }

    async fn upsert_linked_shadow(
        &self,
        ctx: RequestContext,
        peer: &PeerOrgUpsert,
    ) -> Result<bool> {
        self.organization_dao.upsert_linked_shadow(ctx, peer).await
    }

    async fn revoke_link(
        &self,
        ctx: RequestContext,
        link_id: &str,
        peer_org_id: &str,
    ) -> Result<()> {
        // 1) 连接置 Revoked（links 表，幂等）
        self.link_dao.revoke(ctx.clone(), link_id).await?;
        // 2) 对端影子 Linked → Remote（organizations 表，幂等）；失败向上传播，
        //    调用方重试即可修复（重放断链无害）
        self.organization_dao
            .degrade_shadow_to_remote(ctx, peer_org_id)
            .await?;
        Ok(())
    }
}
