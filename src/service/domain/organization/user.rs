//! 用户管理 trait 实现
//!
//! 定义用户相关业务接口实现（组织管理域只管用户本身；
//! 身份凭证资产归 finance domain 的 IdentityCredentialManage）

use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::error::{Error, Result, bail_err};

#[async_trait]
impl super::UserManage for super::OrganizationDomainImpl {
    /// 根据用户名查询用户（用于登录）
    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>> {
        self.user_dal.find_by_username(ctx, username).await
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::user::UserQuery,
    ) -> Result<common::api::PagedResult<UserPo>> {
        self.user_dal.query(ctx, query).await
    }

    /// 根据组织 ID 查询所有用户
    ///
    /// 调用 DAL 层 find_by_organization_id 方法
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>> {
        self.user_dal.find_by_organization_id(ctx, org_id).await
    }

    /// 获取组织接待用户（联邦访客的内部对接身份，P6）
    async fn reception_user(&self, ctx: RequestContext, org_id: &str) -> Result<UserPo> {
        self.user_dal
            .find_reception_user(ctx, org_id)
            .await?
            .ok_or_else(|| Error::not_found(format!("组织 {org_id} 无可用接待用户（缺少管理员）")))
    }

    /// 创建新用户
    async fn create_user(&self, ctx: RequestContext, user: UserPo) -> Result<()> {
        self.user_dal.create(ctx, &user).await
    }

    /// 更新用户信息
    async fn update_user(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        self.user_dal.update(ctx, user).await
    }

    /// 删除用户（软删除）
    async fn delete_user(&self, ctx: RequestContext, user_id: &str) -> Result<()> {
        self.user_dal.delete(ctx, user_id).await
    }

    /// 检查用户名是否已存在
    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool> {
        self.user_dal.exists_by_username(ctx, username).await
    }

    /// 统计组织下用户总数
    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count_users
        self.count_users(
            ctx,
            crate::service::dao::user::UserQuery {
                organization_id: Some(org_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    /// 统计符合查询条件的用户数量（透传 DAL count）
    async fn count_users(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::user::UserQuery,
    ) -> Result<u64> {
        self.user_dal.count(ctx, query).await
    }

    /// 验证用户名密码（用于登录）
    ///
    /// 兼容历史明文口令：非 bcrypt 存储值按明文比对，命中后当场重哈希回写（透明升级）
    async fn verify_password(
        &self,
        ctx: RequestContext,
        org_id: &str,
        username: &str,
        password: &str,
    ) -> Result<UserPo> {
        // 先查找用户
        let user = match self
            .user_dal
            .find_by_username(ctx.clone(), username)
            .await?
        {
            Some(u) => u,
            None => {
                bail_err!(InvalidRequest, "用户名或密码错误");
            }
        };

        // 检查用户所属组织是否匹配
        if user.organization_id.as_str() != org_id {
            bail_err!(InvalidRequest, "用户名或密码错误");
        }

        // 验证密码（bcrypt 或历史明文）
        let stored = user.password_hash.as_str();
        let matched = if crate::pkg::password::is_bcrypt_hash(stored) {
            crate::pkg::password::verify_password(password, stored)?
        } else if stored == password {
            // 历史明文命中：透明升级为 bcrypt
            let upgraded = crate::pkg::password::hash_password(password)?;
            let mut updated = user.clone();
            updated.password_hash = upgraded;
            self.user_dal.update(ctx, &updated).await?;
            true
        } else {
            false
        };
        if !matched {
            bail_err!(InvalidRequest, "用户名或密码错误");
        }

        // 用户状态检查：Active 表示启用
        if user.status != common::enums::UserStatus::Active {
            bail_err!(InvalidRequest, "用户已被禁用");
        }

        Ok(user)
    }

    /// 根据用户 ID 获取用户信息
    async fn get_user_by_id(&self, ctx: RequestContext, user_id: &str) -> Result<Option<UserPo>> {
        self.user_dal.find_by_id(ctx, user_id).await
    }

    /// 邀请码注册新成员（公开接口）
    async fn register_member(
        &self,
        ctx: RequestContext,
        req: common::api::RegisterByInviteRequest,
    ) -> Result<UserPo> {
        // 复用组织管理能力：方法定义在 OrganizationManage trait 上
        use super::OrganizationManage as _;

        // 1. 基础入参校验
        let username = req.username.trim().to_string();
        if username.is_empty() {
            bail_err!(InvalidRequest, "用户名不能为空");
        }
        if req.password.len() < 6 {
            bail_err!(InvalidRequest, "密码至少 6 位");
        }
        let hashed = crate::pkg::password::hash_password(&req.password)?;

        // 2. 验证邀请码并定位组织（归一化逻辑复用组织管理实现）
        let org = self
            .find_org_by_invite_code(ctx.clone(), &req.invite_code)
            .await?;
        let org = match org {
            Some(o) => o,
            None => bail_err!(InvalidRequest, "邀请码无效或已过期"),
        };

        // 3. 用户名全局唯一预检（数据库 UNIQUE 约束兜底）
        if self.exists_by_username(ctx.clone(), &username).await? {
            bail_err!(InvalidRequest, "用户名 '{}' 已存在", username);
        }

        // 4. 创建 Member 用户（created_by 使用自身 ID，表示自注册）
        let display_name = req.display_name.unwrap_or_default().trim().to_string();
        let user_id = super::org::generate_user_id();
        let user = UserPo::new(
            user_id.clone(),
            org.id,
            username,
            display_name,
            String::new(), // 注册接口暂不收集 email
            hashed,
            common::enums::UserRole::Member,
            user_id,
        );

        self.user_dal.create(ctx, &user).await?;

        Ok(user)
    }
}
