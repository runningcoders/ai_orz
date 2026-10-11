-- 本体属性层：属性定义表（TBox 第 4 张表）+ 通用属性值表（ABox，版本化）
--
-- 背景（本体能力盘点 2026-10-11）：
--   本体此前只有 3 张词表（实体类 / 关系类型 / 同义映射），缺失属性维度，导致：
--     1) `ontology_classes.required_fields` 只是字段名字符串数组，无类型、无值域、
--        无基数，且**零消费者**——不参与任何写后校验；
--     2) ABox 侧 `long_term_knowledge_node` **没有属性落点**，属性只能用
--        `attribute_of` / `value_of` 两条关系词「用边模拟属性」，代价是值不可索引、
--        无数据类型、无值域、无法做「查所有 model=xxx 的 agent 节点」这类结构化过滤。
--   本迁移补齐属性维度：TBox 定义「属性是什么」，ABox 存「这个实体的属性是什么值」。
--
-- 关键设计决策：
--   1) 属性值表为什么是 EAV 行存储而不是宿主表加一个 properties JSON 列：
--      - 行存储才能对数值型属性做范围查询与索引（value_num 列）；
--      - 行存储才能按 (target_type, target_id, term_key) 做版本化，JSON 单列覆盖即丢失历史；
--      - 属性数量级为个位数/实体，行存储放大可接受。
--   2) 版本化为什么用 status 软取代而不是版本号列：
--      对齐知识边既有先例（20260918000001_add_status_to_knowledge_relation）——
--      「修正 = 旧行 status=0 + 插入新行」，历史全留，可回答「当时这个值是多少」；
--      部分唯一索引保证同一 (target_type, target_id, term_key, ordinal) 只有一条生效，
--      把「一实体一属性单槽位只有当前值」从应用层约定升级为数据库约束。
--   3) 为什么四个类型化值列（text/num/bool/ref）而不是一个 value_json：
--      由 data_type 决定读哪一列，未使用的列落 NULL；这样数值属性可直接
--      WHERE value_num > ? 走索引，引用属性可直接 JOIN 回节点表。
--   4) data_type 冗余落 ABox：属性定义（TBox）退役后，存量值仍需可解释——
--      与本体词表「退役 ≠ 删除，历史存量引用仍可解释」同一口径。
--   5) 量化指标不进 DuckDB：DuckDB 在本项目是**事件流/统计旁路**（pkg/stats），
--      属性是 ABox 事实，需事务性点查 + 与节点 JOIN + 行级版本化，归 SQLite；
--      指标的「历史时」分析走既有事件流旁路，不在本迁移范围。
--   6) 属性值表为什么是**通用形态**（target_type + target_id）而不是按载体分表：
--      节点属性与边属性是同一套语义（都挂本体 term_key、都要版本化、都要范围查询），
--      分表会把这套语义复制两遍；泛化后一张表同时承载，未来新增图谱内载体只动枚举
--      与 CHECK 约束，不必再迁移。代价是 `target_id` 失去单一归属表——但本项目
--      ABox 表（knowledge_node_relation 的 source/target_node_id）本就**裸列无外键**，
--      归属由应用层解释，泛化不新增一致性债务。
--      ⚠️ 作用域边界：target_type 白名单只含图谱域内载体（node / relation）。
--      **不扩展到 agent / task / project** —— 那些表带 organization_id，而图谱与
--      本体表一律不带（部署单元即作用域），混进一张表会撕裂作用域约定。
--   7) 属性定义为什么需要 applies_to：同一 term_key 挂节点与挂边可能不是同一语义
--      （如「生效时间」修饰边 vs 修饰节点含义不同）。applies_to 在 TBox 侧声明
--      该属性允许挂哪些载体，写侧据此拦截，避免泛化后 term_key 命名空间漂移。
--      约束由应用层执行（JSON 数组内容无法用 CHECK 表达），取值集合同 target_type。
--
-- 归一化契约：term_key 由 DAO 写入侧单点归一（trim + ASCII 小写，
-- 见 common::ontology::normalize）后落库，读取侧裸列比对，本迁移不承担归一职责。
-- 作用域：与既有 3 张本体表一致，全局表不带 organization_id（部署单元即作用域）。

-- ============================================================
-- 1. 属性定义表（TBox：属性类型）
-- ============================================================

