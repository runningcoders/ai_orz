//! DAL 混合搜索标注/分页的单测。
//! 用「向量依赖全关」的 DAL（new()）走纯关键词路，隔离验证：
//! - 短词 LIKE 兜底命中必须被标注为 Keyword（rank=NULL 不影响标注）；
//! - total 是去重后的完整命中数，分页 offset 在其上切片（不能被召回上限截断）。

use crate::models::attachment::{AttachmentDescriptionUpdate, AttachmentPo};
use crate::models::vector::MatchType;
use crate::pkg::request_context_test_support::new_test_ctx;
use crate::service::dal::attachment::new;
use crate::service::dao::attachment::{AttachmentSearch, new_with_attachments_dir};
use common::api::PaginationParams;
use common::enums::FileType;
use sqlx::SqlitePool;

fn po_with_summary(id: &str) -> AttachmentPo {
    AttachmentPo::new(
        id.to_string(),
        format!("{id}.md"),
        format!("{id}.md"),
        format!("20260617/{id}.md"),
        "text/markdown".to_string(),
        FileType::Document,
        12,
        "message".to_string(),
        "test-user".to_string(),
        "test-user".to_string(),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn like_hit_is_labeled_keyword_and_pagination_is_consistent(pool: SqlitePool) {
    let temp_dir = tempfile::tempdir().unwrap();
    let dao = new_with_attachments_dir(temp_dir.path().join("attachments"));
    let dal = new(dao.clone()); // 向量依赖全 None → 纯关键词路
    let ctx = new_test_ctx("test-user", pool);

    // 两条附件，摘要都含「季度营收」（4 字符走 MATCH；「营收」2 字符走 LIKE 兜底）。
    for i in 0..2 {
        let po = po_with_summary(&format!("att-{i}"));
        dao.insert(ctx.clone(), &po).await.unwrap();
        dao.update_description(
            ctx.clone(),
            &po.id,
            &AttachmentDescriptionUpdate {
                description: "季度营收分析报告，含各区域明细".to_string(),
                usage: "供管理层决策参考".to_string(),
                source: "agent_tool".to_string(),
                model: "m".to_string(),
                message_id: "msg".to_string(),
                described_at: 1,
            },
        )
        .await
        .unwrap();
    }

    // 短词「营收」：DAO 走 LIKE 兜底，DAL 标注必须为 Keyword（不是 None）。
    let page = dal
        .search(
            ctx.clone(),
            AttachmentSearch {
                keyword: Some("营收".to_string()),
                root_user_id: Some("test-user".to_string()),
                purpose: None,
                file_type: None,
                pagination: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 2, "total 应为去重后的完整命中数");
    assert_eq!(page.items.len(), 2);
    assert!(
        page.items
            .iter()
            .all(|a| a.search_match.as_ref().map(|m| m.match_type) == Some(MatchType::Keyword)),
        "LIKE 兜底命中必须标注为 Keyword（rank=NULL 不应丢失标注）"
    );

    // 分页：offset=1 / limit=1 应取到第二页 1 条，total 仍是 2。
    let page2 = dal
        .search(
            ctx.clone(),
            AttachmentSearch {
                keyword: Some("营收".to_string()),
                root_user_id: Some("test-user".to_string()),
                purpose: None,
                file_type: None,
                pagination: PaginationParams {
                    offset: Some(1),
                    limit: Some(1),
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(page2.total, 2, "分页不应改变 total");
    assert_eq!(page2.items.len(), 1, "第二页应取到 1 条");
}
