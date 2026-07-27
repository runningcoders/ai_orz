//! Skill DAO SQLite 单元测试

use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use crate::service::dao::skill::{self, SkillDao, SkillQuery, SkillSearch};
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use common::error::Result;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试依赖
fn init_test() {
    // 必须先初始化 config（文件操作需要 base_data_path）
    let _ = crate::config::init();
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn SkillDao> {
    init_test();
    skill::new()
}

/// 创建测试 SkillPo
#[allow(dead_code)] // 测试辅助函数，保留供未来测试使用
fn create_test_skill(name: &str, category: &str) -> SkillPo {
    let skill_id = Uuid::now_v7().to_string();
    SkillPo::new(
        skill_id,
        name.to_string(),
        "".to_string(),
        vec![],
        category.to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", name),
    )
}

/// 测试插入新技能并按 ID 查询
#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env(); // 不用单例，直接创建新实例

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Test Skill".to_string(),
        "A test skill for unit testing".to_string(),
        vec!["test".to_string(), "unit-test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool);
    let found = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, skill_id);
    assert_eq!(found.name, "Test Skill");
    assert_eq!(found.status, SkillStatus::Draft);
    let tags = found.parse_tags();
    assert_eq!(tags.len(), 2);
    assert!(tags.contains(&"test".to_string()));

    Ok(())
}

/// 测试更新技能
#[sqlx::test]
async fn test_update(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let mut skill = SkillPo::new(
        skill_id.clone(),
        "Test Update".to_string(),
        "Original description".to_string(),
        vec!["test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    skill.description = "Updated description".to_string();
    skill.status = SkillStatus::Published;

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.update(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool);
    let found = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.description, "Updated description");
    assert_eq!(found.status, SkillStatus::Published);

    Ok(())
}

/// 测试按状态列表查询
#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill1_id = Uuid::now_v7().to_string();
    let mut skill1 = SkillPo::new(
        skill1_id.clone(),
        "List Test 1".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill1_id}"),
    );
    skill1.status = SkillStatus::Published;

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "List Test 2".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill2_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    let ctx = new_ctx("test-user", pool.clone());
    let available = skill_dao
        .list_by_status(ctx, SkillStatus::Published)
        .await?;
    assert!(available.iter().any(|s| s.id == skill1_id));
    assert!(!available.iter().any(|s| s.id == skill2_id));

    let ctx = new_ctx("test-user", pool);
    let pending = skill_dao.list_by_status(ctx, SkillStatus::Draft).await?;
    assert!(pending.iter().any(|s| s.id == skill2_id));

    Ok(())
}

/// 测试按分类列表查询
#[sqlx::test]
async fn test_list_by_category(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Category Test".to_string(),
        "".to_string(),
        vec![],
        "documentation".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool);
    let result = skill_dao.list_by_category(ctx, "documentation").await?;
    assert!(result.iter().any(|s| s.id == skill_id));

    Ok(())
}

/// 测试 FTS5 关键词搜索（DAO 层返回 (SkillPo, fts_rank) 元组）
#[sqlx::test]
async fn test_search(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Search Test Skill".to_string(),
        "This is a skill for searching".to_string(),
        vec!["search".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool.clone());
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("Search".to_string()),
                ..Default::default()
            },
        )
        .await?;
    // 返回类型为 Vec<(SkillPo, Option<f32>)>
    assert!(result.iter().any(|(s, _)| s.id == skill_id));
    // FTS5 命中时 fts_rank 应有值
    assert!(result.iter().any(|(_, rank)| rank.is_some()));

    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("searching".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert!(result.iter().any(|(s, _)| s.id == skill_id));

    Ok(())
}

/// 测试软删除（标记为过期）
#[sqlx::test]
async fn test_delete_by_id(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Delete Test".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool.clone());
    let found_before = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found_before.is_some());
    assert_eq!(found_before.unwrap().status, SkillStatus::Draft);

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.delete_by_id(ctx, &skill_id).await?;

    let ctx = new_ctx("test-user", pool);
    let found_after = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found_after.is_some());
    assert_eq!(found_after.unwrap().status, SkillStatus::Expired);

    Ok(())
}

