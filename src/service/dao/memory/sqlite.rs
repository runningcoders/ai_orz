//! Memory DAO SQLite 实现
//!
//! 负责：
//! - 短期记忆索引的增删查改（SQLite）
//! - 记忆追踪文件的写入（每日文件追加）
//! - 原始记忆不可修改不可删除，只能追加

use crate::error::AppError;
use crate::models::memory::{MemoryTrace, ShortTermMemoryIndexPo, LongTermKnowledgeNodePo, KnowledgeRelation, KnowledgeReferencePo};
use crate::pkg::storage;
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryDaoTrait;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json;
use std::fs::{OpenOptions, write};
use std::io::Seek;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

/// SQLite Memory DAO 实现
pub struct SqliteMemoryDao;

impl SqliteMemoryDao {
    /// 创建新的 DAO 实例
    pub fn new() -> Self {
        Self
    }

    /// 获取数据目录根路径
    fn data_root(&self) -> PathBuf {
        // 相对于数据库目录
        let db_path = storage::get().db_path();
        let db_dir = Path::new(db_path).parent().unwrap_or(Path::new("data"));
        db_dir.join("long_term_memory")
    }

    /// 获取 Agent 目录
    fn agent_dir(&self, agent_id: &str) -> PathBuf {
        let root = self.data_root();
        root.join(agent_id)
    }

    /// 获取今日日期文件路径
    fn today_path(&self, agent_id: &str) -> PathBuf {
        let now = chrono::Local::now();
        let date_str = now.format("%Y-%m-%d").to_string();
        let agent_dir = self.agent_dir(agent_id);
        agent_dir.join(format!("{}.md", date_str))
    }
}

impl MemoryDaoTrait for SqliteMemoryDao {
    fn append_memory_trace(
        &self,
        _ctx: RequestContext,
        trace: &MemoryTrace,
        summary: String,
        tags: Vec<String>,
    ) -> Result<ShortTermMemoryIndexPo, AppError> {
        // 1. 确保 Agent 目录存在
        let agent_dir = self.agent_dir(&trace.agent_id);
        std::fs::create_dir_all(&agent_dir)?;

        // 2. 获取今日文件路径
        let file_path = self.today_path(&trace.agent_id);
        let date_path = format!(
            "long_term_memory/{}/{}.md",
            trace.agent_id,
            chrono::Local::now().format("%Y-%m-%d")
        );

        // 3. 追加写入文件
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
        {
            Ok(file) => file,
            Err(e) => return Err(AppError::Io(e)),
        };

        // 获取当前文件大小（就是我们要写入的起始偏移）
        let byte_start = file.seek(SeekFrom::End(0))?;
        let content_md = trace.to_markdown();
        let byte_length = content_md.len() as u64;

        // 写入 markdown
        write(&mut file, content_md)?;

        // 4. 插入短期索引到 SQLite
        let conn = storage::get().conn();

        let tags_json = serde_json::to_string(&tags)?;
        let role_str = trace.role.to_string();
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            r#"
INSERT INTO short_term_memory_index (
    id, agent_id, role, summary, tags, date_path, byte_start, byte_length, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
"#,
            params![
                trace.id,
                trace.agent_id,
                role_str,
                summary,
                tags_json,
                date_path,
                byte_start,
                byte_length,
                trace.created_at,
                now,
            ],
        )?;

        // 5. 返回索引
        Ok(ShortTermMemoryIndexPo {
            id: trace.id.clone(),
            agent_id: trace.agent_id.clone(),
            role: role_str,
            summary,
            tags,
            date_path,
            byte_start,
            byte_length,
            created_at: trace.created_at,
            updated_at: now,
        })
    }

