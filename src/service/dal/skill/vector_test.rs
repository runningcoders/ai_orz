//! 向量文本口径、重索引与三种重建模式护栏

use super::test_support::{create_test_skill_po, init_test, init_test_with_embed_calls, new_ctx};
use super::vector::{
    MAX_VECTOR_BODY_CHARS, SKILL_VECTOR_TEXT_VERSION, collection_tag, compose_vector_text,
};
use crate::models::vector::Vectorizable;
use crate::pkg::background_task::TaskProgressCounter;
use crate::service::dao::skill;
use common::enums::VectorRebuildMode;
use common::error::Result;
use sqlx::SqlitePool;

/// 拼接口径：无正文 / 空白正文只留元信息；有正文追加换行；超长按**字符**截断。
///
/// 反例（曾经的实现）：只拼 `name + description + tags`，正文完全不入向量 ——
/// 技能的方法论、适用场景都写在正文里，向量却只有标题和一句描述，
/// 召回时「按内容找技能」根本命中不了。
#[test]
fn compose_vector_text_appends_body_and_truncates_by_chars() {
    assert_eq!(compose_vector_text("meta", None), "meta");
    assert_eq!(compose_vector_text("meta", Some("   ")), "meta");
    assert_eq!(compose_vector_text("meta", Some("  正文 ")), "meta\n正文");

    let long_body = "字".repeat(MAX_VECTOR_BODY_CHARS + 10);
    let text = compose_vector_text("meta", Some(&long_body));
    assert!(text.starts_with("meta\n"));
    assert!(text.ends_with('…'), "截断后应有省略号标记");
    assert_eq!(
        text.chars().count(),
        "meta".chars().count() + 1 + MAX_VECTOR_BODY_CHARS + 1,
        "按字符截断（不是字节），且保留元信息与分隔换行"
    );
}

/// 正文写盘后 `reindex_vector` 必须让内容哈希变化，且同一份正文重跑是幂等的。
///
/// 这条是「正文进向量」的端到端护栏：`create` / `update` 都发生在写盘之前，
/// 只有 DAL 在正文落盘后调用的 `reindex_vector`（`create_with_content` /
/// `update_with_content` 的末尾，或编辑器保存正文后的单独调用）能把正文吃进去。
#[sqlx::test]
async fn reindex_vector_picks_up_main_content(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let po = create_test_skill_po("body-vector");
    skill_dal.create(ctx.clone(), &po).await?;
    let created_hash = skill_dal
        .get_vector_content_hash(ctx.clone(), &po.id)
        .await?;
    assert!(created_hash.is_some(), "create 后应已写入一条向量");

    skill_dal.write_main_content(&po, "# 正文\n这是技能的方法论与适用场景")?;
    skill_dal.reindex_vector(ctx.clone(), &po).await?;

    let after = skill_dal
        .get_vector_content_hash(ctx.clone(), &po.id)
        .await?;
    assert_ne!(
        created_hash, after,
        "正文变化后 hash 必须变化，否则正文没有进入向量"
    );
    assert_ne!(
        after.as_deref(),
        Some(sha256::digest(po.vectorize_text()).as_str()),
        "hash 必须对应「元信息 + 正文」，不能等于纯元信息的哈希"
    );

    // 幂等：同一份正文再跑一次，内容没变不应重算（hash 保持一致）
    skill_dal.reindex_vector(ctx.clone(), &po).await?;
    let again = skill_dal
        .get_vector_content_hash(ctx.clone(), &po.id)
        .await?;
    assert_eq!(after, again);

    Ok(())
}

/// 集合元数据标识必须把「向量文本口径版本」编进去。
///
/// 反例：只比较 `model_provider_id`，provider 没变就短路 ⇒ 口径升级
/// （正文开始进向量）后存量技能永远不会被重建，搜出来仍是「只有标题和一句描述」
/// 的旧语义。
#[test]
fn collection_tag_carries_text_version() {
    assert_eq!(
        collection_tag("provider-a"),
        format!("provider-a#vt{SKILL_VECTOR_TEXT_VERSION}")
    );
    assert_ne!(
        collection_tag("provider-a"),
        "provider-a".to_string(),
        "标识里缺版本 ⇒ 换 provider 才会重建，口径升级永远不会触发重建"
    );
}

// ==================== 重建模式（auto / missing / force）====================

/// 把技能集合的「口径章」盖成当前口径（模拟跑过一次重建）。
async fn stamp_current_tag(ctx: &crate::pkg::RequestContext) {
    ctx.vector_store()
        .set_collection_model_provider_id("skills", &collection_tag("mock-provider"))
        .await
        .expect("盖口径章失败");
}

