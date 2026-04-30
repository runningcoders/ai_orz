//! 组织管理 trait 实现
//!
//! 定义组织相关业务接口实现

use rand::Rng;
use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;

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
fn generate_user_id() -> String {
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
    /// 检查系统是否已经初始化
    async fn check_initialized(&self, ctx: RequestContext) -> Result<bool, AppError> {
        let count = self.org_dal.count_organizations(ctx).await?;
        Ok(count > 0)
    }

    /// 初始化系统：创建第一个组织和第一个超级管理员用户
    ///
    /// 返回: (organization_id, user_id)
    async fn initialize_system(
        &self,
        ctx: RequestContext,
        organization_name: String,
        description: Option<String>,
        username: String,
        password_hash: String,
        display_name: Option<String>,
        email: Option<String>,
    ) -> Result<(String, String), AppError> {
        // 1. 创建组织
        let org_id = generate_org_id();
        let org = OrganizationPo::new(
            org_id.clone(),
            organization_name,
            description.unwrap_or_default(),
            None,
            org_id.clone(), // 系统初始化时由组织自己创建
        );
        self.org_dal.create(ctx.clone(), &org).await?;

        // 2. 创建超级管理员用户
        let user_id = generate_user_id();
        let user = UserPo::new(
            user_id.clone(),
            org_id.clone(),
            username,
            display_name.unwrap_or_else(|| "超级管理员".to_string()),
            email.unwrap_or_default(),
            password_hash,
            common::enums::UserRole::SuperAdmin,
            org_id.clone(), // 系统初始化时由组织创建
        );
        self.user_dal.create(ctx, &user).await?;

        Ok((org_id, user_id))
    }

    /// 获取组织信息
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>, AppError> {
        self.org_dal.get_by_id(ctx, org_id).await
    }

    /// 获取所有组织列表
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        self.org_dal.list_all(ctx).await
    }

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        self.org_dal.update(ctx, org).await
    }

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<(), AppError> {
        self.org_dal.delete(ctx, org_id).await
    }

    /// 统计组织总数
    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64, AppError> {
        self.org_dal.count_organizations(ctx).await
    }
}