    fn get_short_term_index(
        &self,
        _ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ShortTermMemoryIndexPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn.prepare(
            r#"
SELECT id, agent_id, role, summary, tags, date_path, byte_start, byte_length, created_at, updated_at
FROM short_term_memory_index
WHERE id = ?1
"#,
        )?;

        let result = stmt.query_row(params![id], |row| {
            let tags_json: String = row.get(4)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json)?;

            Ok(ShortTermMemoryIndexPo {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                role: row.get(2)?,
                summary: row.get(3)?,
                tags,
                date_path: row.get(5)?,
                byte_start: row.get(6)?,
                byte_length: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        }).optional()?;

        Ok(result)
    }

    fn list_short_term_by_agent(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn.prepare(
            r#"
SELECT id, agent_id, role, summary, tags, date_path, byte_start, byte_length, created_at, updated_at
FROM short_term_memory_index
WHERE agent_id = ?1
ORDER BY created_at DESC
LIMIT ?2
"#,
        )?;

        let rows = stmt.query_map(params![agent_id, limit as i64], |row| {
            let tags_json: String = row.get(4)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json)?;

            Ok(ShortTermMemoryIndexPo {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                role: row.get(2)?,
                summary: row.get(3)?,
                tags,
                date_path: row.get(5)?,
                byte_start: row.get(6)?,
                byte_length: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    fn search_short_term(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError> {
        let conn = storage::get().conn();

        // 使用 SQLite 全文检索
        let mut stmt = conn.prepare(
            r#"
SELECT id, agent_id, role, summary, tags, date_path, byte_start, byte_length, created_at, updated_at
FROM short_term_memory_index
WHERE agent_id = ?1 AND summary MATCH ?2
ORDER BY rank
LIMIT ?3
"#,
        )?;

        let rows = stmt.query_map(params![agent_id, query, limit as i64], |row| {
            let tags_json: String = row.get(4)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json)?;

            Ok(ShortTermMemoryIndexPo {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                role: row.get(2)?,
                summary: row.get(3)?,
                tags,
                date_path: row.get(5)?,
                byte_start: row.get(6)?,
                byte_length: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    fn read_memory_content(&self, index: &ShortTermMemoryIndexPo) -> Result<String, AppError> {
        // 数据目录根路径就是数据库所在目录
        let db_path = storage::get().db_path();
        let db_dir = Path::new(db_path).parent().unwrap_or(Path::new("data"));
        let full_path = db_dir.join(&index.date_path);

        // 打开文件，seek 到偏移，读取指定长度
        let mut file = std::fs::File::open(&full_path)?;
        file.seek(SeekFrom::Start(index.byte_start))?;

        let mut buffer = vec![0u8; index.byte_length as usize];
        std::io::Read::read_exact(&mut file, &mut buffer)?;

        let content = String::from_utf8(buffer)?;
        Ok(content)
    }

    // ========== 长期知识图谱相关 ==========

    fn save_knowledge_node(
        &self,
        _ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<(), AppError> {
        let conn = storage::get().conn();

        // 先试试更新，如果不存在就插入
        let rows_affected = conn.execute(
            r#"
UPDATE long_term_knowledge_node
SET agent_id = ?1,
    node_name = ?2,
    node_description = ?3,
    node_type = ?4,
    summary = ?5,
    relations = ?6,
    updated_at = ?7
WHERE id = ?8
"#,
            params![
                node.agent_id,
                node.node_name,
                node.node_description,
                node.node_type,
                node.summary,
                serde_json::to_string(&node.relations)?,
                node.updated_at,
                node.id,
            ],
        )?;

        if rows_affected == 0 {
            // 不存在，插入新节点
            conn.execute(
                r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
"#,
                params![
                    node.id,
                    node.agent_id,
                    node.node_name,
                    node.node_description,
                    node.node_type,
                    node.summary,
                    serde_json::to_string(&node.relations)?,
                    node.created_at,
                    node.updated_at,
                ],
            )?;
        }

        Ok(())
    }

    fn batch_save_knowledge_nodes(
        &self,
        _ctx: RequestContext,
        nodes: &[LongTermKnowledgeNodePo],
    ) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let tx = conn.transaction()?;

        for node in nodes {
            let rows_affected = tx.execute(
                r#"
UPDATE long_term_knowledge_node
SET agent_id = ?1,
    node_name = ?2,
    node_description = ?3,
    node_type = ?4,
    summary = ?5,
    relations = ?6,
    updated_at = ?7
WHERE id = ?8
"#,
                params![
                    node.agent_id,
                    node.node_name,
                    node.node_description,
                    node.node_type,
                    node.summary,
                    serde_json::to_string(&node.relations)?,
                    node.updated_at,
                    node.id,
                ],
            )?;

            if rows_affected == 0 {
                tx.execute(
                    r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
"#,
                    params![
                        node.id,
                        node.agent_id,
                        node.node_name,
                        node.node_description,
                        node.node_type,
                        node.summary,
                        serde_json::to_string(&node.relations)?,
                        node.created_at,
                        node.updated_at,
                    ],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    fn get_knowledge_node(
        &self,
        _ctx: RequestContext,
        id: &str,
    ) -> Result<Option<LongTermKnowledgeNodePo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn.prepare(
            r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
FROM long_term_knowledge_node
WHERE id = ?1
"#,
        )?;

        let result = stmt.query_row(params![id], |row| {
            let relations_json: String = row.get(6)?;
            let relations: Vec<KnowledgeRelation> = serde_json::from_str(&relations_json)?;

            Ok(LongTermKnowledgeNodePo {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                node_name: row.get(2)?,
                node_description: row.get(3)?,
                node_type: row.get(4)?,
                summary: row.get(5)?,
                relations,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        }).optional()?;

        Ok(result)
    }

    fn list_knowledge_nodes_by_agent(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        node_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>, AppError> {
        let conn = storage::get().conn();

        let (sql, params) = match node_type {
            Some(node_type) => (
                r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
FROM long_term_knowledge_node
WHERE agent_id = ?1 AND node_type = ?2
ORDER BY updated_at DESC
LIMIT ?3
"#,
                vec![agent_id.to_string(), node_type.to_string(), limit.to_string()],
            ),
            None => (
                r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
FROM long_term_knowledge_node
WHERE agent_id = ?1
ORDER BY updated_at DESC
LIMIT ?2
"#,
                vec![agent_id.to_string(), limit.to_string()],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;

        let params = params.iter().map(|s| s.as_str()).collect::<Vec<_>>();

        let rows = stmt.query_map(&params, |row| {
            let relations_json: String = row.get(6)?;
            let relations: Vec<KnowledgeRelation> = serde_json::from_str(&relations_json)?;

            Ok(LongTermKnowledgeNodePo {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                node_name: row.get(2)?,
                node_description: row.get(3)?,
                node_type: row.get(4)?,
                summary: row.get(5)?,
                relations,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    fn search_knowledge_nodes(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn.prepare(
            r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
FROM long_term_knowledge_node
WHERE agent_id = ?1 AND (node_name MATCH ?2 OR summary MATCH ?2)
ORDER BY rank
LIMIT ?3
"#,
        )?;

        let rows = stmt.query_map(params![agent_id, query, limit as i64], |row| {
            let relations_json: String = row.get(6)?;
            let relations: Vec<KnowledgeRelation> = serde_json::from_str(&relations_json)?;

            Ok(LongTermKnowledgeNodePo {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                node_name: row.get(2)?,
                node_description: row.get(3)?,
                node_type: row.get(4)?,
                summary: row.get(5)?,
                relations,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    fn delete_knowledge_node(
        &self,
        _ctx: RequestContext,
        id: &str,
    ) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let tx = conn.transaction()?;

        // 先删除相关引用
        tx.execute(
            r#"DELETE FROM knowledge_reference WHERE knowledge_id = ?1"#,
            params![id],
        )?;

        // 再删除节点
        tx.execute(
            r#"DELETE FROM long_term_knowledge_node WHERE id = ?1"#,
            params![id],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn add_knowledge_reference(
        &self,
        _ctx: RequestContext,
        reference: &KnowledgeReferencePo,
    ) -> Result<(), AppError> {
        let conn = storage::get().conn();

        conn.execute(
            r#"
INSERT INTO knowledge_reference (id, knowledge_id, short_term_id, created_at)
VALUES (?1, ?2, ?3, ?4)
"#,
            params![
                reference.id,
                reference.knowledge_id,
                reference.short_term_id,
                reference.created_at,
            ],
        )?;

        Ok(())
    }

    fn batch_add_knowledge_references(
        &self,
        _ctx: RequestContext,
        references: &[KnowledgeReferencePo],
    ) -> Result<(), AppError> {
        let conn = storage::get().conn();
        let tx = conn.transaction()?;

        for reference in references {
            tx.execute(
                r#"
INSERT INTO knowledge_reference (id, knowledge_id, short_term_id, created_at)
VALUES (?1, ?2, ?3, ?4)
"#,
                params![
                    reference.id,
                    reference.knowledge_id,
                    reference.short_term_id,
                    reference.created_at,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn list_knowledge_references(
        &self,
        _ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Vec<KnowledgeReferencePo>, AppError> {
        let conn = storage::get().conn();

        let mut stmt = conn.prepare(
            r#"
SELECT id, knowledge_id, short_term_id, created_at
FROM knowledge_reference
WHERE knowledge_id = ?1
ORDER BY created_at ASC
"#,
        )?;

        let rows = stmt.query_map(params![knowledge_id], |row| {
            Ok(KnowledgeReferencePo {
                id: row.get(0)?,
                knowledge_id: row.get(1)?,
                short_term_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }
}
