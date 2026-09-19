-- 本体论三表重建：修复唯一索引误伤退役词 + 补齐 DB 侧约束兜底
--
-- 背景（评审 B4 + P2#10）：
--   1) 原 UNIQUE(term_key) 是全行唯一 —— 词条退役（status = 0）后，
--      重新创建同名规范词会被唯一约束拒绝，退役语义形同删除；
--   2) 全库软删除约定靠 status 列承载，但表上没有 CHECK 约束兜底，
--      非法值（如 2）可以直落；
--   3) target_kind / term_key / raw_term / target_key 均无取值与长度约束，
--      脏数据可绕过 Domain 校验直落数据库。
--
-- 处理方式（无存量包袱，项目处于测试阶段，直接 DROP + CREATE）：
--   1) 实体类 / 关系类型两表的 term_key 唯一性改为
--      partial unique index：WHERE "status" = 1 —— 仅活跃词参与查重，
--      退役词不阻塞同名规范词重建；
--   2) 新增 CHECK："status" IN (0, 1)；
--   3) 新增长度约束 length(key) <= 256（字符数，与 common::ontology
--      ::MAX_TERM_KEY_LEN 及 Domain 写侧校验对齐）；
--   4) 同义映射补 target_kind CHECK（仅 'class' / 'relation'，
--      与 TermKind::from_str 取值一致）与 raw_term / target_key 长度约束。
--
-- 归一化契约：本三表的 term_key / raw_term / target_key 由 DAO 写入侧
-- 单点归一（trim + ASCII 小写，见 common::ontology::normalize）后落库，
-- 读取侧裸列比对，本迁移不承担归一职责。

DROP TABLE IF EXISTS ontology_synonym_mappings;
DROP TABLE IF EXISTS ontology_relation_types;
DROP TABLE IF EXISTS ontology_classes;

-- 实体类词表（TBox：节点类型）
CREATE TABLE ontology_classes (
    id               TEXT PRIMARY KEY,
    term_key         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    description      TEXT NOT NULL,
    required_fields  TEXT NOT NULL DEFAULT '[]',
    "status"         INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    CHECK ("status" IN (0, 1)),
    CHECK (length(term_key) > 0 AND length(term_key) <= 256)
) STRICT;

-- 仅活跃词查重：退役（status = 0）词不阻塞同名规范词重建
CREATE UNIQUE INDEX idx_ontology_classes_term_key_active
    ON ontology_classes (term_key) WHERE "status" = 1;

-- 关系类型词表（TBox：边类型）
CREATE TABLE ontology_relation_types (
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
    CHECK ("status" IN (0, 1)),
    CHECK (length(term_key) > 0 AND length(term_key) <= 256)
) STRICT;

CREATE UNIQUE INDEX idx_ontology_relation_types_term_key_active
    ON ontology_relation_types (term_key) WHERE "status" = 1;

-- 同义映射（漂移修复手段：旧词/别名 → 规范词）
CREATE TABLE ontology_synonym_mappings (
    id               TEXT PRIMARY KEY,
    raw_term         TEXT NOT NULL,
    target_kind      TEXT NOT NULL,
    target_key       TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    UNIQUE(raw_term, target_kind),
    CHECK (target_kind IN ('class', 'relation')),
    CHECK (length(raw_term) > 0 AND length(raw_term) <= 256),
    CHECK (length(target_key) > 0 AND length(target_key) <= 256)
) STRICT;
