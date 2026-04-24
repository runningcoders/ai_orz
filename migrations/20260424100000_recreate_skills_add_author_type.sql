-- Recreate skills table to add author_type column
-- SQLite does not support dropping columns well, so we recreate the table

-- 1. Rename old table
ALTER TABLE skills RENAME TO skills_old;

-- 2. Create new table with author_type
CREATE TABLE skills (
    id TEXT NOT NULL PRIMARY KEY,                    -- 技能ID: "name-slug--hash" (名称slug--哈希前6位)
    name TEXT NOT NULL,                              -- 技能显示名称
    description TEXT NOT NULL DEFAULT '',            -- 技能描述：什么时候用这个技能
    tags TEXT NOT NULL DEFAULT '[]',                 -- JSON 数组：标签列表 ["产品", "PRD", "需求"]
    category TEXT NOT NULL DEFAULT '',               -- 单一分类："文档写作" / "问题解决" / "代码开发" / "研究分析"
    parent_skill_id TEXT NOT NULL DEFAULT '',        -- 父技能ID（继承来源，技能树演进）
    author_id TEXT NOT NULL DEFAULT '',              -- 创建人用户ID
    author_type INTEGER NOT NULL DEFAULT 0,          -- 创建者类型：0=User, 1=Agent
    modifier_id TEXT NOT NULL DEFAULT '',            -- 最后修改人用户ID
    status INTEGER NOT NULL DEFAULT 2,               -- 技能状态：0=已过期 1=Published 2=Draft（默认Draft）
    created_at INTEGER NOT NULL,                     -- 创建时间戳（毫秒）
    updated_at INTEGER NOT NULL,                     -- 更新时间戳（毫秒）
    content_path TEXT NOT NULL                       -- 相对 base_data_path 的技能目录路径
) STRICT;

-- 3. Copy data from old table to new table (author_type defaults to 0 = User)
INSERT INTO skills (
    id, name, description, tags, category, parent_skill_id,
    author_id, modifier_id, status, created_at, updated_at, content_path
)
SELECT
    id, name, description, tags, category, parent_skill_id,
    author_id, modifier_id, status, created_at, updated_at, content_path
FROM skills_old;

-- 4. Copy indexes
CREATE INDEX IF NOT EXISTS idx_skills_status ON skills(status);
CREATE INDEX IF NOT EXISTS idx_skills_category ON skills(category);
CREATE INDEX IF NOT EXISTS idx_skills_parent ON skills(parent_skill_id);
CREATE INDEX IF NOT EXISTS idx_skills_updated ON skills(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_skills_author ON skills(author_id);

-- 5. Drop old table
DROP TABLE skills_old;
