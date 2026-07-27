//! Tests for SqliteProjectDao

use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::service::dao::project::{ProjectDao, sqlite};
use common::enums::project::ProjectStatus;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn ProjectDao + Send + Sync> {
    crate::service::dao::project::init();
    sqlite::dao()
}

/// 创建测试 ProjectPo
#[allow(dead_code)] // 测试辅助函数，保留供未来测试使用
fn create_test_project(name: &str, created_by: &str) -> ProjectPo {
    ProjectPo::new(
        Uuid::now_v7().to_string(),
        name.to_string(),
        "".to_string(),
        None,
        None,
        3,
        vec![],
        created_by.to_string(),
        None,
        None,
        None,
        None,
        created_by.to_string(),
    )
}

#[sqlx::test]
async fn test_insert_project(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id.clone(),
        "Test Project".to_string(),
        "This is a test project".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec!["test".to_string(), "demo".to_string()],
        "test-user".to_string(),
        None, // owner_agent_id
        None, // start_at
        None, // due_at
        None, // end_at
        "test-user".to_string(),
    );

    dao.insert(ctx, &project).await?;
    Ok(())
}

#[sqlx::test]
async fn test_find_by_id(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id.clone(),
        "Find Test".to_string(),
        "Test find by id".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );

    dao.insert(ctx.clone(), &project).await?;
    let found = dao.find_by_id(ctx, &project_id).await?;

    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Find Test");
    assert_eq!(found.root_user_id, "test-user");
    assert_eq!(found.status, ProjectStatus::default());
    assert!(found.owner_agent_id.is_none());
    Ok(())
}

#[sqlx::test]
async fn test_list_by_root_user(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    // Insert 3 projects for user1, 1 for user2
    for i in 0..3 {
        let project_id = uuid::Uuid::now_v7().to_string();
        let project = ProjectPo::new(
            project_id,
            format!("Project {}", i),
            "".to_string(),
            None, // workflow
            None, // guidance
            i,
            vec![],
            "user1".to_string(),
            None,
            None,
            None,
            None,
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &project).await?;
    }

    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id,
        "User2 Project".to_string(),
        "".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "user2".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &project).await?;

    let list = dao.list_by_root_user(ctx, "user1", Some(10)).await?;
    assert_eq!(list.len(), 3);
    // Ordered by priority desc
    assert_eq!(list[0].priority, 2);
    Ok(())
}

#[sqlx::test]
async fn test_list_by_root_user_and_status(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    // Create projects with different statuses
    let mut projects = Vec::new();
    for status in [
        ProjectStatus::Active,
        ProjectStatus::Active,
        ProjectStatus::Completed,
        ProjectStatus::Archived,
    ]
    .iter()
    {
        let project_id = uuid::Uuid::now_v7().to_string();
        let mut project = ProjectPo::new(
            project_id,
            "Status Test".to_string(),
            "".to_string(),
            None, // workflow
            None, // guidance
            0,
            vec![],
            "test-user".to_string(),
            None,
            None,
            None,
            None,
            "test-user".to_string(),
        );
        project.status = *status;
        projects.push(project);
    }

    for p in &projects {
        dao.insert(ctx.clone(), p).await?;
    }

    // Filter for Active
    let list = dao
        .list_by_root_user_and_status(
            ctx.clone(),
            "test-user",
            vec![ProjectStatus::Active],
            Some(10),
        )
        .await?;
    assert_eq!(list.len(), 2);

    // Filter for Active and Completed
    let list = dao
        .list_by_root_user_and_status(
            ctx.clone(),
            "test-user",
            vec![ProjectStatus::Active, ProjectStatus::Completed],
            Some(10),
        )
        .await?;
    assert_eq!(list.len(), 3);
    Ok(())
}

#[sqlx::test]
async fn test_update_project(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let project_id = uuid::Uuid::now_v7().to_string();
    let mut project = ProjectPo::new(
        project_id.clone(),
        "Original Name".to_string(),
        "Original Description".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        Some("agent-123".to_string()),
        None,
        None,
        None,
        "test-user".to_string(),
    );

    dao.insert(ctx.clone(), &project).await?;

    project.name = "Updated Name".to_string();
    project.description = "Updated Description".to_string();
    project.priority = 10;
    project.owner_agent_id = Some("agent-456".to_string());
    dao.update(ctx.clone(), &project).await?;

    let found = dao.find_by_id(ctx, &project_id).await?;
    let found = found.unwrap();
    assert_eq!(found.name, "Updated Name");
    assert_eq!(found.description, "Updated Description");
    assert_eq!(found.priority, 10);
    assert_eq!(found.owner_agent_id, Some("agent-456".to_string()));
    Ok(())
}

#[sqlx::test]
async fn test_update_status(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id.clone(),
        "Status Update Test".to_string(),
        "".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );

    dao.insert(ctx.clone(), &project).await?;
    dao.update_status(
        ctx.clone(),
        &project_id,
        ProjectStatus::Completed,
        "test-user",
    )
    .await?;

    let found = dao.find_by_id(ctx, &project_id).await?;
    let found = found.unwrap();
    assert_eq!(found.status, ProjectStatus::Completed);
    Ok(())
}

