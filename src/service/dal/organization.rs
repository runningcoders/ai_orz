//! Organization DAL 模块
//!
//! 职责：组合 OrganizationDao + UserDao，完成组织初始化和基础管理
//! 因为用户必须属于组织，所以初始化需要同时创建组织和超级管理员用户，放在一个 DAL 里更合理

use common::enums::UserRole;
use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{dao as organization_dao, OrganizationDaoTrait};
use crate::service::dao::user::{dao as user_dao, UserDaoTrait};
use rand::Rng;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static ORGANIZATION_DAL: OnceLock<Arc<dyn OrganizationDalTrait + Send + Sync>> = OnceLock::new();

/// 获取 Organization DAL 单例
pub fn dal() -> Arc<dyn OrganizationDalTrait + Send + Sync> {
    ORGANIZATION_DAL.get().cloned().unwrap()
}

/// 初始化 Organization DAL
pub fn init() {
    let org_dao = organization_dao();
    let u_dao = user_dao();
    let _ = ORGANIZATION_DAL.set(Arc::new(OrganizationDal::new(org_dao, u_dao)));
}

// ==================== DAL 接口 ====================

/// Organization DAL 接口
pub trait OrganizationDalTrait: Send + Sync {
    /// 初始化系统：创建第一个组织和第一个超级管理员用户
    ///
    /// 用于系统首次初始化，当 organizations 表为空时调用
    /// - organization_name: 组织名称
    /// - username: 超级管理员用户名
    /// - password_hash: 密码哈希（bcrypt）
    /// - display_name: 超级管理员显示名称
    /// - email: 超级管理员邮箱
    /// - 返回: (organization_id, user_id)
    fn initialize_system(
        &self,
        ctx: RequestContext,
        organization_name: String,
        description: Option<String>,
        username: String,
        password_hash: String,
        display_name: Option<String>,
        email: Option<String>,
    ) -> Result<(String, String), AppError>;

    /// 检查系统是否已经初始化
    ///
    /// 通过检查 organizations 表是否有记录判断
    fn is_initialized(&self, ctx: RequestContext) -> Result<bool, AppError>;

    /// 根据 ID 获取组织
    fn get_by_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Option<OrganizationPo>, AppError>;

    /// 获取所有组织
    fn list_all(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OrganizationPo>, AppError>;

    /// 更新组织信息
    fn update(
        &self,
        ctx: RequestContext,
        org: &OrganizationPo,
    ) -> Result<(), AppError>;

    /// 删除组织（软删除）
    fn delete(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<(), AppError>;

    /// 统计组织总数
    fn count_organizations(
        &self,
        ctx: RequestContext,
    ) -> Result<u64, AppError>;
}

// ==================== DAL 实现 ====================

/// Organization DAL 实现
pub struct OrganizationDal {
    organization_dao: Arc<dyn OrganizationDaoTrait + Send + Sync>,
    user_dao: Arc<dyn UserDaoTrait + Send + Sync>,
}

impl OrganizationDal {
    /// 创建 DAL 实例
    pub fn new(
        organization_dao: Arc<dyn OrganizationDaoTrait + Send + Sync>,
        user_dao: Arc<dyn UserDaoTrait + Send + Sync>,
    ) -> Self {
        Self {
            organization_dao,
            user_dao,
        }
    }

    /// 生成随机 ID
    fn generate_id(&self) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        const ID_LEN: usize = 16;
        let mut rng = rand::thread_rng();
        let id: String = (0..ID_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        id
    }
}

impl OrganizationDalTrait for OrganizationDal {
    fn initialize_system(
        &self,
        ctx: RequestContext,
        organization_name: String,
        description: Option<String>,
        username: String,
        password_hash: String,
        display_name: Option<String>,
        email: Option<String>,
    ) -> Result<(String, String), AppError> {
        // 1. 生成组织 ID
        let org_id = self.generate_id();

        // 2. 创建组织
        let org = OrganizationPo::new(
            org_id.clone(),
            organization_name,
            description.unwrap_or_default(),
            String::new(), // base_url 默认为空，后续可在组织设置中修改
            ctx.uid().clone(),
        );
        self.organization_dao.insert(ctx.clone(), &org)?;

        // 3. 生成用户 ID
        let user_id = self.generate_id();

        // 4. 创建超级管理员用户
        let user = UserPo::new(
            user_id.clone(),
            org_id.clone(),
            username,
            display_name.unwrap_or_default(),
            email.unwrap_or_default(),
            password_hash,
            UserRole::SuperAdmin,
            ctx.uid().clone(),
        );
        self.user_dao.insert(ctx, &user)?;

        // 5. 返回 ID
        Ok((org_id, user_id))
    }

    fn is_initialized(&self, ctx: RequestContext) -> Result<bool, AppError> {
        let count = self.organization_dao.count_all(ctx)?;
        Ok(count > 0)
    }

    fn get_by_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Option<OrganizationPo>, AppError> {
        self.organization_dao.find_by_id(ctx, org_id)
    }

    fn list_all(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OrganizationPo>, AppError> {
        self.organization_dao.find_all(ctx)
    }

    fn update(
        &self,
        ctx: RequestContext,
        org: &OrganizationPo,
    ) -> Result<(), AppError> {
        self.organization_dao.update(ctx, org)
    }

    fn delete(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<(), AppError> {
        self.organization_dao.delete(ctx, org_id)
    }

    fn count_organizations(
        &self,
        ctx: RequestContext,
    ) -> Result<u64, AppError> {
        self.organization_dao.count_all(ctx)
    }
}
