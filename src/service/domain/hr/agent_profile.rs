//! Agent 业务目录（第二层）：先定「要哪些 Agent」，再按 ID 取实体
//!
//! 分层：
//! - 第一层（实体）：`AgentDao` 的 `hr.agent_dao.entities`，按 id 缓存 AgentPo，
//!   任意查询回填、写路径失效；
//! - 第二层（业务语义）：本模块缓存「这类业务要哪些 Agent」的 ID 名单
//!   （`hr.agent_directory.ids`），再调用 `find_by_ids` 拿实体。
//!
//! 两层各管各的过期：实体按 id 各自过期，名单过期只影响成员集合，
//! 不会让某个已缓存的 Agent 凭空消失。运行时状态不进入缓存，由 DAL 实时注入。

use std::sync::OnceLock;
use std::time::Duration;

use common::enums::AgentStatus;
use common::error::Result;

use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::pkg::cache::TtlCache;
use crate::service::dao::agent::{AgentQuery, dao as agent_dao};

/// 名单 TTL：写路径会主动失效，30s 仅作兜底
const IDS_TTL: Duration = Duration::from_secs(30);
/// 「全部未删除 Agent」这一业务语义的名单键
const ALL_ACTIVE: &str = "all_active";

fn ids_cache() -> &'static TtlCache<&'static str, Vec<String>> {
    static CACHE: OnceLock<&'static TtlCache<&'static str, Vec<String>>> = OnceLock::new();
    CACHE.get_or_init(|| crate::pkg::cache::register("hr.agent_directory.ids", IDS_TTL))
}

/// Agent 基础档案（不含运行时状态）
#[derive(Debug, Clone, PartialEq)]
pub struct AgentProfile {
    /// Agent ID
    pub id: String,
    /// 名称
    pub name: String,
    /// 角色标签
    pub roles: Vec<String>,
    /// 描述（空串归一为 `None`）
    pub description: Option<String>,
    /// 类型：local / cli / remote
    pub kind: String,
    /// 关联模型提供商 ID
    pub model_provider_id: String,
    /// 生命周期状态（`AgentStatus as i32`）
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
}

/// 取全部未删除 Agent 的基础档案（按 `created_at DESC`，与列表接口一致）
///
/// 名单命中：只补齐缺失实体；名单未命中：执行一次语义查询拿 id（顺带回填实体）。
pub async fn profiles(ctx: &RequestContext) -> Result<Vec<AgentProfile>> {
    let dao = agent_dao();

    if let Some(ids) = ids_cache().get(&ALL_ACTIVE) {
        let agents = dao.find_by_ids(ctx.clone(), &ids).await?;
        return Ok(agents.iter().map(profile_of).collect());
    }

    let page = dao
        .query(
            ctx.clone(),
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await?;
    let ids: Vec<String> = page.items.iter().map(|po| po.id.clone()).collect();
    ids_cache().insert(ALL_ACTIVE, ids);
    Ok(page.items.iter().map(profile_of).collect())
}

/// 第二层失效：Agent 成员 / 状态变化后清空名单
///
/// 实体失效由第一层（DAO 写路径）自行负责，这里只管业务语义的成员集合。
pub fn invalidate_ids() {
    ids_cache().invalidate(&ALL_ACTIVE);
}

fn profile_of(po: &AgentPo) -> AgentProfile {
    AgentProfile {
        id: po.id.clone(),
        name: po.name.clone(),
        roles: po.get_roles(),
        description: (!po.description.is_empty()).then(|| po.description.clone()),
        kind: po.kind.to_string(),
        model_provider_id: po.model_provider_id.clone(),
        status: po.status as i32,
        created_at: po.created_at,
    }
}
