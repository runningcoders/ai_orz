-- 本体骨架一期：类-关系-类三元组一等公民 + 实体类继承链
--
-- 背景：
--   本体已有实体类 / 关系类型 / 属性定义 / 同义映射四张 TBox 词表，但「哪类实体之间
--   能通过哪条关系相连」只以两个 JSON 数组（domain_classes / range_classes）压在
--   ontology_relation_types 行内，无法按三元组查询、无法表达基数/必填、不利于维护。
--   本期把骨架升级为独立表，并给实体类补父类指针，支持沿继承链解析有效约束。
--
-- 关键设计决策：
--   1) 三元组表独立成表（ontology_class_relations），不再把 domain/range 压进 JSON：
--      每条边是「主语类 → 关系 → 宾语类」的一等公民，可按任意一侧反查，
--      并承载 cardinality / required 等边级约束。
--   2) 空约束语义：某关系没有任何三元组 = 完全不约束；只有一侧为空时用通配符
--      '*' 参与笛卡尔积，表示该侧不限类。解析侧对 '*' 做通配匹配。
--   3) 继承链：ontology_classes.parent_term_key 指向父类 term_key（可空 = 根类），
--      有效约束沿祖先链向上解析（common 纯函数，零物化）；环检测由 Domain 写侧保证。
--   4) 无外键：与既有本体词表一致，term_key 是语义锚点，归属与完整性由应用层解释。
--   5) 无存量包袱（项目处于测试阶段）：先建新表 + 加父类列 → 回填旧 JSON 约束 →
--      重建关系类型表去掉 domain/range 两列（保留 direction / multiplicity /
--      weight_base / inverse_key / status / 时间戳与活跃词唯一索引）。
--
-- 归一化契约：term_key 由 DAO 写入侧单点归一（trim + ASCII 小写，见
-- common::ontology::normalize）后落库，本迁移不承担归一职责。
-- 作用域：与既有 3 张本体表一致，全局表不带 organization_id（部署单元即作用域）。

-- ============================================================
-- 1. 本体骨架三元组表（TBox：类-关系-类）
-- ============================================================

CREATE TABLE ontology_class_relations (
    id                  TEXT PRIMARY KEY,
    -- 主语实体类 term_key
    subject_class_key   TEXT NOT NULL,
    -- 关系类型 term_key
    relation_key        TEXT NOT NULL,
    -- 宾语实体类 term_key（'*' 表示不限）
    object_class_key    TEXT NOT NULL,
    -- 边基数：single（同类同关系最多一条）/ many（默认）
    cardinality         TEXT NOT NULL DEFAULT 'many',
    -- 是否必填：0 可选 / 1 必填
    "required"          INTEGER NOT NULL DEFAULT 0,
    -- 条目状态（1 正常 / 0 退役；退役 ≠ 删除）
    "status"            INTEGER NOT NULL DEFAULT 1,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    CHECK (cardinality IN ('single', 'many')),
    CHECK ("required" IN (0, 1)),
    CHECK ("status" IN (0, 1)),
    CHECK (length(subject_class_key) > 0 AND length(subject_class_key) <= 256),
    CHECK (length(relation_key) > 0 AND length(relation_key) <= 256),
    CHECK (length(object_class_key) > 0 AND length(object_class_key) <= 256)
) STRICT;

-- 同一主语-关系-宾语只允许一条生效三元组
CREATE UNIQUE INDEX idx_ontology_class_relations_unique
    ON ontology_class_relations (subject_class_key, relation_key, object_class_key);

-- 按关系类型反查（骨架图按边类型过滤 / 运行时约束匹配入口）
CREATE INDEX idx_ontology_class_relations_relation
    ON ontology_class_relations (relation_key);

-- 按宾语类反查（「谁能指向我」）
CREATE INDEX idx_ontology_class_relations_object
    ON ontology_class_relations (object_class_key);

-- ============================================================
-- 2. 实体类继承链：parent_term_key
-- ============================================================

ALTER TABLE ontology_classes ADD COLUMN parent_term_key TEXT
    CHECK (parent_term_key IS NULL
           OR (length(parent_term_key) > 0 AND length(parent_term_key) <= 256));

-- ============================================================
-- 3. 回填：旧 JSON 约束 → 三元组
-- ============================================================
--
-- 语义映射：
--   - 两侧都为空 → 不插入任何行（该关系不约束，保持「无三元组 = 不约束」）；
--   - 仅一侧为空 → 该侧用 '*' 通配；
--   - 两侧非空 → 笛卡尔积展开。

INSERT INTO ontology_class_relations (
    id, subject_class_key, relation_key, object_class_key,
    cardinality, "required", "status", created_at, updated_at
)
SELECT
    lower(hex(randomblob(16))),
    triples.subject_class_key,
    triples.relation_key,
    triples.object_class_key,
    'many',
    0,
    1,
    CAST(strftime('%s', 'now') AS INTEGER),
    CAST(strftime('%s', 'now') AS INTEGER)
FROM (
    -- 两侧非空：笛卡尔积
    SELECT d.value AS subject_class_key,
           r.term_key AS relation_key,
           g.value AS object_class_key
    FROM ontology_relation_types AS r,
         json_each(r.domain_classes) AS d,
         json_each(r.range_classes) AS g
    WHERE json_array_length(r.domain_classes) > 0
      AND json_array_length(r.range_classes) > 0

    UNION ALL

    -- 仅 domain 为空：主语通配
    SELECT '*' AS subject_class_key,
           r.term_key AS relation_key,
           g.value AS object_class_key
    FROM ontology_relation_types AS r,
         json_each(r.range_classes) AS g
    WHERE json_array_length(r.domain_classes) = 0
      AND json_array_length(r.range_classes) > 0

    UNION ALL

    -- 仅 range 为空：宾语通配
    SELECT d.value AS subject_class_key,
           r.term_key AS relation_key,
           '*' AS object_class_key
    FROM ontology_relation_types AS r,
         json_each(r.domain_classes) AS d
    WHERE json_array_length(r.domain_classes) > 0
      AND json_array_length(r.range_classes) = 0
) AS triples;

-- ============================================================
-- 4. 重建关系类型表：去掉 domain_classes / range_classes
-- ============================================================

CREATE TABLE ontology_relation_types_new (
    id               TEXT PRIMARY KEY,
    term_key         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    description      TEXT NOT NULL,
    weight_base      REAL NOT NULL DEFAULT 1.0,
    inverse_key      TEXT,
    direction        TEXT NOT NULL DEFAULT 'undirected'
                     CHECK (direction IN ('directed', 'undirected')),
    multiplicity     TEXT,
    "status"         INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    CHECK ("status" IN (0, 1)),
    CHECK (length(term_key) > 0 AND length(term_key) <= 256)
) STRICT;

INSERT INTO ontology_relation_types_new (
    id, term_key, display_name, description, weight_base, inverse_key,
    direction, multiplicity, "status", created_at, updated_at
)
SELECT
    id, term_key, display_name, description, weight_base, inverse_key,
    direction, multiplicity, "status", created_at, updated_at
FROM ontology_relation_types;

DROP TABLE ontology_relation_types;

ALTER TABLE ontology_relation_types_new RENAME TO ontology_relation_types;

-- 仅活跃词查重：退役（status = 0）词不阻塞同名规范词重建
CREATE UNIQUE INDEX idx_ontology_relation_types_term_key_active
    ON ontology_relation_types (term_key) WHERE "status" = 1;
