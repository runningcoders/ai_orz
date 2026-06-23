//! Runtime Awakening 具体实现

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};

use super::context_assembly::PromptBuilder;

#[async_trait::async_trait]
impl RuntimeAwakening for RuntimeDomainImpl {
    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
    ) -> Result<AwakeningResult, AppError> {
        // Step 1: 读取最近短期记忆
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, None, 20)
            .await?;

        // Step 2: 收集关联的 Trace ID 列表
        // 简易版：暂时为空，后续从 message.metadata 提取
        let trace_ids: Vec<String> = vec![];

        // Step 3: 预先构造 MemoryTrace 拿到 trace_id
        // 调用方负责组装 trace，RuntimeMemory 负责写入和补全信息
        use common::enums::MemoryRole;
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(), // input 后续填充
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // Step 4: 拼装 Prompt（注入 trace_id 到头部，模型可见）
        let builder = PromptBuilder::new()
            .current_trace_id(&trace_id)
            .trace_ids(&trace_ids)
            .agent_system(agent)
            .agent_tools(agent)
            .history(&recent_memories)
            .current_message(message);

        // 【角色分工】只有客服类 Agent 才需要拼接用户喜好等信息
        // TODO: 后续从上层 Domain 传入用户画像信息
        let agent_roles = agent.po.get_roles();
        if agent_roles.contains(&"customer_service".to_string())
            || agent_roles.contains(&"客服".to_string())
        {
            // 预留：实际使用时从上层传入用户画像
            // builder = builder.user_profile(user_profile_str);
        }

        let prompt = builder.build();

        // Step 5: 调用大脑思考
        // 统一走 BrainDal.think() 入口，方便审计、统计、监控
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| AppError::Internal("Agent 大脑未唤醒，请先调用 wake_brain()".into()))?;

        let raw_output = self.brain_dal().think(ctx.clone(), brain, &prompt).await?;

        // Step 6: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // Step 7: 通过 RuntimeMemory 子模块写入
        // 架构：awakening → RuntimeMemory → MemoryDal → MemoryDao
        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 8: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id],
            raw_input: prompt,
            raw_output,
        })
    }
}
