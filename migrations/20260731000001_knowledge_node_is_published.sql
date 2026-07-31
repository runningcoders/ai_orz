-- 新增 is_published 字段，用于加速 published 标签查询
-- 原有的 json_each(tags) 查询走全表扫描，is_published 字段可走 B-tree 索引
ALTER TABLE long_term_knowledge_node ADD COLUMN is_published INTEGER NOT NULL DEFAULT 0;

-- 创建索引（部分索引：仅对已发布节点建索引，体积小且命中率高）
CREATE INDEX IF NOT EXISTS idx_ltkn_is_published ON long_term_knowledge_node(is_published) WHERE is_published = 1;

-- 回填已有数据：从 tags JSON 数组中提取 published 标签
UPDATE long_term_knowledge_node
SET is_published = 1
WHERE EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = 'published');
