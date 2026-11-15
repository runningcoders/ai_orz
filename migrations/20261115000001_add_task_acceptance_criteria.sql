-- 任务验收标准（DoD）：随派单一起下发给执行人，也是项目 Owner 验收时的对照依据。
-- 空串 = 未指定标准，此时验收退化为人工判断；存量行由默认值补齐。
-- ⚠️ 只加列，不改 initial.sql：存量库必须靠增量迁移升级（见 20261114000001 同款写法）。
ALTER TABLE tasks ADD COLUMN acceptance_criteria TEXT NOT NULL DEFAULT '';
