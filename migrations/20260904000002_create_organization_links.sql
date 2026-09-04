-- 组织连接表（组网契约）
--
-- 承载「本组织 ↔ 对端组织」的点对点连接参数。实体（组织）与契约（连接）分离：
-- organizations 只描述组织本身，连接的 endpoint/凭证/状态放这里。
--
-- 安全考量：organizations 是被全系统高频 join 的表，凭证混入会放大泄漏面；
-- 因此 access_token / peer_token_hash 独立存放在本表。
--
-- 不变量：organizations 中 scope=Linked 的记录 ⇔ 本表存在对应
-- (local_org_id, peer_org_id, status=Active) 记录（links 是事实源，scope 是投影）。
CREATE TABLE IF NOT EXISTS organization_links (
    id TEXT NOT NULL PRIMARY KEY,
    local_org_id TEXT NOT NULL,             -- 本端组织 id
    peer_org_id TEXT NOT NULL,              -- 对端组织 id（organizations 中 scope=Linked 的影子记录）
    endpoint TEXT NOT NULL,                 -- 对端 API 基址（组网通信地址；base_url 仅用于展示）
    access_token TEXT NOT NULL,             -- 出站凭证：本端调用对端时携带（32 字节随机，hex 明文）
    peer_token_hash TEXT NOT NULL,          -- 入站校验：对端调用本端时携带凭证的 SHA-256 哈希（不存明文）
    status INTEGER NOT NULL DEFAULT 1,      -- 1=Active 0=Revoked
    created_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (local_org_id) REFERENCES organizations(id),
    FOREIGN KEY (peer_org_id) REFERENCES organizations(id)
) STRICT;

-- 一对组织之间只有一条连接（重复建联走 upsert/续联）
CREATE UNIQUE INDEX IF NOT EXISTS uq_organization_links_pair
    ON organization_links(local_org_id, peer_org_id);
CREATE INDEX IF NOT EXISTS idx_organization_links_peer ON organization_links(peer_org_id);
