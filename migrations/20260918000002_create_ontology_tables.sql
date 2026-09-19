-- 本体论知识沉淀：3 张全局词表（实体类 / 关系类型 / 同义映射）
--
-- 背景：
--   长期知识图谱的节点类型（node_type）与关系词（relation_type）目前是自由文本，
--   Agent 各写各的（"contains" / "包含" / "包含于" 指同一语义），图谱无法做跨
--   Agent 的结构化统计与质量门禁。本迁移引入共享本体词表（TBox）：
--   实体类约束节点，关系类型约束边，同义映射负责把历史写法收敛到规范词。
--
-- 为什么是全局表（无 organization_id）：
--   对齐 memory 域先例 —— 本体是「蜂巢共享语言」，同一部署内所有 Agent 用同一套
--   词汇沟通（跨组织知识交换正是本体存在的意义）；词表量级极小（个位数十词），
--   隔离收益为零，拆表成本为正。
--
-- 设计边界（详见 docs/design/ontology_knowledge_sedimentation_design.md §2.2）：
--   1) term_key 是语义锚点（snake_case 规范词），跨表/跨环境引用一律用它，
--      不用代理 id —— 多租户演进、跨库同步时代价最小；
--   2) status 取值对齐全库软删除约定：1 正常 / 0 退役。退役 ≠ 删除：
--      历史图谱中的存量引用仍需可解释（读侧照常展示，写侧不再允许新引用）；
--   3) 同义映射 UNIQUE(raw_term, target_kind)：同一旧词可以分别映射到
--      实体类与关系词（target_kind = 'class' / 'relation'），但同 Kind 唯一；
--   4) 本迁移只建表不注数据 —— 词表内容由 seed 快照（initialize_system 时）
--      与管理页 CRUD 维护，不在迁移里写死。
--
-- 步骤：
--   1) ontology_classes：实体类词表（required_fields JSON 数组 = 实体类必备
--      字段清单，是写后校验的判据之一）；
--   2) ontology_relation_types：关系类型词表（domain/range_classes JSON 数组 =
--      关系的头/尾实体类约束；weight_base = 图谱边权重的校准基线；
--      inverse_key = 逆向关系词，可空）；
--   3) ontology_synonym_mappings：同义映射（漂移修复手段：旧词/别名 → 规范词）。

-- 实体类词表（TBox：节点类型）
CREATE TABLE IF NOT EXISTS ontology_classes (
    id               TEXT PRIMARY KEY,
    term_key         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    description      TEXT NOT NULL,
    required_fields  TEXT NOT NULL DEFAULT '[]',
    "status"         INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE(term_key)
) STRICT;

-- 关系类型词表（TBox：边类型）
CREATE TABLE IF NOT EXISTS ontology_relation_types (
    id               TEXT PRIMARY KEY,
    term_key         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    description      TEXT NOT NULL,
    domain_classes   TEXT NOT NULL DEFAULT '[]',
    range_classes    TEXT NOT NULL DEFAULT '[]',
    weight_base      REAL NOT NULL DEFAULT 1.0,
    inverse_key      TEXT,
    "status"         INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE(term_key)
) STRICT;

-- 同义映射（漂移修复手段：旧词/别名 → 规范词）
CREATE TABLE IF NOT EXISTS ontology_synonym_mappings (
    id               TEXT PRIMARY KEY,
    raw_term         TEXT NOT NULL,
    target_kind      TEXT NOT NULL,
    target_key       TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    UNIQUE(raw_term, target_kind)
) STRICT;
