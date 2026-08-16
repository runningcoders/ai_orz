use ai_orz_tools::collect_target_files;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let apply = std::env::args().any(|a| a == "--apply");

    let replacements: Vec<(&str, &str)> = vec![
        // === DESIGN ARCHIVE (37 files) ===
        ("docs/design/a2a_server_architecture_design.md", "docs/archive/design-archive/a2a_server_architecture_design.md"),
        ("docs/design/agent_loop_engine_design.md", "docs/archive/design-archive/agent_loop_engine_design.md"),
        ("docs/design/agent_onboarding_design.md", "docs/archive/design-archive/agent_onboarding_design.md"),
        ("docs/design/attachment_storage.md", "docs/archive/design-archive/attachment_storage.md"),
        ("docs/design/browser_e2e_test_design.md", "docs/archive/design-archive/browser_e2e_test_design.md"),
        ("docs/design/builtins_http_tool_design.md", "docs/archive/design-archive/builtins_http_tool_design.md"),
        ("docs/design/canvas_rendering_playbook.md", "docs/archive/design-archive/canvas_rendering_playbook.md"),
        ("docs/design/common-error-type.md", "docs/archive/design-archive/common-error-type.md"),
        ("docs/design/consumer_architecture.md", "docs/archive/design-archive/consumer_architecture.md"),
        ("docs/design/entity_list_query_search_design.md", "docs/archive/design-archive/entity_list_query_search_design.md"),
        ("docs/design/event_design.md", "docs/archive/design-archive/event_design.md"),
        ("docs/design/external_agent_design.md", "docs/archive/design-archive/external_agent_design.md"),
        ("docs/design/full_entity_fts5_search_design.md", "docs/archive/design-archive/full_entity_fts5_search_design.md"),
        ("docs/design/generic_builtin_tools_design.md", "docs/archive/design-archive/generic_builtin_tools_design.md"),
        ("docs/design/handler-tool-registration-macro.md", "docs/archive/design-archive/handler-tool-registration-macro.md"),
        ("docs/design/intent_aware_two_stage_awaken_design.md", "docs/archive/design-archive/intent_aware_two_stage_awaken_design.md"),
        ("docs/design/lark_cli_integration.md", "docs/archive/design-archive/lark_cli_integration.md"),
        ("docs/design/mcp_tool_design.md", "docs/archive/design-archive/mcp_tool_design.md"),
        ("docs/design/memory_search_enhancement_design.md", "docs/archive/design-archive/memory_search_enhancement_design.md"),
        ("docs/design/memory_system_enhancement_design.md", "docs/archive/design-archive/memory_system_enhancement_design.md"),
        ("docs/design/message_channel_design.md", "docs/archive/design-archive/message_channel_design.md"),
        ("docs/design/message_interaction_design.md", "docs/archive/design-archive/message_interaction_design.md"),
        ("docs/design/organization_design.md", "docs/archive/design-archive/organization_design.md"),
        ("docs/design/project_design.md", "docs/archive/design-archive/project_design.md"),
        ("docs/design/project_management_design.md", "docs/archive/design-archive/project_management_design.md"),
        ("docs/design/request_context_design.md", "docs/archive/design-archive/request_context_design.md"),
        ("docs/design/seed-config-migration.md", "docs/archive/design-archive/seed-config-migration.md"),
        ("docs/design/skill_design.md", "docs/archive/design-archive/skill_design.md"),
        ("docs/design/skill_system_enhancement_design.md", "docs/archive/design-archive/skill_system_enhancement_design.md"),
        ("docs/design/stats_module_design.md", "docs/archive/design-archive/stats_module_design.md"),
        ("docs/design/stats_query_design.md", "docs/archive/design-archive/stats_query_design.md"),
        ("docs/design/task_design.md", "docs/archive/design-archive/task_design.md"),
        ("docs/design/task_scheduler_design.md", "docs/archive/design-archive/task_scheduler_design.md"),
        ("docs/design/testing_guidelines.md", "docs/archive/design-archive/testing_guidelines.md"),
        ("docs/design/tool_design.md", "docs/archive/design-archive/tool_design.md"),
        ("docs/design/unified-idl-http-handler.md", "docs/archive/design-archive/unified-idl-http-handler.md"),
        ("docs/design/vector_search_architecture.md", "docs/archive/design-archive/vector_search_architecture.md"),

        // === PLAN ARCHIVE ===
        ("docs/plan/2026-08-15-文档规范与仓库精简.md", "docs/archive/plan-archive/文档规范与仓库精简.md"),
        ("docs/plan/Agent 领域能力扩展工程.md", "docs/archive/plan-archive/Agent 领域能力扩展工程.md"),
        ("docs/plan/Domain 层统一错误处理重构.md", "docs/archive/plan-archive/Domain 层统一错误处理重构.md"),
        ("docs/plan/Playwright E2E 测试工程.md", "docs/archive/plan-archive/Playwright E2E 测试工程.md"),
        ("docs/plan/init_base_data 扩展点全域覆盖.md", "docs/archive/plan-archive/init_base_data 扩展点全域覆盖.md"),
        ("docs/plan/runtime_refactor_execution_draft.md", "docs/archive/plan-archive/runtime_refactor_execution_draft.md"),
        ("docs/plan/sqlx_repository_layer_rewrite.md", "docs/archive/plan-archive/sqlx_repository_layer_rewrite.md"),
        ("docs/plan/token_io_assets.md", "docs/archive/plan-archive/token_io_assets.md"),
        ("docs/plan/两阶段启动幂等执行.md", "docs/archive/plan-archive/两阶段启动幂等执行.md"),
        ("docs/plan/身份凭证Domain统一CRUD重构.md", "docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md"),
        ("docs/plan/任务模型标准化重构.md", "docs/archive/plan-archive/任务模型标准化重构.md"),
        ("docs/plan/消息交互与 SSE 双向系统重构.md", "docs/archive/plan-archive/消息交互与 SSE 双向系统重构.md"),
        ("docs/plan/飞书P2P消息集成.md", "docs/archive/plan-archive/飞书P2P消息集成.md"),
        ("docs/plan/统计图表第三期.md", "docs/archive/plan-archive/统计图表第三期.md"),
        ("docs/plan/统计图表Phase2.md", "docs/archive/plan-archive/统计图表Phase2.md"),
        ("docs/plan/知识图谱推荐起点与组件复用重构.md", "docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md"),
        ("docs/plan/architecture_status_20260725.md", "docs/archive/plan-archive/architecture_status_20260725.md"),

        // === STRAYS ===
        ("docs/archive/a2a_server_design.md", "docs/archive/design-archive/a2a_server_design.md"),
        ("docs/archive/runtime-domain-roadmap.md", "docs/archive/design-archive/runtime-domain-roadmap.md"),
        ("docs/archive/handler_management_api_plan.md", "docs/archive/plan-archive/handler_management_api_plan.md"),
        ("docs/archive/test_supplement_plan_20260514.md", "docs/archive/plan-archive/test_supplement_plan_20260514.md"),
        ("docs/archive/frontend_roadmap.md", "docs/archive/plan-archive/frontend_roadmap.md"),
        ("docs/archive/todo-archive-2026-08-15.md", "docs/archive/plan-archive/todo-archive-2026-08-15.md"),
    ];

    let files = collect_target_files();
    let mut total: usize = 0;
    let mut touched: usize = 0;
    let mut sample_files: Vec<String> = Vec::new();

    for f in &files {
        let Ok(content) = fs::read_to_string(f) else {
            continue;
        };
        let mut new_content = content;
        let mut file_replacements: usize = 0;

        for (old, new) in &replacements {
            let count = new_content.matches(old).count();
            if count > 0 {
                new_content = new_content.replace(old, new);
                file_replacements += count;
            }
        }

        if file_replacements > 0 {
            touched += 1;
            total += file_replacements;
            let rel = f.to_string_lossy().to_string();
            if sample_files.len() < 30 {
                sample_files.push(format!("  {} ({} replacements)", rel, file_replacements));
            }
            if apply {
                let _ = fs::write(f, &new_content);
            }
        }
    }

    let mode = if apply { "APPLY" } else { "DRY-RUN" };
    println!("\n=== {} MODE ===", mode);
    println!("Files modified: {}", touched);
    println!("Total string replacements: {}", total);
    println!("\nSample files (first 30):");
    for s in &sample_files {
        println!("{}", s);
    }
    if sample_files.len() < touched {
        println!("  ... and {} more", touched - sample_files.len());
    }

    ExitCode::SUCCESS
}