/// 测试按作者列表查询
#[sqlx::test]
async fn test_list_by_author(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill1_id = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        skill1_id.clone(),
        "Author Test 1".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "alice".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill1_id}"),
    );

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "Author Test 2".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "bob".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill2_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    let ctx = new_ctx("test-user", pool);
    let alice_skills = skill_dao.list_by_author(ctx, "alice").await?;
    assert!(alice_skills.iter().any(|s| s.id == skill1_id));
    assert!(!alice_skills.iter().any(|s| s.id == skill2_id));

    Ok(())
}

/// 测试安装共享技能到 Agent（install_to_agent）
#[sqlx::test]
async fn test_install_to_agent(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // 1. 创建一个已发布的共享技能（源技能）
    let source_id = Uuid::now_v7().to_string();
    let mut source_skill = SkillPo::new(
        source_id.clone(),
        "Shared Skill".to_string(),
        "A shared published skill that can be installed to agents".to_string(),
        vec!["shared".to_string(), "utility".to_string()],
        "tools".to_string(),
        "".to_string(),       // parent_skill_id is empty for original shared skill
        "system".to_string(), // author is system (shared library)
        SkillAuthorType::User,
        format!("shared/{}", source_id),
    );
    // Publish it
    source_skill.status = SkillStatus::Published;

    let ctx = new_ctx("system", pool.clone());
    skill_dao.insert(ctx, &source_skill).await?;

    // 2. Install to agent "agent-123"
    let ctx = new_ctx("admin", pool.clone());
    let installed = skill_dao
        .install_to_agent(ctx, &source_skill, "agent-123")
        .await?;

    // 3. Verify the installed copy
    // - Should have new generated id
    assert!(!installed.id.is_empty());
    assert_ne!(installed.id, source_id);

    // - Should copy all metadata
    assert_eq!(installed.name, source_skill.name);
    assert_eq!(installed.description, source_skill.description);
    assert_eq!(installed.parse_tags(), source_skill.parse_tags());
    assert_eq!(installed.category, source_skill.category);

    // - Should have correct attributes
    assert_eq!(installed.parent_skill_id, source_id.clone());
    assert_eq!(installed.author_id, "agent-123");
    assert_eq!(installed.status, SkillStatus::Draft); // default is Draft
    assert_eq!(
        installed.content_path,
        format!("agents/agent-123/skills/{}", installed.id)
    );

    // 4. Verify it exists in database
    let ctx = new_ctx("test-user", pool);
    let found = skill_dao.find_by_id(ctx, &installed.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, installed.id);
    assert_eq!(found.parent_skill_id, source_id);

    Ok(())
}

/// 测试安装非已发布技能应该返回错误
#[sqlx::test]
async fn test_install_non_published_fails(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // Create a draft skill (not published)
    let source_id = Uuid::now_v7().to_string();
    let source_skill = SkillPo::new(
        source_id.clone(),
        "Draft Skill".to_string(),
        "This is still a draft".to_string(),
        vec![],
        "test".to_string(),
        "".to_string(),
        "author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}", source_id),
    );
    // It's Draft by default, not Published

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &source_skill).await?;

    // Try to install - should fail
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .install_to_agent(ctx, &source_skill, "agent-123")
        .await;

    // Should be error
    assert!(result.is_err());
    let err = result.err().unwrap();
    // Error should mention that only published can be installed
    let err_msg = err.to_string();
    assert!(err_msg.contains("Only published skills can be installed"));

    Ok(())
}

/// 测试读写 skill.md 主文件内容
#[sqlx::test]
async fn test_read_write_main_content(_pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "File Test Skill".to_string(),
        "Test file operations".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/test/{}", skill_id),
    );

    // 1. 写入主文件内容
    let test_content = "# Test Skill\n\nThis is the main skill markdown content.";
    skill_dao.write_main_content(&skill, test_content)?;

    // 2. 读取并验证内容
    let read_content = skill_dao.read_main_content(&skill)?;
    assert_eq!(read_content, test_content);

    // 3. 覆盖写入新内容
    let new_content = "# Updated Skill\n\nNew content here.";
    skill_dao.write_main_content(&skill, new_content)?;

    let updated_content = skill_dao.read_main_content(&skill)?;
    assert_eq!(updated_content, new_content);

    Ok(())
}

