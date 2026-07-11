//! Runtime Awakening 具体实现

use common::error::{err, bail_err, Result};
use crate::models::agent::Agent;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::AgentAwakeEvent;
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};

use super::context_assembly::PromptBuilder;

use crate::enrich_ctx;
use crate::record_event;

#[async_trait::async_trait]
impl RuntimeAwakening for RuntimeDomainImpl {
    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
    ) -> Result<AwakeningResult> {
        let start_time = std::time::SystemTime::now();

        // 设置 Agent 为忙碌状态
        AgentRuntimeStateManager::global()
            .set_busy(&agent.po.id, &message.po.id);

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

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

        // Step 3.5: 加载内置工具（神经工具 + 已安装工具包）
        let builtin_tools = self.load_builtin_tools(ctx.clone(), agent).await?;
        let builtin_tool_prompts: Vec<String> = builtin_tools
            .iter()
            .map(|tool| tool.po.to_tool_prompt())
            .collect();

        // Step 4: 拼装 Prompt（注入 trace_id 到头部，模型可见）
        let builder = PromptBuilder::new()
            .current_trace_id(&trace_id)
            .trace_ids(&trace_ids)
            .agent_system(agent)
            .agent_tools(agent)
            .tools(&builtin_tool_prompts)
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
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        // 调用 think，先捕获结果（不立即 ?）
        let think_result = self.brain_dal().think(ctx.clone(), brain, &prompt).await;

        // 无论成功失败，最后都设置为 Idle
        AgentRuntimeStateManager::global()
            .set_idle(&agent.po.id);

        // 展开 Result，失败时也记录事件
        let raw_output = match think_result {
            Ok(output) => output,
            Err(e) => {
                // 记录唤醒失败事件
                let duration_ms = start_time.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
                let _ = record_event!(ctx, AgentAwakeEvent {
                    agent_id: agent.po.id.clone(),
                    project_id: ctx.project_id().cloned(),
                    task_id: ctx.task_id().cloned(),
                    organization_id: ctx.organization_id.clone(),
                    user_id: Some(ctx.uid()),
                    message_id: Some(message.po.id.clone()),
                    call_count: 1,
                    duration_ms: duration_ms,
                    status: format!("failed: {}", e),
                });
                return Err(e);
            }
        };

        // Step 6: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // Step 7: 通过 RuntimeMemory 子模块写入
        // 架构：awakening → RuntimeMemory → MemoryDal → MemoryDao
        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 8: 记录 Agent 唤醒统计事件
        let duration_ms = start_time.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
        let _ = record_event!(ctx, AgentAwakeEvent {
            agent_id: agent.po.id.clone(),
            project_id: ctx.project_id().cloned(),
            task_id: ctx.task_id().cloned(),
            organization_id: ctx.organization_id.clone(),
            user_id: Some(ctx.uid()),
            message_id: Some(message.po.id.clone()),
            call_count: 1,
            duration_ms: duration_ms,
            status: "success".to_string(),
        });

        // Step 9: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id],
            raw_input: prompt,
            raw_output,
        })
    }
}

impl RuntimeDomainImpl {
    /// 加载所有内置工具（神经工具 + 已安装工具包工具）
    ///
    /// 内置工具是 Agent 天生具备或入职培训获得的能力，不需要显式绑定：
    /// - 神经工具：tags 包含 "neural"，所有 Agent 天生拥有
    /// - 已安装工具包工具：tags 与 agent.installed_tags 有交集
    async fn load_builtin_tools(
        &self,
        ctx: RequestContext,
        agent: &Agent,
    ) -> Result<Vec<crate::models::tool::Tool>> {
        use crate::service::dao::tool::ToolQuery;
        use common::enums::ToolStatus;

        let all_tools = self
            .tool_dal
            .query(
                ctx,
                ToolQuery {
                    enabled_only: Some(true),
                    status: Some(ToolStatus::Enabled),
                    ..Default::default()
                },
            )
            .await?;

        Ok(filter_builtin_tools(all_tools, &agent.po.get_installed_tags()))
    }
}

/// 从全部工具中筛选内置工具（神经工具 + 已安装工具包工具）
///
/// 筛选规则：
/// 1. 神经工具：tags 包含 "neural"，所有 Agent 天生拥有
/// 2. 已安装工具包工具：tags 与 installed_tags 有交集
fn filter_builtin_tools(
    all_tools: Vec<crate::models::tool::Tool>,
    installed_tags: &[String],
) -> Vec<crate::models::tool::Tool> {
    all_tools
        .into_iter()
        .filter(|tool| {
            let tags = tool.po.get_tags();
            if tags.contains(&"neural".to_string()) {
                return true;
            }
            installed_tags.iter().any(|installed| tags.contains(installed))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_builtin_tools;
    use crate::models::tool::{Tool, ToolPo};
    use common::enums::ToolProtocol;
    use serde_json::json;

    fn make_tool(tool_id: &str, tags: Vec<&str>) -> Tool {
        let po = ToolPo::new(
            tool_id.to_string(),
            tool_id.to_string(),
            "test tool".to_string(),
            ToolProtocol::Builtin,
            json!({}),
            Some(json!({ "type": "object" })),
            tags.into_iter().map(String::from).collect(),
            Some("test-user".to_string()),
        );
        Tool::from_po_for_management(po)
    }

    fn make_tags(tags: Vec<&str>) -> Vec<String> {
        tags.into_iter().map(String::from).collect()
    }

    #[test]
    fn filter_includes_neural_tools_regardless_of_installed_tags() {
        let tools = vec![
            make_tool("neural-1", vec!["neural"]),
            make_tool("external-1", vec!["external"]),
        ];
        let installed = make_tags(vec![]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].po.id, "neural-1");
    }

    #[test]
    fn filter_includes_installed_tool_pack_tools() {
        let tools = vec![
            make_tool("neural-1", vec!["neural"]),
            make_tool("pm-1", vec!["project_management"]),
            make_tool("da-1", vec!["data_analysis"]),
            make_tool("external-1", vec!["external"]),
        ];
        let installed = make_tags(vec!["project_management"]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 2);
        let ids: Vec<&str> = result.iter().map(|t| t.po.id.as_str()).collect();
        assert!(ids.contains(&"neural-1"));
        assert!(ids.contains(&"pm-1"));
    }

    #[test]
    fn filter_excludes_tools_when_no_tags_installed() {
        let tools = vec![
            make_tool("neural-1", vec!["neural"]),
            make_tool("pm-1", vec!["project_management"]),
        ];
        let installed = make_tags(vec![]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].po.id, "neural-1");
    }

    #[test]
    fn filter_includes_tool_with_multiple_tags_when_one_matches() {
        let tools = vec![
            // Tool has both "project_management" and "advanced" tags
            make_tool("multi-1", vec!["project_management", "advanced"]),
        ];
        let installed = make_tags(vec!["project_management"]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].po.id, "multi-1");
    }
}
