-- organizations 表增加邀请码字段
-- SQLite ALTER TABLE 限制：不能直接加 UNIQUE 列，分两步走
ALTER TABLE organizations ADD COLUMN invite_code TEXT NULL;

-- 事后加唯一索引（条件唯一：非空的 invite_code 必须唯一，允许多个 NULL）
CREATE UNIQUE INDEX IF NOT EXISTS uq_organizations_invite_code
    ON organizations (invite_code) WHERE invite_code IS NOT NULL;
