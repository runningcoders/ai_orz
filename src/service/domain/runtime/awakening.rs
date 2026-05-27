//! Runtime Awakening 具体实现

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::message::Message;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl, ThinkingTraceType,
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

        // Step 3: 【思考闭环 - 前置】先创建空的 MemoryTrace 拿到 trace_id
        // 这样 trace_id 可以注入到 prompt 头部，模型能看到并引用
        use crate::models::memory::{MemoryCreateParams, MemoryTrace};
        use crate::service::dal::memory::dal;

        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            common::enums::MemoryRole::Assistant,
            String::new(), // 占位符，稍后替换为完整 prompt
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // Step 4: 拼装 Prompt（注入本次思考的 trace_id）
        let mut builder = PromptBuilder::new()
            .current_trace_id(&trace_id)
            .trace_ids(&trace_ids)
            .agent_system(agent)
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

        // 更新 trace 的 input 字段
        trace.input = prompt.clone();

        // Step 5: 写入未完成的 Trace（只有 input，output 待回填）
        dal().create(ctx.clone(), MemoryCreateParams::AppendTraces(vec![trace])).await?;

        // Step 5: 调用大脑思考
        // 统一走 BrainDal.think() 入口，方便审计、统计、监控
        let brain = agent.brain.as_ref()
            .ok_or_else(|| AppError::Internal("Agent 大脑未唤醒，请先调用 wake_brain()".into()))?;
        
        let raw_output = self.brain_dal()
            .think(ctx.clone(), brain, &prompt)
            .await?;

        // Step 6: 【思考闭环 - 步骤2/2】回填输出，完成 Trace
        // 直接通过 DAO 更新 output 和 completed_at 字段
        dal().complete_trace(ctx.clone(), &trace_id, &raw_output).await?;

        // Step 7: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id.clone()],
            raw_input: prompt,
            raw_output,
        })
    }
}
