-- 向量索引元数据表
-- 所有向量集合共享，存储 source_id -> rowid 映射、内容哈希、过期时间等
CREATE TABLE IF NOT EXISTS vector_metadata (
    collection TEXT NOT NULL,
    source_id TEXT NOT NULL,
    content_hash TEXT,
    model TEXT,
    dimensions INTEGER,
    indexed_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expire_at INTEGER,
    PRIMARY KEY (collection, source_id)
) STRICT;

-- 索引：按过期时间快速查询需要清理的向量
CREATE INDEX IF NOT EXISTS idx_vector_metadata_expire ON vector_metadata(expire_at);
