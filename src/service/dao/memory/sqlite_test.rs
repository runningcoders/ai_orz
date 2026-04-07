//! Memory DAO 单元测试
//!
//! 测试内存记忆系统的基本功能：
//! - 追加记忆追踪
//! - 查询短期索引
//! - 全文检索
//! - 知识节点增删查改

use super::*;
use crate::models::memory::{MemoryRole, MemoryTrace, ShortTermMemoryIndexPo};
use crate::pkg::constants::RequestContext;
use crate::service::dao::memory::dao;
use std::collections::HashMap;

#[tokio::test]
async fn test_append_memory_trace() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    // 创建一条记忆
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        MemoryRole::User,
        "你好，今天我们来测试记忆系统".to_string(),
    ).with_metadata("session".to_string(), "test-session".to_string());

    // 追加
    let result = dao().append_memory_trace(
        ctx,
        &trace,
        "用户打招呼，测试记忆系统".to_string(),
        vec!["test".to_string(), "greeting".to_string()],
    );

    // 应该成功
    assert!(result.is_ok());

    let index = result.unwrap();
    assert_eq!(index.id, trace.id);
    assert_eq!(index.agent_id, "test-agent-1");
}

#[tokio::test]
async fn test_get_short_term_index() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    // 创建一条记忆
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        MemoryRole::Assistant,
        "你好！我已经收到你的消息，记忆系统工作正常。".to_string(),
    );

    let result = dao().append_memory_trace(
        ctx.clone(),
        &trace,
        "助手回复，确认记忆系统工作正常".to_string(),
        vec!["test".to_string(), "reply".to_string()],
    );

    assert!(result.is_ok());
    let index = result.unwrap();

    // 查询
    let get_result = dao().get_short_term_index(ctx, &index.id);
    assert!(get_result.is_ok());

    let get_index = get_result.unwrap();
    assert!(get_index.is_some());
    let get_index = get_index.unwrap();

    assert_eq!(get_index.id, index.id);
    assert_eq!(get_index.agent_id, "test-agent-1");
}

#[tokio::test]
async fn test_list_short_term_by_agent() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    // 创建多条记忆
    for i in 0..5 {
        let trace = MemoryTrace::new(
            "test-agent-2".to_string(),
            if i % 2 == 0 { MemoryRole::User } else { MemoryRole::Assistant },
            format!("这是第 {} 条测试记忆", i),
        );

        let result = dao().append_memory_trace(
            ctx.clone(),
            &trace,
            format!("第 {} 条测试记忆", i),
            vec!["test".to_string(), "list".to_string()],
        );

        assert!(result.is_ok());
    }

    // 查询列表
    let result = dao().list_short_term_by_agent(ctx, "test-agent-2", 10);
    assert!(result.is_ok());

    let list = result.unwrap();
    assert_eq!(list.len(), 5);
    // 按时间倒序，最新的在前面
    assert!(list[0].created_at >= list[list.len() - 1].created_at);
}

#[tokio::test]
async fn test_search_short_term() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    // 创建两条不同内容的记忆
    let trace1 = MemoryTrace::new(
        "test-agent-3".to_string(),
        MemoryRole::User,
        "我想了解一下关于 Rust 内存安全的信息".to_string(),
    );
    dao().append_memory_trace(
        ctx.clone(),
        &trace1,
        "用户询问 Rust 内存安全相关信息".to_string(),
        vec!["rust".to_string(), "memory".to_string()],
    ).unwrap();

    let trace2 = MemoryTrace::new(
        "test-agent-3".to_string(),
        MemoryRole::User,
        "Python 有什么优点".to_string(),
    );
    dao().append_memory_trace(
        ctx.clone(),
        &trace2,
        "用户询问 Python 优点".to_string(),
        vec!["python".to_string()],
    ).unwrap();

    // 搜索 Rust
    let result = dao().search_short_term(ctx, "test-agent-3", "Rust", 10);
    assert!(result.is_ok());

    let list = result.unwrap();
    // 应该只有一条匹配
    assert_eq!(list.len(), 1);
    assert!(list[0].summary.contains("Rust"));
}

#[tokio::test]
async fn test_read_memory_content() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    // 创建记忆
    let content = "这是一段测试内容，用来测试从文件读取".to_string();
    let trace = MemoryTrace::new(
        "test-agent-4".to_string(),
        MemoryRole::User,
        content.clone(),
    );

    let result = dao().append_memory_trace(
        ctx.clone(),
        &trace,
        "测试内容读取".to_string(),
        vec!["test".to_string(), "read".to_string()],
    );

    assert!(result.is_ok());
    let index = result.unwrap();

    // 读取内容
    let content_read = dao().read_memory_content(&index);
    assert!(content_read.is_ok());
    let content_read = content_read.unwrap();

    // 内容应该匹配
    assert!(content_read.contains(&content));
}
