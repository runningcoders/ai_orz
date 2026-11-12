//! 技能混合搜索与向量路接线护栏

use super::search::vector_search_top_k;
use super::test_support::{create_test_skill_po, init_test, new_ctx};
use crate::models::skill::SkillPo;
use crate::models::vector::MatchType;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use common::enums::skill::SkillAuthorType;
use common::error::Result;
use sqlx::SqlitePool;

/// 测试搜索技能
#[sqlx::test]
async fn test_search_skill(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", skill_id);
    let po = SkillPo::new(
        skill_id,
        "debug-helper".to_string(),
        "Helps with debugging AI agent code".to_string(),
        vec!["AI Agent".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 搜索：按名称匹配
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].po.name, "debug-helper");

    // 搜索：按描述匹配
    let results2 = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debugging".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results2.items.len(), 1);

    // 搜索：无匹配
    let results3 = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("nonexistent-keyword".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results3.items.len(), 0);

    Ok(())
}

/// 测试 DAL search 方法的三态匹配（Hybrid / Vector / Keyword）
///
/// MockCortexDao 的向量生成策略：
/// - 文本含 "nonexistent" → 向量 [1.0, 0.0, 0.0]
/// - 其他文本 → 向量 [0.0, 1.0, 1.0]
///
/// 场景设计：
/// - skill_matching：name 含 "debug"，向量 [0.0, 1.0, 1.0]
/// - skill_vector_only：name 不含 "debug"，向量 [0.0, 1.0, 1.0]
/// - 搜索关键词 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
/// - skill_matching：FTS5 命中 + 向量距离 0.0 → Hybrid
/// - skill_vector_only：FTS5 未命中 + 向量距离 0.0 → Vector
#[sqlx::test]
async fn test_search_three_state_matching(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 1. 创建 name 含 "debug" 的技能（会同时被 FTS5 和向量命中 → Hybrid）
    let skill_id_matching = uuid::Uuid::now_v7().to_string();
    let po_matching = SkillPo::new(
        skill_id_matching.clone(),
        "debug-helper".to_string(),
        "Helps with debugging".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id_matching),
    );
    skill_dal.create(ctx.clone(), &po_matching).await?;

    // 2. 创建 name 不含 "debug" 的技能（只被向量命中 → Vector）
    let skill_id_vector_only = uuid::Uuid::now_v7().to_string();
    let po_vector_only = SkillPo::new(
        skill_id_vector_only.clone(),
        "python-tool".to_string(),
        "A python utility".to_string(),
        vec!["python".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id_vector_only),
    );
    skill_dal.create(ctx.clone(), &po_vector_only).await?;

    // 3. 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    // 应返回 2 条结果（Hybrid + Vector）
    assert_eq!(results.items.len(), 2, "应返回 Hybrid + Vector 共 2 条结果");

    // 第一条应是 Hybrid（优先级最高）
    assert_eq!(results.items[0].po.id, skill_id_matching);
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid,
        "skill_matching 应是 Hybrid 匹配"
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_some()
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .fts_rank
            .is_some()
    );

    // 第二条应是 Vector（仅向量命中）
    assert_eq!(results.items[1].po.id, skill_id_vector_only);
    assert_eq!(
        results.items[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector,
        "skill_vector_only 应是 Vector 匹配"
    );
    assert!(
        results.items[1]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_some()
    );
    assert!(
        results.items[1]
            .search_match
            .as_ref()
            .unwrap()
            .fts_rank
            .is_none()
    );

    Ok(())
}

/// 测试 DAL search 方法的 Keyword-only 匹配
///
/// 当搜索关键词不含 "nonexistent" 但技能内容含 "nonexistent" 时：
/// - 查询向量 [0.0, 1.0, 1.0]
/// - 技能向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
/// - 向量距离 = 1.0 > 0.8 阈值 → 向量不命中
/// - FTS5 命中 → Keyword-only
#[sqlx::test]
async fn test_search_keyword_only_match(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 name 含 "nonexistent" 的技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let po = SkillPo::new(
        skill_id.clone(),
        "nonexistent-debug-tool".to_string(),
        "A tool for nonexistent debugging".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id),
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
    // 技能向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
    // 向量距离 = 1.0 > 0.8 → 向量不命中
    // FTS5 命中（name 和 tags 含 "debug"）→ Keyword-only
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1, "应返回 1 条 Keyword 匹配结果");
    assert_eq!(results.items[0].po.id, skill_id);
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword,
        "应是 Keyword 匹配"
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .fts_rank
            .is_some()
    );
    assert!(
        results.items[0]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_none()
    );

    Ok(())
}

