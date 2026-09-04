-- 组织配对码表：组网引导的短时效、单用途凭证（评审稿 §4.1）
--
-- 配对码用于「对端管理员签发 → 本地凭码调对端 verify 完成建联」的引导流程。
-- 仅存哈希（防泄漏），配合 consumed_at（单用途判定）+ expires_at（TTL 判定）。
-- 验证端（对端节点）只比对绝对时间，签发时返回 expires_at（评审稿 R4）。

CREATE TABLE IF NOT EXISTS organization_pairing_codes (
    id TEXT NOT NULL PRIMARY KEY,
    org_id TEXT NOT NULL,
    code_hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER NULL,
    created_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    FOREIGN KEY (org_id) REFERENCES organizations(id)
) STRICT;

-- 配对码哈希唯一：消费时按哈希定位单条记录
CREATE UNIQUE INDEX IF NOT EXISTS uq_organization_pairing_codes_hash
    ON organization_pairing_codes(code_hash);

CREATE INDEX IF NOT EXISTS idx_organization_pairing_codes_org
    ON organization_pairing_codes(org_id);
