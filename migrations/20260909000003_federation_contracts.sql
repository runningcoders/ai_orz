-- S3 合约授权（联邦鉴权升级方案 §六 S3）：把连接级白名单具象化为合约
--
-- 新建 federation_contracts：能力集的唯一事实源（SSOT 约束——能力由合约派生，
-- 不得另行配置）。本迁移只立合约的形状、不立合约的语义：
-- kind=basic / state=active|terminated；未来非互信合同（task_sla）= 同一张表
-- 加字段与状态机，双签复用 S2 签名代码。
--
-- 存量 organization_links.capabilities 一次性下沉：每条连接回填一条 basic 合约
-- （能力集原样保留，管理员已配置的白名单不丢），随后删除 links.capabilities 列
-- （两套并行配置必然漂移，见方案 §五）。

CREATE TABLE federation_contracts (
    -- 注意：SQLite 只有 INTEGER PRIMARY KEY 隐式非空，TEXT PRIMARY KEY 必须显式
    -- NOT NULL，否则 sqlx 依 decltype 推断该列可空，query_as! 映射 String 报错
    id TEXT PRIMARY KEY NOT NULL,
    local_org_id TEXT NOT NULL,
    peer_org_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'basic',
    state INTEGER NOT NULL DEFAULT 1,
    capabilities TEXT NOT NULL DEFAULT '["a2a_task"]',
    terms_hash TEXT NOT NULL DEFAULT 'basic',
    local_signature TEXT NOT NULL DEFAULT '',
    peer_signature TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (local_org_id, peer_org_id, kind)
);

CREATE INDEX idx_federation_contracts_pair ON federation_contracts (local_org_id, peer_org_id);

-- 存量回填：每条已存在连接 → 一条 basic 合约（state=1 active；断联连接的合约
-- 仍回填为 active，其入站在验签步即被 Active 连接检查拦截，与现状语义一致）
INSERT INTO federation_contracts (id, local_org_id, peer_org_id, kind, state, capabilities,
                                  terms_hash, local_signature, peer_signature, created_at, updated_at)
SELECT lower(hex(randomblob(16))),
       local_org_id,
       peer_org_id,
       'basic',
       1,
       capabilities,
       'basic',
       '',
       '',
       created_at,
       updated_at
FROM organization_links;

-- 能力集存储位置迁移完成，删除旧列（SQLite >= 3.35 支持 DROP COLUMN）
ALTER TABLE organization_links DROP COLUMN capabilities;