CREATE TABLE IF NOT EXISTS ontology_properties (
    id              TEXT PRIMARY KEY,
    term_key        TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    -- 适用实体类 term_key 清单（JSON 数组；空数组 = 不限定实体类）
    domain_classes  TEXT NOT NULL DEFAULT '[]',
    -- 允许挂载的载体类型（JSON 数组，取值子集：node / relation；见决策 #7）
    applies_to      TEXT NOT NULL DEFAULT '["node"]',
    -- 值类型：string | number | boolean | date | reference | enum
    data_type       TEXT NOT NULL DEFAULT 'string',
    -- 取值约束（JSON）：enum 型为取值白名单数组，number 型为 {"min":..,"max":..}，
    -- 其余类型为空对象；空串视为无约束
    value_range     TEXT NOT NULL DEFAULT '{}',
    -- 基数：single（单值，ordinal 恒 0）| many（多值，ordinal 递增）
    cardinality     TEXT NOT NULL DEFAULT 'single',
    -- 计量单位（可空；仅 number 型有意义，如 "ms" / "percent"）
    unit            TEXT,
    -- 是否必填：0 可选 / 1 必填（写后校验判据之一）
    "required"      INTEGER NOT NULL DEFAULT 0,
    "status"        INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    CHECK ("status" IN (0, 1)),
    CHECK ("required" IN (0, 1)),
    CHECK (data_type IN ('string', 'number', 'boolean', 'date', 'reference', 'enum')),
    CHECK (cardinality IN ('single', 'many')),
    CHECK (length(term_key) > 0 AND length(term_key) <= 256)
) STRICT;

-- 仅活跃词查重：退役（status = 0）词不阻塞同名规范词重建
CREATE UNIQUE INDEX IF NOT EXISTS idx_ontology_properties_term_key_active
    ON ontology_properties (term_key) WHERE "status" = 1;

-- ============================================================
-- 2. 通用属性值表（ABox：属性值，版本化）
--
-- 图谱域内通用：target_type 决定 target_id 指向节点表还是边表（见决策 #6）。
-- ============================================================

CREATE TABLE IF NOT EXISTS knowledge_property (
    id              TEXT PRIMARY KEY,
    -- 挂载载体类型：node → long_term_knowledge_node.id / relation → knowledge_node_relation.id
    target_type     TEXT NOT NULL DEFAULT 'node',
    -- 载体 ID（裸列无外键，归属由 target_type 解释；ABox 既有惯例）
    target_id       TEXT NOT NULL,
    -- 属性语义锚点，指向 ontology_properties.term_key（非 id）
    term_key        TEXT NOT NULL,
    -- 值类型冗余落库：属性定义退役后存量值仍可解释（决策 #4）
    data_type       TEXT NOT NULL,
    -- 多值槽位序号：single 型恒 0；many 型从 0 递增
    ordinal         INTEGER NOT NULL DEFAULT 0,
    -- 四选一值列，由 data_type 决定读哪一列，未使用列落 NULL
    value_text      TEXT,
    value_num       REAL,
    value_bool      INTEGER,
    -- reference 型：指向另一个知识节点 id 或外部实体标识
    value_ref       TEXT,
    -- 版本生命周期（与知识边 `KnowledgeRelationStatus` 同一口径）：
    --   1 生效 / 0 被取代（修正时旧行降级，历史保留）/ 2 已删除（用户显式软删除，可恢复）
    "status"        INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    CHECK (target_type IN ('node', 'relation')),
    CHECK ("status" IN (0, 1, 2)),
    CHECK (ordinal >= 0),
    CHECK (value_bool IS NULL OR value_bool IN (0, 1)),
    CHECK (data_type IN ('string', 'number', 'boolean', 'date', 'reference', 'enum')),
    CHECK (length(term_key) > 0 AND length(term_key) <= 256)
) STRICT;

-- 版本化核心约束：同一载体的同一属性同一槽位，只有一条生效行
CREATE UNIQUE INDEX IF NOT EXISTS uq_knowledge_property_active
    ON knowledge_property (target_type, target_id, term_key, ordinal) WHERE "status" = 1;

-- 取一个实体的全部属性（详情面板）
CREATE INDEX IF NOT EXISTS idx_knowledge_property_target
    ON knowledge_property (target_type, target_id);

-- 按属性反查挂哪些载体（「查所有 model=xxx 的 agent 节点」类结构化过滤入口）
CREATE INDEX IF NOT EXISTS idx_knowledge_property_term
    ON knowledge_property (term_key, target_type, value_text);

-- 数值型属性的范围查询入口（量化指标过滤；term_key 已隐含载体，不再前置 target_type）
CREATE INDEX IF NOT EXISTS idx_knowledge_property_num
    ON knowledge_property (term_key, value_num);
