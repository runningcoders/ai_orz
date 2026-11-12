//! tests 单元测试（拆分自 inbox.rs）
//!
//! 文件瘦身：原 740 行 → 361 行，测试体 380 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::file::FileMeta;
use common::enums::{FileType, MessageRole, MessageStatus, MessageType};
use sqlx::types::Json;

fn anchor(
    from_role: MessageRole,
    project_id: Option<&str>,
    task_id: Option<&str>,
    thread_id: Option<&str>,
    message_type: MessageType,
) -> InboxAnchor {
    InboxAnchor {
        agent_id: "agent-1".to_string(),
        from_id: "sender-1".to_string(),
        from_role,
        project_id: project_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        thread_id: thread_id.map(str::to_string),
        message_type,
        created_at: 1000,
        id: "msg-anchor".to_string(),
        organization_id: Some("org-1".to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn message(
    id: &str,
    created_at: i64,
    from_id: &str,
    from_role: MessageRole,
    project_id: Option<&str>,
    task_id: Option<&str>,
    thread_id: Option<&str>,
    message_type: MessageType,
    file_type: Option<FileType>,
    content: &str,
) -> Message {
    Message::from_po(crate::models::message::MessagePo {
        id: id.to_string(),
        project_id: project_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        from_id: from_id.to_string(),
        to_id: "agent-1".to_string(),
        from_role,
        to_role: MessageRole::Agent,
        message_type,
        file_type,
        status: MessageStatus::Pending,
        content: content.to_string(),
        file_meta: Json(FileMeta::default()),
        reply_to_id: None,
        root_id: None,
        external_key: None,
        thread_id: thread_id.map(str::to_string),
        organization_id: Some("org-1".to_string()),
        created_by: "sender-1".to_string(),
        modified_by: "sender-1".to_string(),
        created_at,
        updated_at: created_at,
    })
}

fn default_cursor() -> InboxCursor {
    InboxCursor {
        created_at: 1000,
        id: "msg-anchor".to_string(),
    }
}

#[test]
fn option_equality_covers_all_contexts() {
    // 默认对话：project/task 皆 None，只吸收同样为 None 的消息
    let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
    assert!(accepts(
        &a,
        &message(
            "m1",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            "继续"
        )
    ));
    assert!(!accepts(
        &a,
        &message(
            "m2",
            1001,
            "sender-1",
            MessageRole::User,
            Some("p1"),
            None,
            None,
            MessageType::Text,
            None,
            "串味"
        )
    ));

    // 有项目、未挂任务：任务必须仍为 None
    let b = anchor(MessageRole::User, Some("p1"), None, None, MessageType::Text);
    assert!(accepts(
        &b,
        &message(
            "m3",
            1001,
            "sender-1",
            MessageRole::User,
            Some("p1"),
            None,
            None,
            MessageType::Text,
            None,
            "ok"
        )
    ));
    assert!(!accepts(
        &b,
        &message(
            "m4",
            1001,
            "sender-1",
            MessageRole::User,
            Some("p1"),
            Some("t1"),
            None,
            MessageType::Text,
            None,
            "串任务"
        )
    ));
}

#[test]
fn same_sender_any_role_can_merge() {
    for role in [MessageRole::User, MessageRole::Agent, MessageRole::System] {
        let a = anchor(role, None, None, None, MessageType::Text);
        let m = message(
            "m1",
            1001,
            "sender-1",
            role,
            None,
            None,
            None,
            MessageType::Text,
            None,
            "补充",
        );
        assert!(accepts(&a, &m), "同发送者同角色应允许合并: {role:?}");
    }
    // 发送角色不同即拒绝（即使 from_id 相同）
    let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
    let m = message(
        "m1",
        1001,
        "sender-1",
        MessageRole::Agent,
        None,
        None,
        None,
        MessageType::Text,
        None,
        "不同角色",
    );
    assert!(!accepts(&a, &m));
}

#[test]
fn thread_id_is_veto_only() {
    let a = anchor(
        MessageRole::User,
        None,
        None,
        Some("thread-a"),
        MessageType::Text,
    );
    let same = message(
        "m1",
        1001,
        "sender-1",
        MessageRole::User,
        None,
        None,
        Some("thread-a"),
        MessageType::Text,
        None,
        "同话题",
    );
    let other = message(
        "m2",
        1001,
        "sender-1",
        MessageRole::User,
        None,
        None,
        Some("thread-b"),
        MessageType::Text,
        None,
        "异话题",
    );
    let missing = message(
        "m3",
        1001,
        "sender-1",
        MessageRole::User,
        None,
        None,
        None,
        MessageType::Text,
        None,
        "无话题",
    );
    assert!(accepts(&a, &same));
    assert!(!accepts(&a, &other));
    assert!(accepts(&a, &missing), "任一端缺失不否决");
}

#[test]
fn whitelist_rejects_non_text_and_attachments() {
    let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
    let tool_call = message(
        "m1",
        1001,
        "sender-1",
        MessageRole::User,
        None,
        None,
        None,
        MessageType::ToolCallRequest,
        None,
        "tool",
    );
    let attachment = message(
        "m2",
        1001,
        "sender-1",
        MessageRole::User,
        None,
        None,
        None,
        MessageType::Text,
        Some(FileType::Image),
        "pic",
    );
    assert!(!accepts(&a, &tool_call));
    assert!(!accepts(&a, &attachment));
}

#[test]
fn cursor_uses_id_tie_break_and_limits() {
    let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
    let cursor = default_cursor();
    // 与游标同毫秒但 ID 更小 → 不算新消息
    let stale = message(
        "msg-0",
        1000,
        "sender-1",
        MessageRole::User,
        None,
        None,
        None,
        MessageType::Text,
        None,
        "旧",
    );
    assert!(!is_after_cursor(&cursor, &stale));
    // ID 更大 → 算新消息
    let fresh = message(
        "msg-z",
        1000,
        "sender-1",
        MessageRole::User,
        None,
        None,
        None,
        MessageType::Text,
        None,
        "新",
    );
    assert!(is_after_cursor(&cursor, &fresh));

    let rows: Vec<Message> = (0..7)
        .map(|i| {
            message(
                &format!("m{i}"),
                1001 + i,
                "sender-1",
                MessageRole::User,
                None,
                None,
                None,
                MessageType::Text,
                None,
                &"x".repeat(100),
            )
        })
        .collect();
    let batch = take_batch(&a, &cursor, rows, None);
    assert_eq!(batch.len(), MAX_INJECTED_MESSAGES);
    assert_eq!(batch.last().unwrap().id, "m4");
}

#[test]
fn token_budget_skips_oversized_without_blocking_later() {
    let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
    let cursor = default_cursor();
    let rows = vec![
        message(
            "m1",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            &"x".repeat(1500),
        ),
        // 单条已超总预算：应跳过，而不是让整批停在它这里
        message(
            "m2",
            1002,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            &"y".repeat(2500),
        ),
        message(
            "m3",
            1003,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            &"z".repeat(400),
        ),
    ];
    // 预算 500 token：m1 375 + m2 625 装不下 ⇒ 跳过 m2；m1 375 + m3 100 刚好装得下
    let batch = take_batch(&a, &cursor, rows, Some(500));
    let ids: Vec<&str> = batch.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["m1", "m3"],
        "超预算单条被跳过，后续短消息仍应吸收"
    );
    assert_eq!(
        batch
            .iter()
            .map(|i| estimate_tokens(&i.content))
            .sum::<usize>(),
        475
    );
}

/// 预算未配置 ⇒ 不裁（不猜兜底值）
#[test]
fn unknown_token_budget_keeps_every_message() {
    let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
    let cursor = default_cursor();
    let rows = vec![message(
        "big",
        1001,
        "sender-1",
        MessageRole::User,
        None,
        None,
        None,
        MessageType::Text,
        None,
        &"x".repeat(100_000),
    )];
    let batch = take_batch(&a, &cursor, rows, None);
    assert_eq!(batch.len(), 1, "无预算时不得因体量跳过");
}

/// 续跑标记只被 PrepareNode 消费一次，避免压缩回环误走追加路径
#[test]
fn resume_append_flag_is_one_shot() {
    let mut state = InboxState::new(anchor(
        MessageRole::User,
        None,
        None,
        None,
        MessageType::Text,
    ));
    assert!(!state.take_resume_append(), "默认不续跑");
    state.mark_resume_append();
    assert!(state.take_resume_append(), "置位后可取走");
    assert!(!state.take_resume_append(), "取走后立即失效");
}
