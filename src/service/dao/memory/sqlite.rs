//! Memory DAO SQLite 实现
//!
//! 负责：
//! - 短期记忆索引的增删查改（SQLite）
//! - 长期知识图谱节点和关系的增删查改（SQLite）
//! - 记忆追踪文件的写入（每日文件追加）
//! - 原始记忆不可修改不可删除，只能追加

use crate::error::AppError;
use crate::models::memory::{MemoryTrace, ShortTermMemoryIndexPo, LongTermKnowledgeNodePo, KnowledgeReferencePo, KnowledgeNodeRelationPo, KnowledgeRelationType};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryDao;
use async_trait::async_trait;
use serde_json;
use sqlx::SqlitePool;
use std::fs::{OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use crate::config;

// ==================== 工厂方法 + 单例 ====================

static MEMORY_DAO: OnceLock<Arc<dyn super::MemoryDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Memory DAO 实例（用于测试）
pub fn new() -> Arc<dyn super::MemoryDao + Send + Sync> {
    Arc::new(MemoryDaoSqliteImpl::new())
}

/// 获取 Memory DAO 单例
pub fn dao() -> Arc<dyn super::MemoryDao + Send + Sync> {
    MEMORY_DAO.get().cloned().unwrap()
}

/// 初始化 Memory DAO 单例
pub fn init() {
    let _ = MEMORY_DAO.set(new());
}

/// SQLite Memory DAO 实现
pub struct MemoryDaoSqliteImpl;

impl MemoryDaoSqliteImpl {
    /// 创建新的 DAO 实例
    pub fn new() -> Self {
        MemoryDaoSqliteImpl
    }

    /// 获取 Agent 记忆目录完整路径（用于写入）
    fn agent_memory_dir(&self, agent_id: &str) -> PathBuf {
        config::get().agent_memory_dir(agent_id)
    }

    /// 获取今日日期文件完整路径（用于写入）
    fn today_path(&self, agent_id: &str) -> PathBuf {
        let now = chrono::Local::now();
        let date_str = now.format("%Y%m%d").to_string();
        let agent_dir = self.agent_memory_dir(agent_id);
        agent_dir.join(format!("{date_str}.jsonl"))
    }

    /// 获取连接池从上下文
    fn pool(&self, ctx: RequestContext) -> SqlitePool {
        ctx.db_pool().clone()
    }

    /// 获取今日日期文件名（用于存储到数据库）
    /// 格式: YYYYMMDD.jsonl（相对于 agent memory 目录）
    pub fn today_filename(&self) -> String {
        let now = chrono::Local::now();
        now.format("%Y%m%d.jsonl").to_string()
    }

    /// Read original memory content by knowledge reference
    ///
    /// Uses date_path (YYYYMMDD.jsonl) + line_number to read the exact JSON line
    pub fn read_memory_reference(&self, reference: &KnowledgeReferencePo) -> Result<String, AppError> {
        // Full path: agent memory dir + date file name
        let agent_id = reference.knowledge_id.split('/').next().unwrap_or(&reference.knowledge_id);
        let agent_dir = self.agent_memory_dir(agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        // date_path is just YYYYMMDD.jsonl
        let date = reference.date_path.replace(".jsonl", "");
        let line = writer.read_line(&date, reference.line_number as usize)?;
        // Parse as MemoryTrace and return formatted content for display
        let trace: MemoryTrace = serde_json::from_str(&line)?;
        Ok(trace.content)
    }
}

#[async_trait]
impl MemoryDao for MemoryDaoSqliteImpl {
    async fn append_memory_trace(
        &self,
        ctx: RequestContext,
        trace: &MemoryTrace,
        summary: String,
        tags: Vec<String>,
    ) -> Result<ShortTermMemoryIndexPo, AppError> {
        // 1. Use DailyJsonlWriter to append to the daily JSONL file
        let agent_dir = self.agent_memory_dir(&trace.agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        let (_date_filename, _line_number) = writer.append(trace)?;

        // 2. Insert short-term index to SQLite
        let pool = self.pool(ctx);
        let tags_json = serde_json::to_string(&tags)?;
        let role_str = trace.role.to_string();
        let now = chrono::Utc::now().timestamp();
        let created_at = trace.created_at;
        let task_id = trace.task_id.as_ref();

        use common::enums::MemoryStatus;
        let status_i32 = MemoryStatus::Active as i32;
        sqlx::query!(
            r#"
INSERT INTO short_term_memory_index (
    id, agent_id, task_id, role, summary, tags, status, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
            trace.id,
            trace.agent_id,
            task_id,
            role_str,
            summary,
            tags_json,
            status_i32,
            created_at,
            now
        )
        .execute(&pool)
        .await?;

        // 3. Return index
        Ok(ShortTermMemoryIndexPo {
            id: trace.id.clone(),
            agent_id: trace.agent_id.clone(),
            task_id: trace.task_id.clone(),
            role: role_str,
            summary,
            tags: tags_json,
            status: MemoryStatus::Active,
            created_at,
            updated_at: now,
        })
    }

    async fn forget_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<(), AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let status_i32 = MemoryStatus::Forgotten as i32;
        // 软删除：标记为已遗忘，保留数据可恢复
        sqlx::query!(
            r#"
UPDATE short_term_memory_index
SET status = ?, updated_at = ?
WHERE id = ?
"#,
            status_i32,
            now,
            id
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn batch_append_memory_traces(
        &self,
        ctx: RequestContext,
        traces: &[(MemoryTrace, String, Vec<String>)],
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError> {
        if traces.is_empty() {
            return Ok(Vec::new());
        }

        // 1. 确保第一个 trace 的 Agent 目录存在
        if let Some((first_trace, _, _)) = traces.first() {
            let agent_dir = self.agent_memory_dir(&first_trace.agent_id);
            std::fs::create_dir_all(&agent_dir)?;
        }

        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        let mut result = Vec::with_capacity(traces.len());
        let now = chrono::Utc::now().timestamp();

        // 计算聚合 id: 所有 trace id 拼接后二次 hash
        let mut combined_ids = String::new();
        for (trace, _, _) in traces {
            combined_ids.push_str(&trace.id);
        }
        let aggregated_id = sha256::digest(combined_ids.as_bytes());
        let aggregated_id_cloned = aggregated_id.clone();

        for (trace, summary, tags) in traces {
            // 获取今日文件路径
            let file_path = self.today_path(&trace.agent_id);

            // 追加写入文件
            let mut file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
            {
                Ok(file) => file,
                Err(e) => return Err(AppError::Io(e)),
            };

            // 获取当前文件大小（就是我们要写入的起始偏移）
            let content_md = trace.to_markdown();
            let _byte_start = file.seek(SeekFrom::End(0))?;
            let _byte_length = content_md.len() as u64;

            // 写入 markdown
            file.write_all(content_md.as_bytes())?;

            let aggregated_id_for_insert = aggregated_id_cloned.clone();
            let tags_json = serde_json::to_string(tags)?;
            let role_str = trace.role.to_string();
            let created_at = trace.created_at;
            let task_id = trace.task_id.as_ref();

            use common::enums::MemoryStatus;
            // 9 Rust parameters → 9 question marks
            let status_i32 = MemoryStatus::Active as i32;
            sqlx::query!(
                r#"
INSERT INTO short_term_memory_index (
    id, agent_id, task_id, role, summary, tags, status, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
                aggregated_id_for_insert,
                trace.agent_id,
                task_id,
                role_str,
                summary,
                tags_json,
                status_i32,
                created_at,
                now
            )
            .execute(&mut *tx)
            .await?;

            // 保存结果
            result.push(ShortTermMemoryIndexPo {
                id: aggregated_id.clone(),
                agent_id: trace.agent_id.clone(),
                task_id: trace.task_id.clone(),
                role: role_str,
                summary: summary.clone(),
                tags: tags_json.clone(),
                status: MemoryStatus::Active,
                created_at,
                updated_at: now,
            });
        }

        tx.commit().await?;
        Ok(result)
    }

    async fn get_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ShortTermMemoryIndexPo>, AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let index = sqlx::query_as!(
            ShortTermMemoryIndexPo,
            r#"
SELECT id, agent_id, task_id, role, summary, tags, status AS "status: MemoryStatus", created_at, updated_at
FROM short_term_memory_index
WHERE id = ? AND status != 0
"#,
            id
        )
        .fetch_optional(&pool)
        .await?;

        Ok(index)
    }

    async fn list_short_term_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let agent_id_owned = agent_id.to_string();
        let limit_i64 = limit as i64;
        let indexes = sqlx::query_as!(
            ShortTermMemoryIndexPo,
            r#"
SELECT id, agent_id, task_id, role, summary, tags, status AS "status: MemoryStatus", created_at, updated_at
FROM short_term_memory_index
WHERE agent_id = ? AND status != 0
ORDER BY created_at DESC
LIMIT ?
"#,
            agent_id_owned,
            limit_i64
        )
        .fetch_all(&pool)
        .await?;

        Ok(indexes)
    }

    async fn search_short_term(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let agent_id_owned = agent_id.to_string();
        let query_owned = query.to_string();
        let limit_i64 = limit as i64;
        let indexes = sqlx::query_as!(
            ShortTermMemoryIndexPo,
            r#"
SELECT id, agent_id, task_id, role, summary, tags, status AS "status: MemoryStatus", created_at, updated_at
FROM short_term_memory_index
WHERE agent_id = ? AND summary MATCH ? AND status != 0
LIMIT ?
"#,
            agent_id_owned,
            query_owned,
            limit_i64
        )
        .fetch_all(&pool)
        .await?;

        Ok(indexes)
    }

    fn read_memory_content(&self, _index: &ShortTermMemoryIndexPo) -> Result<String, AppError> {
        // 原始文件读取由上层业务处理，这里直接返回空字符串占位
        Ok(String::new())
    }

    // ========== 长期知识图谱相关 ==========

    async fn save_knowledge_node(
        &self,
        ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);

        // 先试试更新，如果不存在就插入
        
        let status_i32 = node.status as i32;
        let result: sqlx::Result<sqlx::sqlite::SqliteQueryResult> = sqlx::query!(
            r#"
UPDATE long_term_knowledge_node
SET agent_id = ?,
    node_name = ?,
    node_description = ?,
    node_type = ?,
    summary = ?,
    status = ?,
    updated_at = ?
WHERE id = ?
"#,
            node.agent_id,
            node.node_name,
            node.node_description,
            node.node_type,
            node.summary,
            status_i32,
            node.updated_at,
            node.id,
        )
        .execute(&pool)
        .await;
        let result = result?;
        let rows_affected = result.rows_affected();

        if rows_affected == 0 {
            // 不存在，插入新节点
            // 9 Rust parameters → 9 question marks (all non-Option)
            
            let status_i32 = node.status as i32;
            sqlx::query!(
                r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, status, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
                node.id,
                node.agent_id,
                node.node_name,
                node.node_description,
                node.node_type,
                node.summary,
                status_i32,
                node.created_at,
                node.updated_at
            )
            .execute(&pool)
            .await?;
        }

        Ok(())
    }

    async fn batch_save_knowledge_nodes(
        &self,
        ctx: RequestContext,
        nodes: &[LongTermKnowledgeNodePo],
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for node in nodes {
            
            let status_i32 = node.status as i32;
            let result: sqlx::Result<sqlx::sqlite::SqliteQueryResult> = sqlx::query!(
                r#"
UPDATE long_term_knowledge_node
SET agent_id = ?,
    node_name = ?,
    node_description = ?,
    node_type = ?,
    summary = ?,
    status = ?,
    updated_at = ?
WHERE id = ?
"#,
                node.agent_id,
                node.node_name,
                node.node_description,
                node.node_type,
                node.summary,
                status_i32,
                node.updated_at,
                node.id,
            )
            .execute(&mut *tx)
            .await;
            let result = result?;
            let rows_affected = result.rows_affected();

            if rows_affected == 0 {
                // 不存在，插入新节点
                // 9 Rust parameters → 9 question marks (all non-Option)
                
                let status_i32 = node.status as i32;
                sqlx::query!(
                    r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, status, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
                    node.id,
                    node.agent_id,
                    node.node_name,
                    node.node_description,
                    node.node_type,
                    node.summary,
                    status_i32,
                    node.created_at,
                    node.updated_at
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_knowledge_node(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<LongTermKnowledgeNodePo>, AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let node = sqlx::query_as!(
            LongTermKnowledgeNodePo,
            r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, status AS "status: MemoryStatus", created_at, updated_at
FROM long_term_knowledge_node
WHERE id = ? AND status != 0
"#,
            id
        )
        .fetch_optional(&pool)
        .await?;

        Ok(node)
    }

    async fn list_knowledge_nodes_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        node_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>, AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let agent_id_owned = agent_id.to_string();
        let limit_i64 = limit as i64;
        let nodes = if let Some(node_type) = node_type {
            let node_type_owned = node_type.to_string();
            sqlx::query_as!(
                LongTermKnowledgeNodePo,
                r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, status AS "status: MemoryStatus", created_at, updated_at
FROM long_term_knowledge_node
WHERE agent_id = ? AND node_type = ? AND status != 0
ORDER BY updated_at DESC
LIMIT ?
"#,
                agent_id_owned,
                node_type_owned,
                limit_i64
            )
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as!(
                LongTermKnowledgeNodePo,
                r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, status AS "status: MemoryStatus", created_at, updated_at
FROM long_term_knowledge_node
WHERE agent_id = ? AND status != 0
ORDER BY updated_at DESC
LIMIT ?
"#,
                agent_id_owned,
                limit_i64
            )
            .fetch_all(&pool)
            .await?
        };

        Ok(nodes)
    }

    async fn search_knowledge_nodes(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>, AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let agent_id_owned = agent_id.to_string();
        let query_owned = query.to_string();
        let query_owned2 = query_owned.clone();
        let limit_i64 = limit as i64;
        let nodes = sqlx::query_as!(
            LongTermKnowledgeNodePo,
            r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, status AS "status: MemoryStatus", created_at, updated_at
FROM long_term_knowledge_node
WHERE agent_id = ? AND (node_name MATCH ? OR summary MATCH ?) AND status != 0
LIMIT ?
"#,
            agent_id_owned,
            query_owned,
            query_owned2,
            limit_i64
        )
        .fetch_all(&pool)
        .await?;

        Ok(nodes)
    }

    async fn delete_knowledge_node(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<(), AppError> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let status_i32 = MemoryStatus::Forgotten as i32;
        // 软删除：标记为已遗忘，保留数据可恢复
        sqlx::query!(
            r#"
UPDATE long_term_knowledge_node
SET status = ?, updated_at = ?
WHERE id = ?
"#,
            status_i32,
            now,
            id
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn add_knowledge_reference(
        &self,
        ctx: RequestContext,
        reference: &KnowledgeReferencePo,
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);

        sqlx::query!(
            r#"
INSERT INTO knowledge_reference (
    id, knowledge_id, short_term_id, trace_id, date_path, line_number, created_at
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#,
            reference.id,
            reference.knowledge_id,
            reference.short_term_id,
            reference.trace_id,
            reference.date_path,
            reference.line_number,
            reference.created_at,
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn batch_add_knowledge_references(
        &self,
        ctx: RequestContext,
        references: &[KnowledgeReferencePo],
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for reference in references {
            sqlx::query!(
                r#"
INSERT INTO knowledge_reference (
    id, knowledge_id, short_term_id, trace_id, date_path, line_number, created_at
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#,
                reference.id,
                reference.knowledge_id,
                reference.short_term_id,
                reference.trace_id,
                reference.date_path,
                reference.line_number,
                reference.created_at,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_knowledge_references(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Vec<KnowledgeReferencePo>, AppError> {
        let pool = self.pool(ctx);
        let references = sqlx::query_as!(
            KnowledgeReferencePo,
            r#"
SELECT id, knowledge_id, short_term_id, trace_id, date_path, line_number, created_at
FROM knowledge_reference
WHERE knowledge_id = ?
ORDER BY created_at ASC
"#,
            knowledge_id
        )
        .fetch_all(&pool)
        .await?;

        Ok(references)
    }

    // ========== 知识节点关系相关 ==========

    async fn add_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);
        let relation_type_str = relation.relation_type.to_string();

        sqlx::query!(
            r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?)
"#,
            relation.id,
            relation.source_node_id,
            relation.target_node_id,
            relation_type_str,
            relation.created_at,
            relation.updated_at,
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn batch_add_knowledge_relations(
        &self,
        ctx: RequestContext,
        relations: &[KnowledgeNodeRelationPo],
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for relation in relations {
            let relation_type_str = relation.relation_type.to_string();
            sqlx::query!(
                r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?)
"#,
                relation.id,
                relation.source_node_id,
                relation.target_node_id,
                relation_type_str,
                relation.created_at,
                relation.updated_at,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_outgoing_relations(
        &self,
        ctx: RequestContext,
        source_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError> {
        let pool = self.pool(ctx);
        // sqlx 不自动映射枚举，需要手动处理
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ?
ORDER BY created_at ASC
"#,
            source_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn list_incoming_relations(
        &self,
        ctx: RequestContext,
        target_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError> {
        let pool = self.pool(ctx);
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE target_node_id = ?
ORDER BY created_at ASC
"#,
            target_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn list_all_relations_for_node(
        &self,
        ctx: RequestContext,
        node_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError> {
        let pool = self.pool(ctx);
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ? OR target_node_id = ?
ORDER BY created_at ASC
"#,
            node_id,
            node_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn delete_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation_id: &str,
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);

        sqlx::query!(
            r#"DELETE FROM knowledge_node_relation WHERE id = ?"#,
            relation_id
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn delete_all_relations_for_node(
        &self,
        ctx: RequestContext,
        node_id: &str,
    ) -> Result<(), AppError> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        // 删除所有源节点为该节点的关系
        sqlx::query!(
            r#"DELETE FROM knowledge_node_relation WHERE source_node_id = ?"#,
            node_id
        )
        .execute(&mut *tx)
        .await?;

        // 删除所有目标节点为该节点的关系
        sqlx::query!(
            r#"DELETE FROM knowledge_node_relation WHERE target_node_id = ?"#,
            node_id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn find_relations_by_type(
        &self,
        ctx: RequestContext,
        source_id: &str,
        relation_type: KnowledgeRelationType,
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError> {
        let pool = self.pool(ctx);
        let relation_type_str = relation_type.to_string();
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ? AND relation_type = ?
ORDER BY created_at ASC
"#,
            source_id,
            relation_type_str
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }
}
