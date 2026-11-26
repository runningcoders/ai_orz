-- 统一动态调度器：cron_triggers 扩展为「When / Who / What / How」四组顶层参数
--
-- 原表 CHECK (trigger_type IN (0, 1, 2)) 无法用 ALTER TABLE 放宽（SQLite 不支持
-- ALTER CONSTRAINT），因此采用重建表：建新表 → 搬运存量列 → 删旧表 → 改名。
-- trigger_type 追加 3 = Schedule（形态标签，时间规则复用 Once/Cron/Interval）。
-- behavior 默认 0 = SyncMaintenance：存量 6 条系统触发器零行为变化。
--
-- action 回填：存量记录把动作放在 payload JSON 里，搬到顶层列；消费侧保留
-- 「action 列优先、回落 payload.action」的兼容路径，老数据不会失效。
--
-- Who 组三字段分工（互不兼任）：
--   owner_user_id  —— 归属用户（可见范围 + 推送受众）
--   creator_type/creator_id —— 创建者（user / agent / system）
--   agent_id       —— 负责 Agent（唤醒目标 / 提醒发送者）
-- 不用 target_type/target_id 表达「谁执行」：多态目标里只有 Agent 被真正消费，
-- 两套字段并存会造出两个口径。

CREATE TABLE cron_triggers_new (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    trigger_type INTEGER NOT NULL CHECK (trigger_type IN (0, 1, 2, 3)),
    cron_expression TEXT,
    interval_seconds INTEGER,
    run_at INTEGER,
    next_run_at INTEGER NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    payload TEXT NOT NULL DEFAULT '{}',
    last_run_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT,
    updated_by TEXT,
    org_id TEXT,
    owner_user_id TEXT,
    creator_type INTEGER,
    creator_id TEXT,
    agent_id TEXT,
    description TEXT NOT NULL DEFAULT '',
    action TEXT,
    behavior INTEGER NOT NULL DEFAULT 0 CHECK (behavior IN (0, 1, 2)),
    start_at INTEGER,
    end_at INTEGER,
    max_runs INTEGER,
    total_runs INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    lease_owner TEXT,
    lease_until INTEGER
) STRICT;

INSERT INTO cron_triggers_new (
    id, name, trigger_type, cron_expression, interval_seconds, run_at,
    next_run_at, is_enabled, payload, last_run_at, created_at, updated_at,
    created_by, updated_by, action
)
SELECT
    id, name, trigger_type, cron_expression, interval_seconds, run_at,
    next_run_at, is_enabled, payload, last_run_at, created_at, updated_at,
    created_by, updated_by,
    CASE WHEN json_valid(payload) THEN json_extract(payload, '$.action') END
FROM cron_triggers;

DROP TABLE cron_triggers;
ALTER TABLE cron_triggers_new RENAME TO cron_triggers;

-- 重建表会连带删除原索引，这里把 4 个旧索引与 3 个新索引一起补回
CREATE INDEX IF NOT EXISTS idx_cron_triggers_next_run_at ON cron_triggers(next_run_at);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_is_enabled ON cron_triggers(is_enabled);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_trigger_type ON cron_triggers(trigger_type);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_created_at ON cron_triggers(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_due ON cron_triggers(is_enabled, next_run_at, behavior);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_owner ON cron_triggers(org_id, owner_user_id);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_agent ON cron_triggers(agent_id);
