//! `inbox::collect` 循环分批的 DB 级回归测试
//!
//! 覆盖：一次唤醒处理 > 一批（10 条）的待处理消息、预算跨批累加、
//! 已判定 ID 排除、Pending 状态过滤、同上下文过滤。
//! 判定走 `brain=None` 的 fail-open 路径（全部放行并写缓存），
//! 因此本文件专注分批机制本身，不涉及小脑脚本。

use super::*;
use crate::models::file::FileMeta;
use crate::service::domain::runtime::inbox_relevance::InboxJudgeContext;
use sqlx::SqlitePool;

fn init(pool: SqlitePool) -> crate::pkg::RequestContext {
    let _ = crate::config::init();
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::dal::user::init();
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

fn anchor(id: &str) -> InboxAnchor {
    InboxAnchor {
        agent_id: "test-agent".to_string(),
        from_id: "test-user".to_string(),
        from_role: MessageRole::User,
        project_id: None,
        task_id: None,
        thread_id: None,
        message_type: MessageType::Text,
        created_at: 1000,
        id: id.to_string(),
        content: "帮我看一下登录页为什么打不开".to_string(),
        organization_id: None,
    }
}

fn make_msg(
    id: &str,
    created_at: i64,
    content: &str,
    project_id: Option<&str>,
    status: MessageStatus,
) -> Message {
    let mut msg = Message::new_with_context(
        id.to_string(),
        project_id.map(str::to_string),
        None,
        "test-user".to_string(),
        "test-agent".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        content.to_string(),
        None,
        FileMeta::default(),
        None,
        None,
        None,
        "test-user".to_string(),
    );
    msg.po.created_at = created_at;
    msg.po.updated_at = created_at;
    msg.po.status = status;
    msg
}

fn judge_ctx(agent_id: &str) -> InboxJudgeContext {
    InboxJudgeContext {
        agent_id: agent_id.to_string(),
        agent_name: "test-agent".to_string(),
        scene: "Awaken".to_string(),
        situation: Vec::new(),
    }
}

async fn save(ctx: &crate::pkg::RequestContext, msg: &Message) {
    crate::service::dal::message::dal()
        .save_message(ctx.clone(), msg)
        .await
        .expect("save message");
}

/// 25 条 Pending 跨 3 批（10 + 10 + 5）全部处理，不留到下一条唤醒
#[sqlx::test]
async fn collect_processes_all_batches(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    for i in 0..25 {
        save(
            &ctx,
            &make_msg(
                &format!("{tag}-m{i:02}"),
                1001 + i as i64,
                "补充内容",
                None,
                MessageStatus::Pending,
            ),
        )
        .await;
    }

    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &BTreeSet::new(),
        &judge_ctx(&a.agent_id),
        None,
        None,
        None,
    )
    .await
    .expect("collect");

    assert_eq!(outcome.injected.len(), 25, "25 条应跨 3 批全部吸收");
    assert_eq!(outcome.judged.len(), 25, "每条候选都只判定一次");
    assert_eq!(outcome.injected.first().unwrap().id, format!("{tag}-m00"));
    assert_eq!(outcome.injected.last().unwrap().id, format!("{tag}-m24"));
}

/// token 预算跨批累加：第一批用满部分、第二批只再放 2 条即触顶停止，
/// 不会有「每批各自一份预算」导致的超额吸收
#[sqlx::test]
async fn collect_budget_accumulates_across_batches(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    // 每条正文 40 个 ASCII 字符 ⇒ estimate_tokens = 10
    let content = "x".repeat(40);
    for i in 0..25 {
        save(
            &ctx,
            &make_msg(
                &format!("{tag}-m{i:02}"),
                1001 + i as i64,
                &content,
                None,
                MessageStatus::Pending,
            ),
        )
        .await;
    }

    // 预算 120 token ⇒ 恰好 12 条；第 1 批 10 条 + 第 2 批 2 条后触顶
    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &BTreeSet::new(),
        &judge_ctx(&a.agent_id),
        None,
        Some(120),
        None,
    )
    .await
    .expect("collect");

    assert_eq!(outcome.injected.len(), 12, "预算跨批累加，恰好放 12 条");
    assert_eq!(
        outcome.judged.len(),
        20,
        "两批各 10 条都已判定；预算触顶后不再拉第三批"
    );
}

/// 传入的 `judged_ids` 在首批 SQL 就被排除，不会重复判定/占位
#[sqlx::test]
async fn collect_excludes_judged_ids(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    for i in 0..15 {
        save(
            &ctx,
            &make_msg(
                &format!("{tag}-m{i:02}"),
                1001 + i as i64,
                "补充内容",
                None,
                MessageStatus::Pending,
            ),
        )
        .await;
    }
    let pre_judged: BTreeSet<String> = (0..10).map(|i| format!("{tag}-m{i:02}")).collect();

    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &pre_judged,
        &judge_ctx(&a.agent_id),
        None,
        None,
        None,
    )
    .await
    .expect("collect");

    assert_eq!(outcome.injected.len(), 5, "前 10 条已判定，只吸收后 5 条");
    assert_eq!(outcome.judged.len(), 5, "judged 只含本轮新判定的候选");
}

