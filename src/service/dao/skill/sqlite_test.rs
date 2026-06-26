//! Skill DAO SQLite 单元测试

use common::error::Error;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use crate::service::dao::skill::{self, SkillDao, SkillSearch};
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;
use common::error::Result;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
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

/// 测试关键词搜索
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
    assert!(result.iter().any(|s| s.id == skill_id));

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
    assert!(result.iter().any(|s| s.id == skill_id));

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
async fn test_read_write_main_content(pool: SqlitePool) -> Result<()> {
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
async fn test_read_write_attachment_files(pool: SqlitePool) -> Result<()> {
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
async fn test_list_files_with_content(pool: SqlitePool) -> Result<()> {
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
async fn test_list_files_empty_dir(pool: SqlitePool) -> Result<()> {
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
async fn test_delete_skill_dir(pool: SqlitePool) -> Result<()> {
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
