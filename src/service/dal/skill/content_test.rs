//! 内容源摄取：三存储写入顺序护栏

use super::test_support::{create_test_skill_po, init_test, new_ctx};
use crate::models::skill::SkillFileImport;
use crate::models::vector::Vectorizable;
use common::error::Result;
use sqlx::SqlitePool;

/// `create_with_content` 必须在**正文落盘之后**算向量。
///
/// 反例（改造前的真实行为）：`create` 先 embed 一次元信息（此时正文还没写盘），
/// 随后补写正文再 embed 第二次 —— 第一次的 hash 等于纯元信息哈希，
/// 正是这条断言要拦的形态。
#[sqlx::test]
async fn create_with_content_indexes_body_after_files_land(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let po = create_test_skill_po("content-first");
    let imports = vec![SkillFileImport {
        target_path: None,
        source_abs_path: None,
        content_bytes: Some("# 正文\n这是技能的方法论与适用场景".as_bytes().to_vec()),
        suggested_name: Some("skill.md".to_string()),
        executable: None,
    }];

    let effective = skill_dal
        .create_with_content(ctx.clone(), po.clone(), &imports, None)
        .await?;

    assert!(
        skill_dal.file_abs_path(&effective, "skill.md").exists(),
        "内容源必须已落盘"
    );

    let hash = skill_dal
        .get_vector_content_hash(ctx, &effective.id)
        .await?;
    assert_ne!(
        hash.as_deref(),
        Some(sha256::digest(po.vectorize_text()).as_str()),
        "向量必须包含正文：等于纯元信息哈希说明 embed 发生在写盘之前"
    );
    Ok(())
}

/// 内容源处理失败必须回滚元数据行。
///
/// 三个存储没有跨库事务，只能补偿；不补偿就会留下「列表里看得见、打开是空正文」
/// 的孤儿技能，只能靠手工删。
#[sqlx::test]
async fn create_with_content_rolls_back_row_on_bad_import(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let po = create_test_skill_po("orphan-guard");
    // 既无路径也无 bytes：必然进入「无内容来源」分支
    let imports = vec![SkillFileImport::default()];

    let result = skill_dal
        .create_with_content(ctx.clone(), po.clone(), &imports, None)
        .await;
    assert!(result.is_err(), "空来源必须报错");

    assert!(
        skill_dal.get_po_by_id(ctx, po.id).await?.is_none(),
        "失败的创建不能留下元数据行"
    );
    Ok(())
}