/// 每批都强制 `status = Pending` 与同上下文过滤：Processed 与异项目消息不进
#[sqlx::test]
async fn collect_filters_status_and_context(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    for i in 0..3 {
        save(
            &ctx,
            &make_msg(
                &format!("{tag}-m{i:02}"),
                1001 + i as i64,
                "补充内容",
                None,
                MessageStatus::Pending,
            ),
        )
        .await;
    }
    save(
        &ctx,
        &make_msg(
            &format!("{tag}-done"),
            1010,
            "补充内容",
            None,
            MessageStatus::Processed,
        ),
    )
    .await;
    save(
        &ctx,
        &make_msg(
            &format!("{tag}-other-project"),
            1011,
            "补充内容",
            Some("other-project"),
            MessageStatus::Pending,
        ),
    )
    .await;

    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &BTreeSet::new(),
        &judge_ctx(&a.agent_id),
        None,
        None,
        None,
    )
    .await
    .expect("collect");

    assert_eq!(
        outcome.injected.len(),
        3,
        "只有 Pending 且同上下文的 3 条进"
    );
    assert_eq!(outcome.judged.len(), 3, "被过滤的候选不参与判定");
}

/// 带协议附件的 Mixed 消息与 Text 锚点同上下文时应被收集，且携带附件引用
#[sqlx::test]
async fn collect_carries_attachment_refs_from_mixed_messages(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    let mut mixed = make_msg(
        &format!("{tag}-mixed"),
        1001,
        "再看 [这张图](attachment:att-1)",
        None,
        MessageStatus::Pending,
    );
    mixed.po.message_type = MessageType::Mixed;
    save(&ctx, &mixed).await;

    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &BTreeSet::new(),
        &judge_ctx(&a.agent_id),
        None,
        None,
        None,
    )
    .await
    .expect("collect");

    assert_eq!(outcome.injected.len(), 1);
    assert_eq!(
        outcome.injected[0].attachment_ids,
        vec!["att-1".to_string()]
    );
}

/// 非整除预算：预算 120、每条 11 token（44 字符）时，used 停在 110
/// 永远达不到 budget。旧退出条件 `used >= budget` 会失效、刷完整库；
/// 新判据用 `blocked` 标志，第二批遇到放不下的候选即停。
#[sqlx::test]
async fn collect_stops_when_next_item_exceeds_remaining_budget(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    // 44 个 ASCII 字符 ⇒ estimate_tokens = 11；120/11 非整除，余 10。
    let content = "x".repeat(44);
    for i in 0..25 {
        save(
            &ctx,
            &make_msg(
                &format!("{tag}-m{i:02}"),
                1001 + i as i64,
                &content,
                None,
                MessageStatus::Pending,
            ),
        )
        .await;
    }

    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &BTreeSet::new(),
        &judge_ctx(&a.agent_id),
        None,
        Some(120),
        None,
    )
    .await
    .expect("collect");

    // 预算 120 只放得下 10 条（10×11=110，第 11 条 121>120 被挡）。
    assert_eq!(
        outcome.injected.len(),
        10,
        "非整除预算按实际放得下的条数截断"
    );
    assert_eq!(
        outcome.used_tokens, 110,
        "返回本次实际用量（10×11），供外部做 剩余 -= 用量"
    );
    // 第二批仍有候选被挡 ⇒ 立即停，不刷完 25 条。
    assert!(
        outcome.judged.len() < 25,
        "预算触顶后不得继续拉库判完所有候选（实际 {}）",
        outcome.judged.len()
    );
}

/// 剩余预算语义：模拟 poll_front 已用掉 110，外部把剩余 10 传进来；
/// collect 只认传入的剩余值，放不下任何一条，`used_tokens` 返回本次实际的 0
/// （而非累计 110）。外部据此扣减后剩余仍为 10，不会重领一份 5%。
#[sqlx::test]
async fn collect_honours_reduced_remaining_budget_from_earlier_poll(pool: SqlitePool) {
    let ctx = init(pool);
    let tag = uuid::Uuid::now_v7().to_string();
    let a = anchor(&format!("anchor-{tag}"));
    let cursor = InboxCursor {
        created_at: a.created_at,
        id: a.id.clone(),
    };
    let content = "x".repeat(44);
    for i in 0..5 {
        save(
            &ctx,
            &make_msg(
                &format!("{tag}-m{i:02}"),
                1001 + i as i64,
                &content,
                None,
                MessageStatus::Pending,
            ),
        )
        .await;
    }

    // 外部总账已用 110、总预算 120 ⇒ 本次传入剩余 10。
    let outcome = collect(
        &ctx,
        &a,
        &cursor,
        &BTreeSet::new(),
        &judge_ctx(&a.agent_id),
        None,
        Some(10),
        None,
    )
    .await
    .expect("collect");

    assert!(
        outcome.injected.is_empty(),
        "剩余 10 放不下 11 token 的一条"
    );
    assert_eq!(
        outcome.used_tokens, 0,
        "返回本次实际用量（0），不是累计用量（110）"
    );
}
