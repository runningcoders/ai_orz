-- 用户身份凭证独立表（user_credentials）
--
-- 背景（取代 users.identity_credentials JSON 列，见 docs/design/user_credentials_design.md）：
-- - JSON 列读-改-写整列覆盖存在并发丢更新（lost update）；独立表行级 CRUD 天然消解
-- - 凭证是用户级资产：kind（TEXT 字符串枚举）/ visibility（private/public）/ is_default
--   （作用域由 visibility 派生：private=个人默认 / public=组织默认）
-- - 外部使用方绑定（Agent/工具/渠道）归使用方实体，凭据表零外部引用
-- - secret 类字段在 detail JSON 内落库前已加密（pkg::crypto::encrypt_channel_secret）
--
-- 本迁移一步到位（项目初期无生产包袱）：
-- 1. 建 user_credentials 表 + 双部分唯一索引（默认唯一性由数据库兜底）+ 常规索引
-- 2. 存量 users.identity_credentials JSON 搬迁为行记录：
--    - visibility 全部置 'private'（public 为组织共享场景预留）
--    - 三个 default_*_id 槽位迁移为对应凭据行 is_default=1（个人默认语义保留）
--    - RFC3339 时间串转毫秒时间戳
-- 3. 删除 users.identity_credentials 列

CREATE TABLE IF NOT EXISTS user_credentials (
    id TEXT PRIMARY KEY NOT NULL,                -- 凭证 ID（UUID v7，使用方引用键）
    org_id TEXT NOT NULL,                        -- 组织 ID，多租户隔离
    user_id TEXT NOT NULL,                       -- 凭证归属用户 ID（资产所有者）
    kind TEXT NOT NULL,                          -- 凭证类型（字符串枚举）：lark_app / github_token / tavily_key
    name TEXT NOT NULL,                          -- 用户自定义名称（仅展示，不参与解析）
    detail TEXT NOT NULL,                        -- 凭证详情 JSON（secret 类字段落库前已加密）
    visibility TEXT NOT NULL DEFAULT 'private',  -- 可见性（字符串枚举）：private / public
    is_default INTEGER NOT NULL DEFAULT 0,       -- 默认标记：作用域由 visibility 派生（private=个人默认 / public=组织默认）
    status INTEGER NOT NULL DEFAULT 1,           -- 软删除：1=Active, 0=Deleted
    created_by TEXT NOT NULL,                    -- 创建人 ID
    modified_by TEXT NOT NULL,                   -- 最后修改人 ID
    created_at INTEGER NOT NULL,                 -- 创建时间戳（毫秒）
    updated_at INTEGER NOT NULL                  -- 更新时间戳（毫秒）
) STRICT;

-- 默认唯一性（作用域由 visibility 派生）：
-- 个人默认：同 (user_id, kind) 最多一条 private 默认
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_credentials_default_private
ON user_credentials(user_id, kind)
WHERE is_default = 1 AND visibility = 'private' AND status = 1;

-- 组织默认：同 (org_id, kind) 最多一条 public 默认
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_credentials_default_public
ON user_credentials(org_id, kind)
WHERE is_default = 1 AND visibility = 'public' AND status = 1;

-- 常规查询索引
CREATE INDEX IF NOT EXISTS idx_user_credentials_org_id ON user_credentials(org_id);
CREATE INDEX IF NOT EXISTS idx_user_credentials_user_id ON user_credentials(user_id);
CREATE INDEX IF NOT EXISTS idx_user_credentials_kind ON user_credentials(kind);
CREATE INDEX IF NOT EXISTS idx_user_credentials_visibility ON user_credentials(visibility);
CREATE INDEX IF NOT EXISTS idx_user_credentials_status ON user_credentials(status);

-- 存量搬迁：users.identity_credentials JSON items 展开为行（幂等：INSERT OR IGNORE 主键冲突跳过）
-- 空串 / 非法 JSON / 无 items 的用户由 json_type 守卫安全跳过
INSERT OR IGNORE INTO user_credentials (
    id, org_id, user_id, kind, name, detail, visibility, is_default, status,
    created_by, modified_by, created_at, updated_at
)
SELECT
    json_extract(item.value, '$.id'),
    u.organization_id,
    u.id,
    json_extract(item.value, '$.kind'),
    json_extract(item.value, '$.name'),
    json_extract(item.value, '$.detail'),
    'private',
    CASE
        WHEN json_extract(item.value, '$.id') = json_extract(u.identity_credentials, '$.default_credential_id')
          OR json_extract(item.value, '$.id') = json_extract(u.identity_credentials, '$.default_github_credential_id')
          OR json_extract(item.value, '$.id') = json_extract(u.identity_credentials, '$.default_tavily_credential_id')
        THEN 1 ELSE 0
    END,
    1,
    u.id,
    u.id,
    -- RFC3339 → 毫秒（解析失败兜底当前时间）
    COALESCE(CAST(strftime('%s', json_extract(item.value, '$.created_at')) AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)) * 1000,
    COALESCE(CAST(strftime('%s', json_extract(item.value, '$.updated_at')) AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER)) * 1000
FROM users u,
    json_each(
        CASE
            WHEN json_type(u.identity_credentials, '$.items') = 'array' THEN u.identity_credentials
            ELSE '{"items":[]}'
        END,
        '$.items'
    ) AS item
WHERE json_valid(u.identity_credentials) AND u.identity_credentials != '';

-- 删除旧 JSON 列（存量已搬迁，无回滚窗口）
ALTER TABLE users DROP COLUMN identity_credentials;