/// `auto` 会因口径一致整体跳过（一条不看），`missing` 则会真的把洞补上。
///
/// 这条是「按钮为什么要给两档」的根因护栏：把 `missing` 实现成 `auto` 的别名，
/// 技能页「查漏补缺」在常见情形（口径一致但个别行缺失）下永远是 0 条，
/// 用户只能靠「强制重建」把全库重算一遍来兜底。
#[sqlx::test]
async fn auto_short_circuits_but_missing_fills_holes(pool: SqlitePool) -> Result<()> {
    let dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool.clone());

    let po = create_test_skill_po("hole");
    dal.create(ctx.clone(), &po).await?;
    stamp_current_tag(&ctx).await;

    // 制造一个洞：向量行没了，但技能行还在（写入时无 provider / upsert 被降级吞掉）
    skill::new_skill_vector_dao()
        .delete_vector(ctx.clone(), &po.id)
        .await?;
    assert!(
        dal.get_vector_content_hash(ctx.clone(), &po.id)
            .await?
            .is_none(),
        "前置：洞已制造"
    );

    // auto：口径没变 ⇒ 整体跳过，洞依旧是洞
    let progress = TaskProgressCounter::new();
    dal.rebuild_vectors(ctx.clone(), &progress, VectorRebuildMode::Auto)
        .await?;
    assert_eq!(progress.processed(), 0, "auto 不应处理任何条目");
    assert!(
        dal.get_vector_content_hash(ctx.clone(), &po.id)
            .await?
            .is_none(),
        "auto 短路时不会补洞 —— 这正是它不能当「查漏补缺」用的原因"
    );

    // missing：同样口径一致的条件下，必须把洞补上
    let progress = TaskProgressCounter::new();
    dal.rebuild_vectors(ctx.clone(), &progress, VectorRebuildMode::Missing)
        .await?;
    assert!(
        dal.get_vector_content_hash(ctx.clone(), &po.id)
            .await?
            .is_some(),
        "missing 必须补齐缺失的向量"
    );
    assert_eq!(progress.processed(), 1, "missing 模式按扫描行数上报进度");

    Ok(())
}

/// `missing` 不清空集合：新鲜的条目原样保留、内容已变的条目重算、缺失的条目补上。
#[sqlx::test]
async fn missing_rebuild_only_touches_stale_entries(pool: SqlitePool) -> Result<()> {
    let (dal, embed_calls) = init_test_with_embed_calls(pool.clone()).await;
    let ctx = new_ctx("test-user", pool.clone());

    let fresh = create_test_skill_po("fresh");
    let stale = create_test_skill_po("stale");
    let hole = create_test_skill_po("hole");
    dal.create(ctx.clone(), &fresh).await?;
    dal.create(ctx.clone(), &stale).await?;
    dal.create(ctx.clone(), &hole).await?;
    stamp_current_tag(&ctx).await;

    // stale：正文改了但没走 reindex（如直接改磁盘文件），hash 与当前文本不符
    let stale_hash_before = dal.get_vector_content_hash(ctx.clone(), &stale.id).await?;
    dal.write_main_content(&stale, "# 新正文\n这行在向量里应该反映出来")?;

    // hole：向量行直接删掉
    skill::new_skill_vector_dao()
        .delete_vector(ctx.clone(), &hole.id)
        .await?;

    // 只关心「重建这几步」发起了几次 embedding
    embed_calls.reset();
    dal.rebuild_vectors(
        ctx.clone(),
        &TaskProgressCounter::new(),
        VectorRebuildMode::Missing,
    )
    .await?;

    assert_eq!(
        embed_calls.count(),
        2,
        "三条技能里只有 stale + hole 该重算；把 fresh 也算了一遍说明逐条比对没生效\
         （「查漏补缺」等于全量重建，省不下一分钱）"
    );

    let stale_hash_after = dal.get_vector_content_hash(ctx.clone(), &stale.id).await?;
    assert_ne!(
        stale_hash_before, stale_hash_after,
        "正文已变但 hash 没更新 ⇒ 逐条比对没生效"
    );
    // 期望值按「元信息 + 磁盘上的正文」现算，不依赖 DAL 内部方法（测试要能独立复算）
    let expected_text = compose_vector_text(
        &stale.vectorize_text(),
        Some(&dal.read_main_content(&stale)?),
    );
    assert_eq!(
        stale_hash_after.as_deref(),
        Some(sha256::digest(&expected_text).as_str()),
        "重算后的 hash 必须对应「元信息 + 最新正文」"
    );

    assert!(
        dal.get_vector_content_hash(ctx.clone(), &hole.id)
            .await?
            .is_some(),
        "缺失的向量必须被补上"
    );

    Ok(())
}

/// 集合口径（provider / 文本版本）与当前不一致时，`missing` 退化为清空后全量重算。
///
/// 反例：口径不一致还坚持「只补差异」⇒ 存量行是用另一个模型算的向量，hash 必然
/// 全不等、代价等同全量，却把已删技能的孤儿行留在集合里。
#[sqlx::test]
async fn missing_rebuild_falls_back_to_full_when_tag_differs(pool: SqlitePool) -> Result<()> {
    let dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool.clone());

    let po = create_test_skill_po("tag-mismatch");
    dal.create(ctx.clone(), &po).await?;
    // 盖一个「旧口径」章：provider 不同 / 文本版本更低
    ctx.vector_store()
        .set_collection_model_provider_id("skills", "legacy-provider#vt1")
        .await?;

    let progress = TaskProgressCounter::new();
    dal.rebuild_vectors(ctx.clone(), &progress, VectorRebuildMode::Missing)
        .await?;

    assert_eq!(progress.processed(), 1, "口径不一致必须走全量重算");
    assert!(
        ctx.vector_store()
            .get_collection_model_provider_id("skills")
            .await?
            .as_deref()
            == Some(collection_tag("mock-provider").as_str()),
        "重建完成后口径章必须更新为当前口径，否则下次 auto 会反复整体重算"
    );
    assert!(
        dal.get_vector_content_hash(ctx.clone(), &po.id)
            .await?
            .is_some()
    );

    Ok(())
}