/// 测试读写附属文件
#[sqlx::test]
async fn test_read_write_attachment_files(_pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Attachment Test".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/test/{}", skill_id),
    );

    // 1. 写入多个附属文件
    skill_dao.write_file(&skill, "example.py", "print('hello world')")?;
    skill_dao.write_file(&skill, "notes.md", "# Notes\n\nSome notes.")?;
    skill_dao.write_file(&skill, "config.json", "{\"enabled\": true}")?;

    // 2. 读取并验证
    let py_content = skill_dao.read_file(&skill, "example.py")?;
    assert_eq!(py_content, "print('hello world')");

    let md_content = skill_dao.read_file(&skill, "notes.md")?;
    assert_eq!(md_content, "# Notes\n\nSome notes.");

    // 3. 读取不存在的文件应该返回错误
    let result = skill_dao.read_file(&skill, "nonexistent.txt");
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("File not found"));

    Ok(())
}

/// 测试 list_files 列出所有文件并自动预读小文件内容
#[sqlx::test]
async fn test_list_files_with_content(_pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "List Files Test".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/test/{}", skill_id),
    );

    // 1. 写入文件
    skill_dao.write_main_content(&skill, "# Main Skill\n\nContent here.")?;
    skill_dao.write_file(&skill, "small.txt", "This is a small file.")?;

    // 2. 列出文件
    let files = skill_dao.list_files(&skill)?;

    // 应该有 2 个文件
    assert_eq!(files.len(), 2);

    // skill.md 应该排在前面（按字母排序）
    assert_eq!(files[0].filename, "skill.md");
    assert_eq!(files[1].filename, "small.txt");

    // 小文件应该自动预读了内容
    assert!(files[0].content.is_some());
    assert!(files[1].content.is_some());
    assert_eq!(
        files[0].content.as_ref().unwrap(),
        "# Main Skill\n\nContent here."
    );
    assert_eq!(files[1].content.as_ref().unwrap(), "This is a small file.");

    Ok(())
}

/// 测试不存在的技能目录返回空列表
#[sqlx::test]
async fn test_list_files_empty_dir(_pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Empty Dir Test".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/nonexistent/{}", skill_id),
    );

    // 目录不存在时返回空列表
    let files = skill_dao.list_files(&skill)?;
    assert_eq!(files.len(), 0);

    // 不存在的目录读取主文件返回空字符串
    let content = skill_dao.read_main_content(&skill)?;
    assert_eq!(content, "");

    Ok(())
}

/// 测试删除技能目录
#[sqlx::test]
async fn test_delete_skill_dir(_pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Delete Dir Test".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/delete/{}", skill_id),
    );

    // 1. 创建文件
    skill_dao.write_main_content(&skill, "# To be deleted")?;
    skill_dao.write_file(&skill, "temp.txt", "temporary data")?;

    // 验证文件存在
    let files_before = skill_dao.list_files(&skill)?;
    assert_eq!(files_before.len(), 2);

    // 2. 删除目录
    skill_dao.delete_skill_dir(&skill)?;

    // 3. 验证文件已删除
    let files_after = skill_dao.list_files(&skill)?;
    assert_eq!(files_after.len(), 0);

    // 删除不存在的目录不报错
    skill_dao.delete_skill_dir(&skill)?;

    Ok(())
}

/// 测试 install_to_agent 时完整拷贝所有文件
#[sqlx::test]
async fn test_install_to_agent_copies_all_files(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // 1. 创建一个已发布的共享技能，带多个文件
    let source_id = Uuid::now_v7().to_string();
    let mut source_skill = SkillPo::new(
        source_id.clone(),
        "Copy Files Skill".to_string(),
        "".to_string(),
        vec![],
        "tools".to_string(),
        "".to_string(),
        "system".to_string(),
        SkillAuthorType::User,
        format!("shared/source/{}", source_id),
    );
    source_skill.status = SkillStatus::Published;

    // 写入源技能文件
    skill_dao.write_main_content(&source_skill, "# Shared Skill\n\nThis is a shared skill.")?;
    skill_dao.write_file(
        &source_skill,
        "example.rs",
        "fn main() { println!(\"hello\"); }",
    )?;
    skill_dao.write_file(
        &source_skill,
        "usage.md",
        "# Usage\n\nHow to use this skill.",
    )?;

    let ctx = new_ctx("system", pool.clone());
    skill_dao.insert(ctx, &source_skill).await?;

    // 2. 安装到 Agent
    let ctx = new_ctx("admin", pool.clone());
    let installed = skill_dao
        .install_to_agent(ctx, &source_skill, "agent-456")
        .await?;

    // 3. 验证所有文件都被拷贝了
    let installed_files = skill_dao.list_files(&installed)?;
    assert_eq!(installed_files.len(), 3); // skill.md + example.rs + usage.md

    // 验证文件名都在
    let filenames: Vec<&str> = installed_files
        .iter()
        .map(|f| f.filename.as_str())
        .collect();
    assert!(filenames.contains(&"skill.md"));
    assert!(filenames.contains(&"example.rs"));
    assert!(filenames.contains(&"usage.md"));

    // 验证内容正确拷贝
    let main_content = skill_dao.read_main_content(&installed)?;
    assert_eq!(main_content, "# Shared Skill\n\nThis is a shared skill.");

    let example_content = skill_dao.read_file(&installed, "example.rs")?;
    assert_eq!(example_content, "fn main() { println!(\"hello\"); }");

    let usage_content = skill_dao.read_file(&installed, "usage.md")?;
    assert_eq!(usage_content, "# Usage\n\nHow to use this skill.");

    Ok(())
}

