//! User DAL 模块
//!
//! 职责：User 领域的数据访问层，封装 UserDao 提供统一的查询接口；
//! 凭证是用户资产的延伸，本 DAL 同时组合 UserCredentialDao 提供行级凭证方法
//! （不另立凭证 DAL，见 docs/design/user_credentials_design.md D11）。

use crate::models::user::UserPo;
use crate::models::user_credential::UserCredential;
use crate::pkg::RequestContext;
use crate::service::dao::user;
use crate::service::dao::user::{UserDao, UserQuery};
use crate::service::dao::user_credential::{UserCredentialDao, UserCredentialQuery};
use common::api::PagedResult;
use common::error::Result;
use common::models::CredentialKind;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static USER_DAL: OnceLock<Arc<dyn UserDal + Send + Sync>> = OnceLock::new();

/// 获取 User DAL 单例
pub fn dal() -> Arc<dyn UserDal + Send + Sync> {
    USER_DAL.get().cloned().unwrap()
}

/// 初始化 User DAL
pub fn init() {
    let _ = USER_DAL.set(new(user::dao(), crate::service::dao::user_credential::dao()));
}

/// 创建 User DAL（返回 trait 对象）
pub fn new(
    user_dao: Arc<dyn UserDao + Send + Sync>,
    credential_dao: Arc<dyn UserCredentialDao + Send + Sync>,
) -> Arc<dyn UserDal + Send + Sync> {
    Arc::new(UserDalImpl {
        user_dao,
        credential_dao,
    })
}

// ==================== DAL 接口 ====================

/// User DAL 接口
#[async_trait::async_trait]
pub trait UserDal: Send + Sync {
    /// 创建用户
    async fn create(&self, ctx: RequestContext, user: &UserPo) -> Result<()>;

    /// 根据 ID 获取用户
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>>;

    /// 根据用户名获取用户
    async fn find_by_username(&self, ctx: RequestContext, username: &str)
    -> Result<Option<UserPo>>;

    /// 通用综合查询
    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<PagedResult<UserPo>>;

    /// 获取组织下的所有用户
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>>;

    /// 更新用户信息
    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<()>;

    /// 删除用户（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 检查用户名是否存在
    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool>;

    /// 统计组织下的用户数量
    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64>;

    /// 统计符合查询条件的用户数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: UserQuery) -> Result<u64>;

    // ==================== 用户身份凭证（行级，一表一 DAO 组合，PO 不出 DAL） ====================

    /// 查询凭证（条件查询唯一入口，D14；软删默认过滤）
    async fn query_credentials(
        &self,
        ctx: RequestContext,
        query: UserCredentialQuery,
    ) -> Result<PagedResult<UserCredential>>;

    /// 统计凭证数量（复用 query 过滤条件）
    async fn count_credentials(
        &self,
        ctx: RequestContext,
        query: UserCredentialQuery,
    ) -> Result<u64>;

    /// 插入凭证
    async fn insert_credential(&self, ctx: RequestContext, credential: &UserCredential)
    -> Result<()>;

    /// 按主键查找活跃凭证（引用/解析语义下软删凭证视为不存在）
    async fn find_credential_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<UserCredential>>;

    /// 更新凭证（行级全字段：name/detail/visibility/is_default 由 Domain 编排后整体写入）
    async fn update_credential(&self, ctx: RequestContext, credential: &UserCredential)
    -> Result<()>;

