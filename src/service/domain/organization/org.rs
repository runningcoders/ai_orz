//! 组织管理 trait 实现
//!
//! 定义组织相关业务接口实现

use crate::models::organization::OrganizationPo;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::api::OrganizationConfig;
use common::error::Result;
use rand::Rng;

/// 生成组织 ID（12 位大写字母 + 数字）
fn generate_org_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const LEN: usize = 12;
    let mut rng = rand::thread_rng();
    (0..LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 生成用户 ID（16 位大写字母 + 数字）
pub(super) fn generate_user_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const LEN: usize = 16;
    let mut rng = rand::thread_rng();
    (0..LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[async_trait]
impl super::OrganizationManage for super::OrganizationDomainImpl {
    /// 检查系统是否已经初始化（即是否存在 Local 组织）
    ///
    /// 一台设备只允许有一个 Local 组织；Remote 组织可存在多个（记录交互信息）。
    async fn check_initialized(&self, ctx: RequestContext) -> Result<bool> {
        use crate::service::dao::organization::OrganizationQuery;
        use common::enums::OrganizationScope;

        let count = self
            .org_dal
            .count(
                ctx,
                OrganizationQuery {
                    scope: Some(OrganizationScope::Local),
                    ..Default::default()
                },
            )
            .await?;
        Ok(count > 0)
    }

    /// 创建组织 + Owner（超级管理员角色）
    ///
    /// 通用方法：可用于系统初始化，也可用于后续创建新组织。
    /// 返回 (organization_id, user_id)
    async fn create_org_and_owner(
        &self,
        ctx: RequestContext,
        params: common::api::InitializeSystemRequest,
    ) -> Result<(String, String)> {
        // 1. 创建组织
        let org_id = generate_org_id();
        let org = OrganizationPo::new(
            org_id.clone(),
            params.organization_name,
            params.description.unwrap_or_default(),
            None,
            org_id.clone(), // 系统初始化时由组织自己创建
        );
        self.org_dal.create(ctx.clone(), &org).await?;

        // 2. 创建超级管理员用户
        let user_id = generate_user_id();
        let user = UserPo::new(
            user_id.clone(),
            org_id.clone(),
            params.admin_username,
            params
                .admin_display_name
                .unwrap_or_else(|| "超级管理员".to_string()),
            params.admin_email.unwrap_or_default(),
            crate::pkg::password::hash_password(&params.admin_password)?,
            common::enums::UserRole::SuperAdmin,
            org_id.clone(), // 系统初始化时由组织创建
        );
        self.user_dal.create(ctx.clone(), &user).await?;

        Ok((org_id, user_id))
    }

    /// 获取组织信息
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>> {
        self.org_dal.get_by_id(ctx, org_id).await
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::organization::OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>> {
        self.org_dal.query(ctx, query).await
    }

    /// 获取所有组织列表
    ///
    /// 调用 DAL 层 list_all 方法
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>> {
        self.org_dal.list_all(ctx).await
    }

    /// 根据邀请码获取组织（公开注册用，仅返回未删除的有效组织）
    ///
    /// 归一化规则：去首尾空白 + 统一大写（邀请码字符集本身全大写且不含易混淆的 I/O）
    async fn find_org_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>> {
        let code = invite_code.trim().to_ascii_uppercase();
        if code.is_empty() {
            return Ok(None);
        }
        self.org_dal.find_by_invite_code(ctx, &code).await
    }

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        self.org_dal.update(ctx, org).await
    }

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()> {
        self.org_dal.delete(ctx, org_id).await
    }

    /// 读取组织级配置（透传 DAL → DAO，带缓存）
    async fn get_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<OrganizationConfig> {
        self.org_dal.get_org_config(ctx, org_id).await
    }

    /// 写入组织级配置（透传 DAL → DAO，写穿缓存）
    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()> {
        self.org_dal.update_org_config(ctx, org_id, config).await
    }

    /// 统计符合查询条件的组织数量（透传 DAL count）
    async fn count_organizations(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::organization::OrganizationQuery,
    ) -> Result<u64> {
        self.org_dal.count(ctx, query).await
    }
}