/// 测试按 tag 过滤查询
#[sqlx::test]
async fn test_query_by_tag(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // 创建带不同 tag 的技能
    let skill1_id = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        skill1_id.clone(),
        "Python Automation".to_string(),
        "".to_string(),
        vec!["python".to_string(), "automation".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill1_id),
    );

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "Rust Systems".to_string(),
        "".to_string(),
        vec!["rust".to_string(), "systems".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill2_id),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    // 按 python tag 查询，应只返回 skill1
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .query(
            ctx,
            SkillQuery {
                tags: Some(vec!["python".to_string()]),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].id, skill1_id);

    Ok(())
}

/// 测试多 tag OR 语义查询
#[sqlx::test]
async fn test_query_by_multiple_tags(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill1_id = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        skill1_id.clone(),
        "Python Tool".to_string(),
        "".to_string(),
        vec!["python".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill1_id),
    );

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "Rust Tool".to_string(),
        "".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill2_id),
    );

    let skill3_id = Uuid::now_v7().to_string();
    let skill3 = SkillPo::new(
        skill3_id.clone(),
        "JavaScript Tool".to_string(),
        "".to_string(),
        vec!["javascript".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill3_id),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx.clone(), &skill2).await?;
    skill_dao.insert(ctx, &skill3).await?;

    // 查询 python 或 rust tag，应返回 skill1 和 skill2（OR 语义），不含 skill3
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .query(
            ctx,
            SkillQuery {
                tags: Some(vec!["python".to_string(), "rust".to_string()]),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(result.items.len(), 2);
    let ids: Vec<String> = result.items.iter().map(|s| s.id.clone()).collect();
    assert!(ids.contains(&skill1_id));
    assert!(ids.contains(&skill2_id));
    assert!(!ids.contains(&skill3_id));

    Ok(())
}

/// 测试不传 tags 时保持现有行为（返回全部）
#[sqlx::test]
async fn test_query_without_tags(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill1_id = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        skill1_id.clone(),
        "Tagged Skill A".to_string(),
        "".to_string(),
        vec!["alpha".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill1_id),
    );

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "Tagged Skill B".to_string(),
        "".to_string(),
        vec!["beta".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill2_id),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    // tags = None 时，不过滤 tag，应返回全部
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .query(
            ctx,
            SkillQuery {
                tags: None,
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(result.items.len(), 2);
    let ids: Vec<String> = result.items.iter().map(|s| s.id.clone()).collect();
    assert!(ids.contains(&skill1_id));
    assert!(ids.contains(&skill2_id));

    Ok(())
}

/// 测试 FTS5 搜索可以匹配 tags 字段（trigram 分词器对 tags JSON 字符串建立索引）
#[sqlx::test]
async fn test_keyword_search_matches_tags(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // name 和 description 都不含 "unique_tag_xyz"，只有 tags 含
    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Generic Skill".to_string(),
        "A generic description".to_string(),
        vec!["unique_tag_xyz".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{}", skill_id),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    // 用 tag 内容作为 FTS5 关键词搜索，应能命中（trigram 对 tags JSON 字符串建索引）
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("unique_tag_xyz".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0.id, skill_id);

    Ok(())
}

// ==================== FTS5 触发器同步测试 ====================
// 注意：trigram 分词器对 CJK 字符基于三字符子串匹配，MATCH 搜索使用英文关键词验证。
// 中文内容同步通过直接 SELECT FTS 表验证。

/// 测试 skills AFTER INSERT 触发器同步到 skills_fts
#[sqlx::test]
async fn test_skill_fts5_trigger_insert_sync(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Rust ownership skill".to_string(),
        "A skill about Rust memory safety".to_string(),
        vec!["rust".to_string(), "memory".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    // 1. FTS 表应该有 1 条记录
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "INSERT 后 FTS 表应有 1 条记录");

    // 2. 直接查询 FTS 表验证内容同步（含字段对齐）
    let row = sqlx::query("SELECT rowid, name, description, tags FROM skills_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    let name: String = row.get("name");
    let description: String = row.get("description");
    let tags: String = row.get("tags");
    let rowid: i64 = row.get("rowid");
    assert!(name.contains("Rust"));
    assert!(description.contains("memory safety"));
    assert!(tags.contains("rust"));
    assert!(rowid > 0);

    // 3. 通过 name MATCH 搜索英文关键词
    let name_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts WHERE name MATCH ?")
        .bind("rust")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name_count, 1, "name MATCH rust 应命中");

    // 4. 通过 description MATCH 搜索英文关键词
    let desc_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts WHERE description MATCH ?")
            .bind("memory")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(desc_count, 1, "description MATCH memory 应命中");

    // 5. 通过 tags MATCH 搜索英文关键词
    let tags_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts WHERE tags MATCH ?")
        .bind("rust")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tags_count, 1, "tags MATCH rust 应命中");

    Ok(())
}

/// 测试 skills AFTER UPDATE 触发器同步到 skills_fts（先删旧条目再插新条目）
#[sqlx::test]
async fn test_skill_fts5_trigger_update_sync(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let mut skill = SkillPo::new(
        skill_id.clone(),
        "Rust programming skill".to_string(),
        "Original description about rust".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill).await?;

    // 更新 name/description/tags：rust -> python（AFTER UPDATE 触发器先删旧 FTS 条目再插新条目）
    skill.name = "Python programming skill".to_string();
    skill.description = "Updated description about python".to_string();
    skill.tags = serde_json::to_string(&vec!["python".to_string()]).unwrap();
    skill_dao.update(ctx, &skill).await?;

    // FTS 表仍应只有 1 条记录（update 触发器先删后插，不是新增）
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "UPDATE 后 FTS 表仍应只有 1 条记录");

    // 新关键词 python 应能搜到
    let new_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts WHERE name MATCH ?")
        .bind("python")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(new_count, 1, "更新后应能搜到新关键词 python");

    // 旧关键词 rust 应搜不到（旧 FTS 条目已被触发器删除）
    let old_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts WHERE name MATCH ?")
        .bind("rust")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_count, 0, "更新后旧关键词 rust 应已从 FTS 移除");

    Ok(())
}

