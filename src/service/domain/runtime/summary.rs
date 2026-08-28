//! 总结退出流程
//!
//! `awaken_for_summary` 是 `RuntimeDomainImpl` 的私有辅助方法，
//! 不属于 `RuntimeAwakening` trait（只在 awaken 内部调用）。
//!
//! 触发时机：
//! - 思考轮次耗尽（MaxRoundsExceeded）时，让 Agent 总结当前工作进展
//! - 正常完成（Final）时，将本次工作对话总结为短期记忆

use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, messages_to_summary};
use crate::models::events::AgentLoopEvent;
use crate::models::memory::MemoryTrace;
use crate::pkg::paths;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{RuntimeDomain, RuntimeDomainImpl};
use common::enums::ThinkingScene;
use common::error::{Result, err};

use super::awakening::{
    build_scene_skills, build_scene_tool_descriptors, init_think_runtime_and_policy,
};
use super::types::config_resolve;
use super::types::{ThinkLoopResult, ThinkingOptions};

impl RuntimeDomainImpl {
    /// 总结退出流程
    ///
    /// 当思考轮次耗尽时，或正常完成时，让 Agent 总结当前工作进展并写入短期记忆。
    /// 内部构建 summary prompt，调用 think loop 让 Agent 自主完成总结，
    /// 可通过 send_message / update_task_progress 等工具发送通知（仅 MaxRoundsExceeded 场景）。
    ///
    /// `trace_ids` 为本次总结依赖的 trace 列表，写入 prompt 要求 Agent 调用
    /// save_short_term_memory 时填入此字段。
    ///
    /// 返回 Agent 的总结文本（作为 raw_output 记录到 trace）。
    pub(crate) async fn awaken_for_summary(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        work_messages: &[ChatMessage],
        options: &ThinkingOptions,
        parent_trace_id: &str,
        trace_ids: &[String],
    ) -> Result<String> {
        use common::enums::MemoryRole;

        let scene = ThinkingScene::Summary;

        // 1. 读取最近短期记忆
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, 20)
            .await?;

        // 2. 构造 summary trace
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            format!("summary-{}", parent_trace_id),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(),
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // 创建总结场景的思考运行时（覆盖 awaken 的，因为这是一个独立思考阶段）
        let (think_runtime, policy) =
            init_think_runtime_and_policy(agent, ThinkingScene::Summary, &trace_id);

        // 3. 发布循环启动事件
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "summary",
            None,
        ))
        .await;

        // 4. 按场景过滤技能
        let skill_pos = build_scene_skills(agent, scene);

        // 5. 构建 summary prompt
        let work_summary = messages_to_summary(work_messages, 500);
        let total_rounds = options.effective_max_rounds();

        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(agent);
        builder.skills(&skill_pos);
        let base = crate::config::get().base_data_path();
        let uid = ctx.uid();
        let uid_ref = if uid.is_empty() {
            None
        } else {
            Some(uid.as_str())
        };
        let default_workspace = paths::default_workspace(&base, uid_ref, Some(&agent.po.id))
            .to_string_lossy()
            .to_string();
        let user_home = if uid.is_empty() {
            paths::users_root_dir(&base).to_string_lossy().to_string()
        } else {
            paths::user_home(&base, &uid).to_string_lossy().to_string()
        };
        let user_shared_workspace = if uid.is_empty() {
            default_workspace.clone()
        } else {
            paths::user_shared_workspace(&base, &uid)
                .to_string_lossy()
                .to_string()
        };
        let user_agent_workspace = if uid.is_empty() {
            None
        } else {
            Some(
                paths::user_agent_workspace(&base, &uid, &agent.po.id)
                    .to_string_lossy()
                    .to_string(),
            )
        };
        let agent_workspace = Some(
            paths::agent_workspace(&base, &agent.po.id)
                .to_string_lossy()
                .to_string(),
        );
        let project_workspace = if let (Some(project), true) = (&options.project, !uid.is_empty()) {
            Some(
                paths::user_project_workspace(&base, &uid, &project.po.id)
                    .to_string_lossy()
                    .to_string(),
            )
        } else {
            None
        };
        builder.workspace_context(
            default_workspace,
            user_home,
            user_shared_workspace,
            user_agent_workspace,
            agent_workspace,
            project_workspace,
        );
        if let Some(project) = &options.project {
            builder.project_context(project);
        }
        if let Some(task) = &options.task {
            builder.task_context(task);
        }
        builder.history(&recent_memories);
        let prompt = builder.build_summary_prompt(&work_summary, total_rounds, trace_ids);
        // P0-b：角色拆分版初始消息；prompt 仍保留用于 trace.input 记录
        let initial_messages =
            builder.build_summary_initial_messages(&work_summary, total_rounds, trace_ids);

        // 6. 构建 Summary 场景的 ToolDescriptor（只允许消息和任务管理工具）
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        let tool_descriptors = build_scene_tool_descriptors(agent, scene);

        // 7. 调用 think loop（Summary 场景需要写短期记忆 + 可能发通知）
        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                initial_messages,
                &tool_descriptors,
                agent,
                ThinkingScene::Summary,
                &trace_id,
                config_resolve::summary_max_rounds(agent),
                0,
                config_resolve::think_timeout_secs(agent),
                Some(&think_runtime),
                Some(policy.as_ref()),
            )
            .await;

        let raw_output = match think_result {
            Ok(ThinkLoopResult::Final { content, .. }) => content,
            Ok(ThinkLoopResult::ContextOverflow { .. })
            | Ok(ThinkLoopResult::MaxRoundsExceeded { .. })
            | Ok(ThinkLoopResult::Cancelled { .. }) => {
                // 总结场景兜底：即使超限/轮次耗尽/被取消也返回已有内容
                String::new()
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "awaken_for_summary",
                    "summary think loop failed: {:?}",
                    e
                );
                String::new()
            }
        };

        // 8. 写入 trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // 补充运行时元数据
        trace.metadata.insert("scene".into(), "summary".into());
        trace
            .metadata
            .insert("parent_trace_id".into(), parent_trace_id.to_string());
        trace.metadata.insert(
            "depended_trace_ids".into(),
            serde_json::to_string(trace_ids).unwrap_or_default(),
        );
        if let Some(task_id) = ctx.task_id() {
            trace.metadata.insert("task_id".into(), task_id.clone());
        }
        if let Some(project_id) = ctx.project_id() {
            trace
                .metadata
                .insert("project_id".into(), project_id.clone());
        }

        let _ = self.memory().write_thinking_trace(ctx.clone(), trace).await;

        // 9. 发布循环完成事件
        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "summary",
            "success",
            0,
            None,
        ))
        .await;

        Ok(raw_output)
    }
}
