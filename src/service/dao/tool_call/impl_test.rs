//! ToolCallDao::execute 单元测试
//!
//! 覆盖 call_id 单一事实源机制：
//! - 自动生成 call_id 注入 ctx，日志文件名/进程条目/返回 JSON 全链路同值
//! - 业务指定 call_id 的幂等防重（Completed 直接返回 / Failed 允许重试）

use crate::models::tool::Tool;
use crate::pkg::request_context::RequestContext;
use crate::pkg::request_context_test_support::{
    ensure_test_base_data_path, ensure_test_tool_call_logger, new_test_ctx,
};
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::shell_exec::ShellExecToolFactory;
use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use crate::service::dao::tool_call::new;

fn make_shell_tool() -> Tool {
    let factory = ShellExecToolFactory;
    let po = factory.create_po();
    let our_tool = factory.create(po.clone());
    Tool {
        po,
        our_tool,
        search_match: None,
        stats: None,
    }
}

fn base_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    new_test_ctx("test-user", pool)
}

fn setup() {
    ensure_test_base_data_path();
    // shell_exec 内部依赖 config::get().base_data_path()
    let _ = crate::config::init();
    ensure_test_tool_call_logger();
}

#[cfg(unix)]
#[tokio::test]
async fn test_execute_injects_call_id_and_links_full_chain() {
    setup();
    let dao = new();
    let tool = make_shell_tool();
    let args = serde_json::json!({"command": "echo call-id-chain"});

    let (output, entry) = dao.execute(base_ctx(), &tool, args).await.unwrap();

    // 返回 JSON 的 call_id 与 entry 一致
    assert_eq!(
        output.get("call_id").and_then(|v| v.as_str()),
        Some(entry.call_id.as_str())
    );

    // 日志文件名主干 == call_id（按天分区目录 YYYYMMDD 下）
    let day_dir = chrono::Local::now().format("%Y%m%d").to_string();
    let log_path = ensure_test_base_data_path()
        .join("tools")
        .join("shell_exec")
        .join("logs")
        .join(&day_dir)
        .join(format!("{}.log", entry.call_id));
    assert!(log_path.exists(), "log file should be named by call_id");

    // 进程注册中心条目的 call_id 与 pid 关联一致
    let pid = output.get("pid").and_then(|v| v.as_u64()).unwrap() as u32;
    let proc_entry = crate::pkg::process::registry()
        .get(pid)
        .expect("process should be registered");
    assert_eq!(proc_entry.call_id, entry.call_id);

    crate::pkg::process::registry().remove(pid);
}

#[cfg(unix)]
#[tokio::test]
async fn test_execute_dedup_returns_history_for_completed() {
    setup();
    let dao = new();
    let tool = make_shell_tool();
    let args = serde_json::json!({"command": "echo dedup-ok"});
    let call_id = format!("dedup-{}", uuid::Uuid::now_v7());

    // 首次执行（业务指定 call_id）
    let ctx = base_ctx()
        .to_builder()
        .tool_call_id(call_id.clone())
        .build();
    let (output1, entry1) = dao.execute(ctx, &tool, args.clone()).await.unwrap();
    assert_eq!(entry1.call_id, call_id);
    let registered_count = crate::pkg::process::registry()
        .list()
        .iter()
        .filter(|e| e.call_id == call_id)
        .count();
    assert_eq!(registered_count, 1);

    // 模拟消费者落盘 JSONL
    ToolCallLogger::get()
        .log_call("shell_exec", entry1.clone())
        .unwrap();

    // 同 call_id 再次调用 → 直接返回历史结果，不重复执行
    let ctx2 = base_ctx()
        .to_builder()
        .tool_call_id(call_id.clone())
        .build();
    let (output2, entry2) = dao.execute(ctx2, &tool, args).await.unwrap();
    assert_eq!(entry2.call_id, call_id);
    assert_eq!(
        entry2.metadata.get("deduplicated"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(output2, output1);

    // 未重复执行：注册中心仍只有一条该 call_id 的进程记录
    let registered_count = crate::pkg::process::registry()
        .list()
        .iter()
        .filter(|e| e.call_id == call_id)
        .count();
    assert_eq!(registered_count, 1);

    if let Some(pid) = output1.get("pid").and_then(|v| v.as_u64()) {
        crate::pkg::process::registry().remove(pid as u32);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn test_execute_dedup_allows_retry_after_failed() {
    setup();
    let dao = new();
    let tool = make_shell_tool();
    let call_id = format!("retry-{}", uuid::Uuid::now_v7());

    // 手工写入一条 Failed 历史
    let failed_entry = ToolCallEntry {
        call_id: call_id.clone(),
        tool_id: "shell_exec".to_string(),
        tool_name: "shell_exec".to_string(),
        agent_id: None,
        task_id: None,
        project_id: None,
        started_at: 0,
        finished_at: 0,
        duration_ms: 0,
        input: serde_json::json!({}),
        output: None,
        error: Some("previous failure".to_string()),
        status: ToolCallStatus::Failed,
        metadata: serde_json::json!({}),
    };
    ToolCallLogger::get()
        .log_call("shell_exec", failed_entry)
        .unwrap();

    // 同 call_id 调用 → 失败历史不阻止重试，正常执行
    let ctx = base_ctx()
        .to_builder()
        .tool_call_id(call_id.clone())
        .build();
    let args = serde_json::json!({"command": "echo retry-ok"});
    let (output, entry) = dao.execute(ctx, &tool, args).await.unwrap();
    assert_eq!(entry.call_id, call_id);
    assert_eq!(entry.status, ToolCallStatus::Completed);
    assert!(entry.metadata.get("deduplicated").is_none());
    assert!(
        output
            .get("output")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.contains("retry-ok"))
    );

    if let Some(pid) = output.get("pid").and_then(|v| v.as_u64()) {
        crate::pkg::process::registry().remove(pid as u32);
    }
}