/// 测试 skills AFTER DELETE 触发器同步到 skills_fts
#[sqlx::test]
async fn test_skill_fts5_trigger_delete_sync(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Rust deletable skill".to_string(),
        "A skill that will be hard deleted".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    // 确认 FTS 已有 1 条记录
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 1);

    // 硬删除（注意：DAO 的 delete_by_id 是软删除=UPDATE status，不会触发 AFTER DELETE）
    // 这里用原始 SQL 触发 AFTER DELETE 触发器
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(&skill_id)
        .execute(&pool)
        .await
        .unwrap();

    // FTS 表中对应记录应已被触发器删除
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 0, "硬删除后 FTS 表应为空");

    // MATCH 搜索应搜不到
    let search: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skills_fts WHERE name MATCH ?")
        .bind("rust")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(search, 0, "删除后 MATCH 搜索应无结果");

    Ok(())
}

// ==================== FTS5 DAO search 方法测试 ====================

/// 测试 FTS5 search 方法返回 (SkillPo, fts_rank) 元组，并按 BM25 相关性排序
#[sqlx::test]
async fn test_search_returns_fts_rank_and_bm25_order(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // 创建 3 个技能：都包含关键词 "rust"，但在不同字段中（影响 BM25 评分）
    let id1 = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        id1.clone(),
        "Rust Rust Rust".to_string(), // name 中多次出现
        "A skill about rust programming".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id1}"),
    );

    let id2 = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        id2.clone(),
        "Programming skill".to_string(),
        "Rust rust rust rust rust".to_string(), // description 中多次出现
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id2}"),
    );

    let id3 = Uuid::now_v7().to_string();
    let skill3 = SkillPo::new(
        id3.clone(),
        "Generic Tool".to_string(),
        "A generic tool".to_string(),
        vec!["rust".to_string()], // 仅 tags 含
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id3}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx.clone(), &skill2).await?;
    skill_dao.insert(ctx, &skill3).await?;

    // 搜索 "rust"，应返回 3 条结果，每条都带 fts_rank
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("rust".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(result.len(), 3, "应返回 3 条匹配结果");

    // 所有结果都应有 fts_rank 值
    for (po, rank) in &result {
        assert!(rank.is_some(), "skill {} 的 fts_rank 不应为 None", po.id);
    }

    // 结果应按 BM25 相关性排序（rank 越小越相关）
    let ranks: Vec<f32> = result.iter().filter_map(|(_, r)| *r).collect();
    for i in 0..ranks.len() - 1 {
        assert!(
            ranks[i] <= ranks[i + 1],
            "BM25 排序错误：rank[{}]={} 应 <= rank[{}]={}",
            i,
            ranks[i],
            i + 1,
            ranks[i + 1]
        );
    }

    Ok(())
}

