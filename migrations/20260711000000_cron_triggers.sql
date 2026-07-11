-- Cron 触发器表
-- 管理定时触发的任务配置。

CREATE TABLE IF NOT EXISTS cron_triggers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    trigger_type INTEGER NOT NULL CHECK (trigger_type IN (0, 1, 2)), -- 0=Once, 1=Cron, 2=Interval
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
    updated_by TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_cron_triggers_next_run_at ON cron_triggers(next_run_at);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_is_enabled ON cron_triggers(is_enabled);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_trigger_type ON cron_triggers(trigger_type);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_created_at ON cron_triggers(created_at DESC);
