-- 知识节点关系：新增 status 状态列 + 「双节点间同型关系仅一条生效」约束
--
-- 背景：
--   修正一条知识关系 = 软删除旧边 + 插入新边（版本化替换）。此前表里没有状态列，
--   修正只能「先删后插」，替换前的历史边永久丢失 —— 未来做记忆因果链回放时，
--   没有办法回答「当时这两点之间是怎么连的」。
--
-- 为什么是 status 枚举而不是 boolean：
--   遗忘的记忆可以回忆起来（可恢复），状态机未来还要容纳更多档位（如撤销、休眠）。
--   取值对齐全库软删除约定：0 = 非生效（已替换），1 = 生效；
--   Rust 侧为 KnowledgeRelationStatus（见 common/src/enums/memory.rs）。
--
-- 与节点状态「不对等」的边界（刻意为之）：
--   节点遗忘（MemoryStatus = Forgotten）不联动边的 status —— 边连接两个节点，
--   其可见性取决于两端点的状态，无法用单个状态值表达；该联动由读侧派生
--   （端点不入批 → 边随批次丢弃 → 节点恢复后边自然回归，无需写恢复逻辑）。
--   边的 status 只表达边自身的版本生命周期。
--
-- 步骤：
--   1) 加列：存量行全部视为生效（DEFAULT 1，与「当前库里只有现行版本」的现实一致）；
--   2) 存量去重：同一 (source_node_id, target_node_id, relation_type) 若已有多条，
--      保留 updated_at 最新的一条（并列取 id 更大者）为生效，其余降级为 Superseded(0)
--      —— 「最新认知代表当下」，旧版本作为历史痕迹保留，因果链不丢细节；
--   3) 部分唯一索引：仅约束生效边，把「双节点间同型关系只有一条生效」从应用层
--      约定升级为数据库约束；被替换的旧边（status = 0）不参与唯一性，可无限留存。
--      键为有向三元组（source → target 不含反向），relation_type 参与唯一性：
--      同一对节点的 contains 与 prerequisite 是两条语义不同的边，互不挤占。

ALTER TABLE knowledge_node_relation ADD COLUMN "status" INTEGER NOT NULL DEFAULT 1;

UPDATE knowledge_node_relation
SET "status" = 0
WHERE "status" = 1
  AND id NOT IN (
    SELECT id FROM (
      SELECT id, ROW_NUMBER() OVER (
        PARTITION BY source_node_id, target_node_id, relation_type
        ORDER BY updated_at DESC, id DESC
      ) AS rn
      FROM knowledge_node_relation
    ) AS ranked
    WHERE rn = 1
  );

CREATE UNIQUE INDEX IF NOT EXISTS uq_knowledge_relation_active_edge
ON knowledge_node_relation (source_node_id, target_node_id, relation_type)
WHERE "status" = 1;
