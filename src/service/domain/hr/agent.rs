//! Agent 管理具体方法实现

use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentDal;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};
use common::enums::AgentStatus;
use common::error::{Result, err, bail_err};

use crate::enrich_ctx;

#[async_trait::async_trait]
impl AgentManage for HrDomainImpl {
    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    /// 强制校验：必须指定 model_provider_id，创建后状态固定为 Interviewing
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        // 强制校验：必须指定 model_provider_id
        if agent.po.model_provider_id.is_empty() {
            bail_err!(InvalidRequest, "创建 Agent 必须指定 model_provider_id");
        }

        // 强制校验：状态必须是 Interviewing
        if agent.po.status != AgentStatus::Interviewing {
            bail_err!(InvalidRequest, "新建 Agent 状态必须为 Interviewing");
        }

        self.agent_dal.create(ctx, agent).await
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    async fn get_agent(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>> {
        self.agent_dal.find_by_id(ctx, id).await
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>> {
        self.agent_dal.query(ctx, query).await
    }

    /// 列出所有 Agent
    ///
    /// 语法糖：调用通用查询，默认排除已删除状态
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        self.query(
            ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    async fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.agent_dal.update(ctx, agent).await
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    async fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.agent_dal.delete(ctx, agent).await
    }

    /// 状态流转
    ///
    /// 校验状态流转合法性，更新状态并持久化
    async fn transition_status(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        target_status: AgentStatus,
    ) -> Result<()> {
        // 补充 Agent 上下文
        let ctx = enrich_ctx!(&ctx, &*agent);

        let current_status = agent.po.status.clone();

        // 状态机校验：定义合法的流转路径
        let is_valid_transition = match (&current_status, &target_status) {
            // 面试中 → 待入职
            (AgentStatus::Interviewing, AgentStatus::PendingOnboard) => true,
            // 待入职 → 已入职
            (AgentStatus::PendingOnboard, AgentStatus::Onboarded) => true,
            // 已入职 → 待离职
            (AgentStatus::Onboarded, AgentStatus::PendingOffboard) => true,
            // 待离职 → 已离职
            (AgentStatus::PendingOffboard, AgentStatus::Offboarded) => true,
            // 任意状态 → 已删除
            (_, AgentStatus::Deleted) => true,
            // 同状态跳转：允许幂等
            (a, b) if a == b => true,
            // 其他情况：非法
            _ => false,
        };

        if !is_valid_transition {
            bail_err!(InvalidRequest, "非法状态流转：{:?} → {:?}", current_status, target_status);
        }

        // 幂等：状态相同直接返回
        if current_status == target_status {
            return Ok(());
        }

        // 更新状态
        agent.po.status = target_status;

        // 持久化
        self.agent_dal.update(ctx, agent).await
    }

    /// 校验入职就绪状态
    ///
    /// 检查工具绑定、技能安装等完整性条件
    async fn validate_onboard_readiness(
        &self,
        ctx: RequestContext,
        agent: &Agent,
    ) -> Result<()> {
        let agent_id = agent.po.id.as_str();

        // 1. 校验状态必须是 PendingOnboard
        if agent.po.status != AgentStatus::PendingOnboard {
            bail_err!(InvalidRequest, "Agent 状态必须是 PendingOnboard 才能入职，当前状态：{:?}", agent.po.status);
        }

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        // 2. 校验至少绑定了 1 个工具
        let tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), agent_id)
            .await?;
        if tools.is_empty() {
            bail_err!(InvalidRequest, "Agent 至少绑定 1 个工具才能入职");
        }

        // 3. 校验技能：没有技能只告警，不阻止入职
        let skills = self.skill_dal.list_for_agent(ctx.clone(), agent_id).await?;
        if skills.is_empty() {
            log_warn!(
                ctx.clone(),
                "onboard_agent",
                "Agent {} 未安装任何技能",
                agent_id
            );
        }

        Ok(())
    }
}