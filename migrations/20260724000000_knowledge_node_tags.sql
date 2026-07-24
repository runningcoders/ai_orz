-- 长期知识节点新增 tags 字段，并将其纳入 FTS5 全文索引
--
-- 背景：
-- - 短期记忆（short_term_memory_index）已有 tags 字段并已纳入 FTS5 索引
-- - 长期知识节点此前没有 tags 字段，缺少细粒度关键词检索能力
-- - 本次为 long_term_knowledge_node 添加 tags 字段（JSON 数组字符串，默认 '[]'）
--   并重建 knowledge_node_fts 虚拟表，将 tags 列纳入索引
--
-- 注意：
-- - SQLite 不支持直接 ALTER FTS5 虚拟表的列结构，必须 DROP + CREATE 重建
-- - 触发器绑定在主表 long_term_knowledge_node 上，需先 DROP TRIGGER 再重建
-- - 主表 STRICT 表有隐式 rowid，FTS5 触发器通过 rowid 关联

-- ============================================================
-- 1. 先移除旧触发器与旧 FTS5 虚拟表
-- ============================================================

DROP TRIGGER IF EXISTS knowledge_node_fts_au;
DROP TRIGGER IF EXISTS knowledge_node_fts_ad;
DROP TRIGGER IF EXISTS knowledge_node_fts_ai;

DROP TABLE IF EXISTS knowledge_node_fts;

-- ============================================================
-- 2. 主表新增 tags 字段
-- ============================================================

ALTER TABLE long_term_knowledge_node
    ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';

-- 为 tags 添加 B-tree 索引（便于按标签过滤）
CREATE INDEX IF NOT EXISTS idx_ltkn_tags ON long_term_knowledge_node(tags);

-- ============================================================
-- 3. 重建 FTS5 虚拟表（纳入 tags 列）
-- ============================================================

CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_node_fts USING fts5(
    node_name,
    summary,
    node_description,
    tags,
    tokenize = 'trigram'
);

-- ============================================================
-- 4. 重建触发器：主表变更同步到 FTS5
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_ai
AFTER INSERT ON long_term_knowledge_node BEGIN
    INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description, tags)
    VALUES (new.rowid, new.node_name, new.summary, new.node_description, new.tags);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_ad
AFTER DELETE ON long_term_knowledge_node BEGIN
    DELETE FROM knowledge_node_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE：先删旧条目再插入新条目
CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_au
AFTER UPDATE ON long_term_knowledge_node BEGIN
    DELETE FROM knowledge_node_fts WHERE rowid = old.rowid;
    INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description, tags)
    VALUES (new.rowid, new.node_name, new.summary, new.node_description, new.tags);
END;

-- ============================================================
-- 5. 存量数据回填
-- ============================================================

INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description, tags)
SELECT rowid, node_name, summary, node_description, tags FROM long_term_knowledge_node;