/// 测试 DAL search 方法的综合排序（Hybrid 优先 → Vector → Keyword）
#[sqlx::test]
async fn test_search_comprehensive_sorting(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 1. Hybrid 技能：name 含 "debug"，向量 [0.0, 1.0, 1.0]
    let hybrid_id = uuid::Uuid::now_v7().to_string();
    let po_hybrid = SkillPo::new(
        hybrid_id.clone(),
        "debug-hybrid".to_string(),
        "Debug skill hybrid".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", hybrid_id),
    );
    skill_dal.create(ctx.clone(), &po_hybrid).await?;

    // 2. Vector-only 技能：name 不含 "debug"，向量 [0.0, 1.0, 1.0]
    let vector_id = uuid::Uuid::now_v7().to_string();
    let po_vector = SkillPo::new(
        vector_id.clone(),
        "vector-only-tool".to_string(),
        "A vector only tool".to_string(),
        vec!["utility".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", vector_id),
    );
    skill_dal.create(ctx.clone(), &po_vector).await?;

    // 3. Keyword-only 技能：name 含 "debug" + "nonexistent"，向量 [1.0, 0.0, 0.0]
    let keyword_id = uuid::Uuid::now_v7().to_string();
    let po_keyword = SkillPo::new(
        keyword_id.clone(),
        "nonexistent-debug-keyword".to_string(),
        "Keyword only debug".to_string(),
        vec!["debug".to_string()],
        "debugging".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", keyword_id),
    );
    skill_dal.create(ctx.clone(), &po_keyword).await?;

    // 搜索 "debug"：
    // - hybrid: FTS5 命中 + 向量距离 0.0 → Hybrid
    // - vector: FTS5 未命中 + 向量距离 0.0 → Vector
    // - keyword: FTS5 命中 + 向量距离 1.0 > 0.8 → Keyword
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 3, "应返回 3 条结果");

    // 验证排序：Hybrid → Vector → Keyword
    assert_eq!(results.items[0].po.id, hybrid_id, "第一条应是 Hybrid");
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid
    );

    assert_eq!(results.items[1].po.id, vector_id, "第二条应是 Vector");
    assert_eq!(
        results.items[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector
    );

    assert_eq!(results.items[2].po.id, keyword_id, "第三条应是 Keyword");
    assert_eq!(
        results.items[2].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword
    );

    Ok(())
}

/// 测试 DAL search 方法的 fts_rank 透传（从 DAO 到 Skill 实体）
#[sqlx::test]
async fn test_search_fts_rank_transparency(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建技能
    let skill_id = uuid::Uuid::now_v7().to_string();
    let po = SkillPo::new(
        skill_id.clone(),
        "rust-programming".to_string(),
        "A rust programming skill".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}/", skill_id),
    );
    skill_dal.create(ctx.clone(), &po).await?;

    // 搜索 "rust"
    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                keyword: Some("rust".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1);
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    // Hybrid 匹配时 fts_rank 应有值
    assert!(
        match_info.fts_rank.is_some(),
        "fts_rank 应有值（从 DAO 透传）"
    );

    Ok(())
}

/// `SkillSearch.top_k` 必须真正控制向量路宽度：此前 DAL 完全没读这个字段，
/// 向量路恒按 20 条召回，调用方声明的「宽召回 6 条」实际只对 FTS 路生效。
#[test]
fn vector_search_top_k_honors_explicit_width() {
    let search = SkillSearch {
        keyword: Some("Rust 代码 审阅".into()),
        top_k: Some(6),
        filters: SkillQuery {
            pagination: common::api::PaginationParams {
                limit: Some(6),
                offset: None,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(vector_search_top_k(&search), 6);
}

/// 未声明 `top_k` 的既有调用方（handler / 工具搜索）行为不能变：回落 20；
/// 越界值夹紧到 1~20，避免把 0 / 负数 / 超大值透传给 VectorStore。
#[test]
fn vector_search_top_k_defaults_and_clamps() {
    assert_eq!(
        vector_search_top_k(&SkillSearch::default()),
        20,
        "缺省必须维持老行为的 20 条"
    );
    for (given, expected) in [(0, 1), (-5, 1), (1, 1), (20, 20), (999, 20)] {
        let s = SkillSearch {
            top_k: Some(given),
            ..Default::default()
        };
        assert_eq!(vector_search_top_k(&s), expected, "top_k={given}");
    }
}

/// ⚠️ 反例护栏：查询向量生成失败不能把整个 `search` 打成 `Err`。
/// C-知识写的是「向量搜索失败时自动降级为关键词搜索」，而旧代码在
/// `embed_text_for_search` 上用了 `?`；生产 11-10 出现过一次 embedding 500，
/// 会让技能召回整路退回 1 条 tag 技能、小脑精选再次恒定空转。
#[sqlx::test]
async fn search_keeps_keyword_hits_when_query_embedding_fails(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);
    let po = create_test_skill_po("debug-helper");
    skill_dal.create(ctx.clone(), &po).await?;

    let results = skill_dal
        .search(
            ctx.clone(),
            SkillSearch {
                // FTS 命中 debug；同时触发 MockCortexDao 的 embed 失败分支
                keyword: Some("debug embed-fail".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1, "向量路挂了，FTS 命中必须照常返回");
    assert_eq!(results.items[0].po.id, po.id);
    let match_type = results.items[0].search_match.as_ref().map(|m| m.match_type);
    assert_eq!(
        match_type,
        Some(MatchType::Keyword),
        "没有向量结果时应是纯关键词命中，不能是 Hybrid / Vector"
    );

    Ok(())
}

// ============================================================
// 向量文本口径：正文必须进入向量
// ============================================================