    /// 软删除凭证（默认标记联动清除）
    async fn soft_delete_credential(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 解析用户某类型可用凭证（§2.3 链 2→5 单点：个人默认 > 个人其他 > 组织默认 > 组织其他 public）
    async fn find_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<Option<UserCredential>>;

    /// 设立默认凭证（作用域由目标凭据 visibility 派生；同事务清旧立新）
    async fn set_default_credential(&self, ctx: RequestContext, credential_id: &str)
    -> Result<()>;

    /// 取消个人默认（该用户该 kind 的 private 默认清位，无默认时幂等无操作）
    async fn clear_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// User DAL 实现
struct UserDalImpl {
    user_dao: Arc<dyn UserDao + Send + Sync>,
    /// 凭证 DAO（user_credentials 表，一表一 DAO）
    credential_dao: Arc<dyn UserCredentialDao + Send + Sync>,
}

#[async_trait::async_trait]
impl UserDal for UserDalImpl {
    async fn create(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        self.user_dao.insert(ctx, user).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>> {
        self.user_dao.find_by_id(ctx, id).await
    }

    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>> {
        self.user_dao.find_by_username(ctx, username).await
    }

    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<PagedResult<UserPo>> {
        self.user_dao.query(ctx, query).await
    }

    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>> {
        let page = self
            .query(
                ctx,
                UserQuery {
                    organization_id: Some(org_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        self.user_dao.update(ctx, user).await
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.user_dao.delete(ctx, id).await
    }

    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool> {
        self.user_dao.exists_by_username(ctx, username).await
    }

    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            UserQuery {
                organization_id: Some(org_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count(&self, ctx: RequestContext, query: UserQuery) -> Result<u64> {
        self.user_dao.count(ctx, query).await
    }

    async fn query_credentials(
        &self,
        ctx: RequestContext,
        query: UserCredentialQuery,
    ) -> Result<PagedResult<UserCredential>> {
        Ok(self
            .credential_dao
            .query(ctx, query)
            .await?
            .map(UserCredential::from_po))
    }

    async fn count_credentials(
        &self,
        ctx: RequestContext,
        query: UserCredentialQuery,
    ) -> Result<u64> {
        self.credential_dao.count(ctx, query).await
    }

    async fn insert_credential(
        &self,
        ctx: RequestContext,
        credential: &UserCredential,
    ) -> Result<()> {
        self.credential_dao.insert(ctx, &credential.po).await
    }

    async fn find_credential_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<UserCredential>> {
        Ok(self
            .credential_dao
            .find_by_id(ctx, id)
            .await?
            .map(UserCredential::from_po))
    }

    async fn update_credential(
        &self,
        ctx: RequestContext,
        credential: &UserCredential,
    ) -> Result<()> {
        self.credential_dao.update(ctx, &credential.po).await
    }

    async fn soft_delete_credential(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.credential_dao.soft_delete(ctx, id).await
    }

    async fn find_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<Option<UserCredential>> {
        Ok(self
            .credential_dao
            .find_default(ctx, user_id, kind)
            .await?
            .map(UserCredential::from_po))
    }

    async fn set_default_credential(&self, ctx: RequestContext, credential_id: &str) -> Result<()> {
        self.credential_dao.set_default(ctx, credential_id).await
    }

    async fn clear_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<()> {
        self.credential_dao.clear_default(ctx, user_id, kind).await
    }
}

// ==================== gh_cli 凭证解析器 ====================

/// gh_cli 工具身份来源：按解析链取用户可用 GitHub token（解密后明文）
///
/// 解析链 §2.3 链 2→5（个人默认 > 个人其他 > 组织默认 > 组织其他 public）
/// 单点收敛在 UserCredentialDao::find_default。
pub struct GhDalCredentialResolver;

#[async_trait::async_trait]
impl crate::pkg::tool_registry::gh_cli::GhCredentialResolver for GhDalCredentialResolver {
    async fn resolve(&self, ctx: &RequestContext) -> Result<Option<String>> {
        let Some(user_id) = ctx.user_id.clone() else {
            return Ok(None);
        };
        let Some(credential) = dal()
            .find_default_credential(ctx.clone(), &user_id, CredentialKind::GithubToken)
            .await?
        else {
            return Ok(None);
        };
        let common::models::CredentialDetail::GithubToken { token } = credential.detail() else {
            return Ok(None);
        };
        Ok(Some(crate::pkg::crypto::decrypt_channel_secret(token.as_str())?))
    }
}

// ==================== tavily_search 凭证解析器 ====================

/// tavily_search 工具身份来源：按解析链取用户可用 Tavily API key（解密后明文）
///
/// 解析链 §2.3 链 2→5 单点收敛在 UserCredentialDao::find_default。
pub struct TavilyDalCredentialResolver;

#[async_trait::async_trait]
impl crate::pkg::tool_registry::tavily_search::TavilyCredentialResolver
    for TavilyDalCredentialResolver
{
    async fn resolve(&self, ctx: &RequestContext) -> Result<Option<String>> {
        let Some(user_id) = ctx.user_id.clone() else {
            return Ok(None);
        };
        let Some(credential) = dal()
            .find_default_credential(ctx.clone(), &user_id, CredentialKind::TavilyKey)
            .await?
        else {
            return Ok(None);
        };
        let common::models::CredentialDetail::TavilyKey { api_key } = credential.detail() else {
            return Ok(None);
        };
        Ok(Some(crate::pkg::crypto::decrypt_channel_secret(api_key.as_str())?))
    }
}
