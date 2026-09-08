//! Agent 运行时出站 DAL——按 Agent 运行时配置构造外部运行时客户端并执行出站调用
//!
//! 与 `a2a.rs` / `codex.rs`（数据面派生 Dal）不同，本模块封装
//! 「Agent 运行时配置 → 出站客户端」的构造 + 调用，供 Domain 层消费：
//! Producer / Consumer 等组装层不再直捅 `dao/agent_runtime`。

use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dao::agent_runtime::a2a::{A2aRuntimeConfig, A2aRuntimeDao};
use common::api::a2a::A2aTask;
use common::error::{Result, err};

/// Agent 运行时出站 DAL 接口
#[async_trait::async_trait]
pub trait AgentRuntimeDal: Send + Sync {
    /// 按远端 Agent 的运行时配置构造 A2A 客户端并拉取远端任务（tasks/get）
    ///
    /// 运行时配置缺失/非法（非 Remote 类型或 external_config 不完整）返回
    /// InvalidRequest 错误，由调用方决定跳过策略。
    async fn fetch_remote_task(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        remote_task_id: &str,
    ) -> Result<A2aTask>;
}

/// Agent 运行时出站 DAL 实现
///
/// 无状态：每次调用按 Agent 配置临时构造轻量 HTTP 客户端。
pub struct AgentRuntimeDalImpl;

#[async_trait::async_trait]
impl AgentRuntimeDal for AgentRuntimeDalImpl {
    async fn fetch_remote_task(
        &self,
        _ctx: RequestContext,
        agent: &Agent,
        remote_task_id: &str,
    ) -> Result<A2aTask> {
        let config = agent.po.get_remote_config().ok_or_else(|| {
            err!(
                InvalidRequest,
                "远端 Agent {} 缺少或非法的 remote 运行时配置",
                agent.po.id
            )
        })?;
        let dao = A2aRuntimeDao::new(A2aRuntimeConfig {
            endpoint: config.endpoint,
            agent_name: config.agent_name,
            auth_token: config.auth_token,
            timeout_secs: config.timeout_secs,
        });
        dao.fetch_task(remote_task_id).await
    }
}