/// 测试 FTS5 search 方法支持中文关键词（trigram 分词器）
#[sqlx::test]
async fn test_search_chinese_keyword(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "代码审查技能".to_string(),
        "用于审查 Rust 代码质量的技能".to_string(),
        vec!["审查".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    // trigram 分词器对中文基于三字符子串匹配
    // "代码审查" 是 4 个字符，包含 "代码审" 和 "码审查" 两个 trigram
    let ctx = new_ctx("test-user", pool.clone());
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("代码审查".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0.id, skill_id);
    assert!(result[0].1.is_some(), "fts_rank 应有值");

    // 搜索 description 中的中文内容
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("代码质量".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result.len(), 1, "应通过 description 中的中文内容命中");

    Ok(())
}

/// 测试 FTS5 search 方法空关键词返回空结果
#[sqlx::test]
async fn test_search_empty_keyword_returns_empty(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let skill_id = Uuid::now_v7().to_string();
    let skill_dir = format!("skills/pending/{skill_id}");
    let skill = SkillPo::new(
        skill_id,
        "Test Skill".to_string(),
        "Description".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        skill_dir,
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    // 空关键词应返回空结果（FTS5 MATCH 空字符串会报错，所以直接返回空）
    let ctx = new_ctx("test-user", pool.clone());
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result.len(), 0);

    // None 关键词也应返回空结果
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: None,
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result.len(), 0);

    Ok(())
}

/// 测试 FTS5 search 方法结合业务过滤条件（status 过滤）
#[sqlx::test]
async fn test_search_with_status_filter(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    // 创建一个 Published 技能和一个 Draft 技能，都包含 "rust" 关键词
    let id1 = Uuid::now_v7().to_string();
    let mut skill1 = SkillPo::new(
        id1.clone(),
        "Rust Published".to_string(),
        "Published rust skill".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id1}"),
    );
    skill1.status = SkillStatus::Published;

    let id2 = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        id2.clone(),
        "Rust Draft".to_string(),
        "Draft rust skill".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id2}"),
    );
    // skill2 默认是 Draft

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    // 不加过滤：应返回 2 条
    let ctx = new_ctx("test-user", pool.clone());
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("rust".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result.len(), 2);

    // 过滤只看 Published：应返回 1 条
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .search(
            ctx,
            SkillSearch {
                keyword: Some("rust".to_string()),
                filters: SkillQuery {
                    status: Some(SkillStatus::Published),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0.id, id1);

    Ok(())
}

/// 测试 query 方法中 keyword 被忽略（已迁移到 FTS5 search 方法）
#[sqlx::test]
async fn test_query_keyword_is_ignored(pool: SqlitePool) -> Result<()> {
    let skill_dao = init_test_env();

    let id1 = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        id1.clone(),
        "Rust Programming".to_string(),
        "A rust skill".to_string(),
        vec!["rust".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id1}"),
    );

    let id2 = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        id2.clone(),
        "Python Programming".to_string(),
        "A python skill".to_string(),
        vec!["python".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{id2}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    // query 方法带 keyword：keyword 会被忽略，返回全部结果（不按关键词过滤）
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao
        .query(
            ctx,
            SkillQuery {
                keyword: Some("rust".to_string()),
                ..Default::default()
            },
        )
        .await?;

    // keyword 被忽略，应返回全部 2 条（而不是只返回包含 "rust" 的 1 条）
    assert_eq!(
        result.items.len(),
        2,
        "query 方法的 keyword 应被忽略，返回全部结果"
    );

    Ok(())
}
