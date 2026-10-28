-- Attachment 摘要元信息 + 关键词全文索引
--
-- 一期：Agent 在唤醒阶段识别附件后调用工具写入摘要（不含向量索引）。

ALTER TABLE attachments ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE attachments ADD COLUMN description_usage TEXT NOT NULL DEFAULT '';
ALTER TABLE attachments ADD COLUMN description_source TEXT NOT NULL DEFAULT '';
ALTER TABLE attachments ADD COLUMN description_model TEXT NOT NULL DEFAULT '';
ALTER TABLE attachments ADD COLUMN description_message_id TEXT NOT NULL DEFAULT '';
ALTER TABLE attachments ADD COLUMN described_at INTEGER NOT NULL DEFAULT 0;

CREATE VIRTUAL TABLE IF NOT EXISTS attachments_fts USING fts5(
    original_name,
    description,
    description_usage,
    purpose,
    tokenize = 'trigram'
);

-- 回填存量行
INSERT INTO attachments_fts(rowid, original_name, description, description_usage, purpose)
SELECT rowid, original_name, description, description_usage, purpose FROM attachments;

CREATE TRIGGER IF NOT EXISTS attachments_fts_ai
AFTER INSERT ON attachments BEGIN
    INSERT INTO attachments_fts(rowid, original_name, description, description_usage, purpose)
    VALUES (new.rowid, new.original_name, new.description, new.description_usage, new.purpose);
END;

CREATE TRIGGER IF NOT EXISTS attachments_fts_ad
AFTER DELETE ON attachments BEGIN
    DELETE FROM attachments_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS attachments_fts_au
AFTER UPDATE OF original_name, description, description_usage, purpose ON attachments BEGIN
    DELETE FROM attachments_fts WHERE rowid = old.rowid;
    INSERT INTO attachments_fts(rowid, original_name, description, description_usage, purpose)
    VALUES (new.rowid, new.original_name, new.description, new.description_usage, new.purpose);
END;
