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

        // Step 3: 拼装 Prompt
        let mut builder = PromptBuilder::new()
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

        // Step 4: 生成本次输入的 Trace ID 并记录
        let input_trace_id = format!("trace-{}-{}", &agent.po.id, chrono::Utc::now().timestamp_nanos());
        let _input_trace = self
            .memory()
            .write_thinking_trace(
                ctx.clone(),
                &agent.po.id,
                ThinkingTraceType::Input,
                &prompt,
                Some(input_trace_id.clone()),
            )
            .await?;

        // Step 5: 调用大脑思考
        // 统一走 BrainDal.think() 入口，方便审计、统计、监控
        let brain = agent.brain.as_ref()
            .ok_or_else(|| AppError::Internal("Agent 大脑未唤醒，请先调用 wake_brain()".into()))?;
        
        let raw_output = self.brain_dal()
            .think(ctx.clone(), brain, &prompt)
            .await?;

        // Step 6: 记录输出 Trace（复用输入的 trace_id，形成关联对）
        let _output_trace = self
            .memory()
            .write_thinking_trace(
                ctx.clone(),
                &agent.po.id,
                ThinkingTraceType::Output,
                &raw_output,
                Some(input_trace_id.clone()),
            )
            .await?;

        // Step 7: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![input_trace_id.clone(), input_trace_id],
            raw_input: prompt,
            raw_output,
        })
    }
}
