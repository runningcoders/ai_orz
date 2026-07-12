-- Memory FTS5 全文索引迁移
-- 为短期记忆（short_term_memory_index）和知识节点（long_term_knowledge_node）
-- 创建 FTS5 全文索引虚拟表，并配置触发器自动同步。
-- 分词器：trigram（支持中文/英文混合全文搜索，基于三字符子串匹配）
--
-- 主表均为 STRICT 表且未声明 WITHOUT ROWID，因此具有隐式 rowid（INTEGER）。
-- FTS5 虚拟表自带 rowid，触发器通过 rowid 关联主表与索引表。
--
-- DELETE/UPDATE 触发器使用 `DELETE FROM fts WHERE rowid = old.rowid` 而非
-- FTS5 的 'delete' 特殊命令，以避免值不匹配导致的 SQL logic error。

-- ============================================================
-- 1. FTS5 虚拟表
-- ============================================================

-- 短期记忆全文索引（summary + tags）
CREATE VIRTUAL TABLE IF NOT EXISTS short_term_memory_fts USING fts5(
    summary,
    tags,
    tokenize = 'trigram'
);

-- 知识节点全文索引（node_name + summary + node_description）
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_node_fts USING fts5(
    node_name,
    summary,
    node_description,
    tokenize = 'trigram'
);

-- ============================================================
-- 2. 触发器：短期记忆同步（short_term_memory_index -> short_term_memory_fts）
-- ============================================================

-- AFTER INSERT：新增主表行时同步写入 FTS
CREATE TRIGGER IF NOT EXISTS short_term_memory_fts_ai
AFTER INSERT ON short_term_memory_index BEGIN
    INSERT INTO short_term_memory_fts(rowid, summary, tags)
    VALUES (new.rowid, new.summary, new.tags);
END;

-- AFTER DELETE：删除主表行时从 FTS 移除
CREATE TRIGGER IF NOT EXISTS short_term_memory_fts_ad
AFTER DELETE ON short_term_memory_index BEGIN
    DELETE FROM short_term_memory_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE：更新主表行时先删除旧 FTS 条目再插入新条目
CREATE TRIGGER IF NOT EXISTS short_term_memory_fts_au
AFTER UPDATE ON short_term_memory_index BEGIN
    DELETE FROM short_term_memory_fts WHERE rowid = old.rowid;
    INSERT INTO short_term_memory_fts(rowid, summary, tags)
    VALUES (new.rowid, new.summary, new.tags);
END;

-- ============================================================
-- 3. 触发器：知识节点同步（long_term_knowledge_node -> knowledge_node_fts）
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_ai
AFTER INSERT ON long_term_knowledge_node BEGIN
    INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description)
    VALUES (new.rowid, new.node_name, new.summary, new.node_description);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_ad
AFTER DELETE ON long_term_knowledge_node BEGIN
    DELETE FROM knowledge_node_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_au
AFTER UPDATE ON long_term_knowledge_node BEGIN
    DELETE FROM knowledge_node_fts WHERE rowid = old.rowid;
    INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description)
    VALUES (new.rowid, new.node_name, new.summary, new.node_description);
END;

-- ============================================================
-- 4. 存量数据回填
-- ============================================================

-- 回填短期记忆 FTS 索引（迁移前已存在的数据）
INSERT INTO short_term_memory_fts(rowid, summary, tags)
SELECT rowid, summary, tags FROM short_term_memory_index;

-- 回填知识节点 FTS 索引（迁移前已存在的数据）
INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description)
SELECT rowid, node_name, summary, node_description FROM long_term_knowledge_node;
