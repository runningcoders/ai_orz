-- Agent 表新增 kind 列：区分本地/CLI/远程 Agent
-- 0 = Local（默认，ai_orz 内部 Brain 执行）
-- 1 = Cli（子进程包装，如 Codex）
-- 2 = Remote（A2A 协议远程调用）
ALTER TABLE agents ADD COLUMN kind INTEGER NOT NULL DEFAULT 0;
