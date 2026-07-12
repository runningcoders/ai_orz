-- Entity FTS5 全文索引迁移
-- 为 6 张主表（skills/tools/messages/tasks/projects/agents）创建 FTS5 全文索引虚拟表，
-- 并配置触发器自动同步。
-- 分词器：trigram（支持中文/英文混合全文搜索，基于三字符子串匹配）
--
-- 主表均为 STRICT 表且未声明 WITHOUT ROWID，因此具有隐式 rowid（INTEGER）。
-- FTS5 虚拟表自带 rowid，触发器通过 rowid 关联主表与索引表。
--
-- DELETE/UPDATE 触发器使用 `DELETE FROM fts WHERE rowid = old.rowid` 而非
-- FTS5 的 'delete' 特殊命令，以避免值不匹配导致的 SQL logic error。
--
-- 注意：agents 表不索引 soul 字段（soul 是 Agent 灵魂设定，不适合搜索）。

-- ============================================================
-- 1. FTS5 虚拟表
-- ============================================================

-- skills 全文索引（name + description + tags）
CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
    name,
    description,
    tags,
    tokenize = 'trigram'
);

-- tools 全文索引（name + description + tags）
CREATE VIRTUAL TABLE IF NOT EXISTS tools_fts USING fts5(
    name,
    description,
    tags,
    tokenize = 'trigram'
);

-- messages 全文索引（content）
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    tokenize = 'trigram'
);

-- tasks 全文索引（title + description + tags）
CREATE VIRTUAL TABLE IF NOT EXISTS tasks_fts USING fts5(
    title,
    description,
    tags,
    tokenize = 'trigram'
);

-- projects 全文索引（name + description + workflow + guidance + tags）
CREATE VIRTUAL TABLE IF NOT EXISTS projects_fts USING fts5(
    name,
    description,
    workflow,
    guidance,
    tags,
    tokenize = 'trigram'
);

-- agents 全文索引（name + role + description + capabilities）
-- 注意：不索引 soul 字段（Agent 灵魂设定不适合搜索）
CREATE VIRTUAL TABLE IF NOT EXISTS agents_fts USING fts5(
    name,
    role,
    description,
    capabilities,
    tokenize = 'trigram'
);

-- ============================================================
-- 2. 触发器：skills 同步（skills -> skills_fts）
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS skills_fts_ai
AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS skills_fts_ad
AFTER DELETE ON skills BEGIN
    DELETE FROM skills_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS skills_fts_au
AFTER UPDATE ON skills BEGIN
    DELETE FROM skills_fts WHERE rowid = old.rowid;
    INSERT INTO skills_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- ============================================================
-- 3. 触发器：tools 同步（tools -> tools_fts）
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS tools_fts_ai
AFTER INSERT ON tools BEGIN
    INSERT INTO tools_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS tools_fts_ad
AFTER DELETE ON tools BEGIN
    DELETE FROM tools_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS tools_fts_au
AFTER UPDATE ON tools BEGIN
    DELETE FROM tools_fts WHERE rowid = old.rowid;
    INSERT INTO tools_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- ============================================================
-- 4. 触发器：messages 同步（messages -> messages_fts）
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS messages_fts_ai
AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content)
    VALUES (new.rowid, new.content);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS messages_fts_ad
AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS messages_fts_au
AFTER UPDATE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.rowid;
    INSERT INTO messages_fts(rowid, content)
    VALUES (new.rowid, new.content);
END;

-- ============================================================
-- 5. 触发器：tasks 同步（tasks -> tasks_fts）
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS tasks_fts_ai
AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts(rowid, title, description, tags)
    VALUES (new.rowid, new.title, new.description, new.tags);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS tasks_fts_ad
AFTER DELETE ON tasks BEGIN
    DELETE FROM tasks_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS tasks_fts_au
AFTER UPDATE ON tasks BEGIN
    DELETE FROM tasks_fts WHERE rowid = old.rowid;
    INSERT INTO tasks_fts(rowid, title, description, tags)
    VALUES (new.rowid, new.title, new.description, new.tags);
END;

-- ============================================================
-- 6. 触发器：projects 同步（projects -> projects_fts）
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS projects_fts_ai
AFTER INSERT ON projects BEGIN
    INSERT INTO projects_fts(rowid, name, description, workflow, guidance, tags)
    VALUES (new.rowid, new.name, new.description, new.workflow, new.guidance, new.tags);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS projects_fts_ad
AFTER DELETE ON projects BEGIN
    DELETE FROM projects_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS projects_fts_au
AFTER UPDATE ON projects BEGIN
    DELETE FROM projects_fts WHERE rowid = old.rowid;
    INSERT INTO projects_fts(rowid, name, description, workflow, guidance, tags)
    VALUES (new.rowid, new.name, new.description, new.workflow, new.guidance, new.tags);
END;

-- ============================================================
-- 7. 触发器：agents 同步（agents -> agents_fts）
-- 注意：不索引 soul 字段
-- ============================================================

-- AFTER INSERT
CREATE TRIGGER IF NOT EXISTS agents_fts_ai
AFTER INSERT ON agents BEGIN
    INSERT INTO agents_fts(rowid, name, role, description, capabilities)
    VALUES (new.rowid, new.name, new.role, new.description, new.capabilities);
END;

-- AFTER DELETE
CREATE TRIGGER IF NOT EXISTS agents_fts_ad
AFTER DELETE ON agents BEGIN
    DELETE FROM agents_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE
CREATE TRIGGER IF NOT EXISTS agents_fts_au
AFTER UPDATE ON agents BEGIN
    DELETE FROM agents_fts WHERE rowid = old.rowid;
    INSERT INTO agents_fts(rowid, name, role, description, capabilities)
    VALUES (new.rowid, new.name, new.role, new.description, new.capabilities);
END;

-- ============================================================
-- 8. 存量数据回填
-- ============================================================

-- 回填 skills FTS 索引
INSERT INTO skills_fts(rowid, name, description, tags)
SELECT rowid, name, description, tags FROM skills;

-- 回填 tools FTS 索引
INSERT INTO tools_fts(rowid, name, description, tags)
SELECT rowid, name, description, tags FROM tools;

-- 回填 messages FTS 索引
INSERT INTO messages_fts(rowid, content)
SELECT rowid, content FROM messages;

-- 回填 tasks FTS 索引
INSERT INTO tasks_fts(rowid, title, description, tags)
SELECT rowid, title, description, tags FROM tasks;

-- 回填 projects FTS 索引
INSERT INTO projects_fts(rowid, name, description, workflow, guidance, tags)
SELECT rowid, name, description, workflow, guidance, tags FROM projects;

-- 回填 agents FTS 索引（不含 soul 字段）
INSERT INTO agents_fts(rowid, name, role, description, capabilities)
SELECT rowid, name, role, description, capabilities FROM agents;
