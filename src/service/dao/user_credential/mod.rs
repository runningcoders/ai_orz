//! UserCredential DAO 模块
//!
//! 用户身份凭证独立表（user_credentials）的行级数据访问：
//! - query 优先（D14）：通用 `query` + `UserCredentialQuery` 为核心查询能力，
//!   COUNT 与 LIST 复用 `push_query_filters` 同一套 WHERE（AGENTS §4.9）
//! - 语义读方法仅限 `find_by_id`（主键，活跃凭证）与 `find_default`
//!   （解析链 §2.3 链 2→5 单点实现，消费侧禁止各自实现）
//! - `set_default` 同事务「清同作用域旧默认 → 立新默认」（作用域由目标凭据
//!   visibility 派生：private=个人默认 / public=组织默认，双部分唯一索引兜底并发）

use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use common::api::PagedResult;
use common::error::Result;
use common::models::{CredentialKind, CredentialVisibility};

/// 用户凭证通用查询条件
///
/// 所有字段都是 Option：None 表示不限制该条件，Some(value) 表示必须匹配。
/// 无外部使用方维度过滤字段（agent_id/bound_tool 不存在，D12）——
/// 「我的凭据」「组织共享凭据」「个人默认」「某类型凭据」均由现有字段组合表达。
#[derive(Debug, Clone, Default)]
pub struct UserCredentialQuery {
    /// 按凭证 ID 查询（通常返回单条）
    pub id: Option<String>,
    /// 按组织 ID 查询
    pub org_id: Option<String>,
    /// 按归属用户 ID 查询
    pub user_id: Option<String>,
    /// 按凭证类型查询
    pub kind: Option<CredentialKind>,
    /// 按可见性查询（private / public）
    pub visibility: Option<CredentialVisibility>,
    /// 按默认标记查询（作用域由 visibility 派生）
    pub is_default: Option<bool>,
    /// 按状态 IN 查询（软删过滤默认 Active=1，查历史走显式 status_in）
    pub status_in: Option<Vec<i32>>,
    /// 凭证名模糊匹配（LIKE，凭证千行级无需 FTS5；仅展示检索，不参与解析）
    pub keyword: Option<String>,
    /// 分页参数
    pub pagination: common::api::PaginationParams,
    /// 排序规则，如 "created_at ASC", "created_at DESC"
    pub order_by: Option<String>,
}

// ==================== 接口 ====================

/// UserCredential DAO 接口
#[async_trait::async_trait]
pub trait UserCredentialDao: Send + Sync {
    /// 插入一条新凭证
    async fn insert(&self, ctx: RequestContext, po: &UserCredentialPo) -> Result<()>;

    /// 更新凭证（行级全字段更新：name/detail/visibility/is_default 由 Domain
    /// 编排后整体写入；modified_by 取 po，updated_at 取当前时间）
    async fn update(&self, ctx: RequestContext, po: &UserCredentialPo) -> Result<()>;

    /// 软删除凭证（status=0，operator 取 ctx）
    async fn soft_delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 通用查询（条件查询唯一入口，D14）
    async fn query(
        &self,
        ctx: RequestContext,
        query: UserCredentialQuery,
    ) -> Result<PagedResult<UserCredentialPo>>;

    /// 统计符合查询条件的凭证数量（复用 query 的 filter 逻辑，只跑 COUNT）
    async fn count(&self, ctx: RequestContext, query: UserCredentialQuery) -> Result<u64>;

    /// 按主键查找活跃凭证（status != 0；引用/解析语义下软删凭证视为不存在）
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserCredentialPo>>;

    /// 解析用户某类型可用凭证（§2.3 链 2→5 单点实现，作用域优先）
    ///
    /// 链序：个人默认 > 个人其他活跃（创建序）> 组织默认 > 组织其他 public 活跃；
    /// org 作用域经 JOIN users 取目标用户组织，调用方无需先查用户。
    async fn find_default(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<Option<UserCredentialPo>>;

    /// 设立默认凭证（同事务清旧立新；作用域由目标凭据 visibility 派生）
    ///
    /// 目标凭据 private → 清该用户该 kind 个人默认后立新；
    /// 目标凭据 public → 清该 org 该 kind 组织默认后立新。
    /// 目标不存在或已软删时返回 NotFound。
    async fn set_default(&self, ctx: RequestContext, credential_id: &str) -> Result<()>;

    /// 取消个人默认（该用户该 kind 的 private 默认清位，无默认时幂等无操作）
    async fn clear_default(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};