#[sqlx::test]
async fn test_count_functions(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    for i in 0..5 {
        let project_id = uuid::Uuid::now_v7().to_string();
        let mut project = ProjectPo::new(
            project_id,
            format!("Count {}", i),
            "".to_string(),
            None, // workflow
            None, // guidance
            0,
            vec![],
            "test-user".to_string(),
            None,
            None,
            None,
            None,
            "test-user".to_string(),
        );
        if i % 2 == 0 {
            project.status = ProjectStatus::Active;
        } else {
            project.status = ProjectStatus::Completed;
        }
        dao.insert(ctx.clone(), &project).await?;
    }

    let total = dao.count_by_root_user(ctx.clone(), "test-user").await?;
    assert_eq!(total, 5);

    let active = dao
        .count_by_root_user_and_status(ctx, "test-user", ProjectStatus::Active)
        .await?;
    assert_eq!(active, 3);
    Ok(())
}

#[sqlx::test]
async fn test_deleted_not_found(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let project_id = uuid::Uuid::now_v7().to_string();
    let mut project = ProjectPo::new(
        project_id.clone(),
        "To Delete".to_string(),
        "".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );
    project.status = ProjectStatus::Deleted;
    dao.insert(ctx.clone(), &project).await?;

    let found = dao.find_by_id(ctx, &project_id).await?;
    assert!(found.is_none());
    Ok(())
}

// ==================== FTS5 搜索测试 ====================

use crate::service::dao::project::ProjectSearch;

/// 创建带完整字段的测试 ProjectPo（用于搜索测试）
fn create_searchable_project(
    name: &str,
    description: &str,
    workflow: Option<&str>,
    guidance: Option<&str>,
    root_user_id: &str,
) -> ProjectPo {
    ProjectPo::new(
        Uuid::now_v7().to_string(),
        name.to_string(),
        description.to_string(),
        workflow.map(|s| s.to_string()),
        guidance.map(|s| s.to_string()),
        0,
        vec![],
        root_user_id.to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    )
}

/// 测试 FTS5 英文关键词搜索（按 name 匹配）
#[sqlx::test]
async fn test_search_projects_english_keyword(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let p1 = create_searchable_project(
        "Alpha Project",
        "Machine learning research",
        None,
        None,
        "user1",
    );
    let p2 = create_searchable_project("Beta Task", "Data pipeline", None, None, "user1");
    dao.insert(ctx.clone(), &p1).await?;
    dao.insert(ctx.clone(), &p2).await?;

    // 按名称关键词搜索
    let results = dao
        .search_projects(
            ctx,
            ProjectSearch {
                keyword: Some("Alpha".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.name, "Alpha Project");
    // fts_rank 应该有值（BM25 评分）
    assert!(results[0].1.is_some());
    Ok(())
}

/// 测试 FTS5 中文关键词搜索（trigram 分词器支持中文）
#[sqlx::test]
async fn test_search_projects_chinese_keyword(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let p1 = create_searchable_project(
        "智能助手项目",
        "基于大语言模型的对话系统",
        None,
        None,
        "user1",
    );
    let p2 = create_searchable_project("数据分析平台", "实时数据流处理", None, None, "user1");
    dao.insert(ctx.clone(), &p1).await?;
    dao.insert(ctx.clone(), &p2).await?;

    // 中文关键词搜索：按名称匹配
    let results = dao
        .search_projects(
            ctx.clone(),
            ProjectSearch {
                keyword: Some("智能助手".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.name, "智能助手项目");

    // 中文关键词搜索：按描述匹配
    let results2 = dao
        .search_projects(
            ctx,
            ProjectSearch {
                keyword: Some("数据流".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results2.len(), 1);
    assert_eq!(results2[0].0.name, "数据分析平台");
    Ok(())
}

/// 测试 FTS5 搜索无匹配返回空结果
#[sqlx::test]
async fn test_search_projects_no_match(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let p1 = create_searchable_project("Existing Project", "Some description", None, None, "user1");
    dao.insert(ctx.clone(), &p1).await?;

    let results = dao
        .search_projects(
            ctx,
            ProjectSearch {
                keyword: Some("nonexistent".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.len(), 0);
    Ok(())
}

/// 测试 FTS5 搜索过滤软删除项目（status = 0）
#[sqlx::test]
async fn test_search_projects_filters_soft_deleted(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    // 创建一个正常项目
    let p1 = create_searchable_project("Active Searchable", "visible content", None, None, "user1");
    dao.insert(ctx.clone(), &p1).await?;

    // 创建一个软删除项目（status = Deleted = 0）
    let mut p2 =
        create_searchable_project("Deleted Searchable", "hidden content", None, None, "user1");
    p2.status = ProjectStatus::Deleted;
    dao.insert(ctx.clone(), &p2).await?;

    // 搜索 "Searchable" 关键词：应只返回未删除的项目
    let results = dao
        .search_projects(
            ctx,
            ProjectSearch {
                keyword: Some("Searchable".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.name, "Active Searchable");
    Ok(())
}

/// 测试 FTS5 搜索 workflow 和 guidance 字段
#[sqlx::test]
async fn test_search_projects_workflow_guidance(pool: SqlitePool) -> Result<()> {
    let dao = init_test_env();
    let ctx = new_ctx("test-user", pool);

    let p1 = create_searchable_project(
        "Project With Workflow",
        "desc",
        Some("agile development process"),
        Some("follow coding standards"),
        "user1",
    );
    dao.insert(ctx.clone(), &p1).await?;

    // 按 workflow 内容搜索
    let results = dao
        .search_projects(
            ctx.clone(),
            ProjectSearch {
                keyword: Some("agile".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.name, "Project With Workflow");

    // 按 guidance 内容搜索
    let results2 = dao
        .search_projects(
            ctx,
            ProjectSearch {
                keyword: Some("coding standards".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results2.len(), 1);
    Ok(())
}
