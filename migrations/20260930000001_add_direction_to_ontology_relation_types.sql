-- 三期：方向性边承载——ontology_relation_types 增 direction/multiplicity 两列
-- 依据：AMan 拍板 a′（三点拍板 01a0f0b3 + 四点拍板 01a0f049），
-- direction 二值（directed/undirected），multiplicity 占位（消费 backlog）
ALTER TABLE ontology_relation_types
    ADD COLUMN direction TEXT NOT NULL DEFAULT 'undirected'
    CHECK(direction IN ('directed', 'undirected'));

ALTER TABLE ontology_relation_types
    ADD COLUMN multiplicity TEXT;

-- 回填：10 有逆词（inverse_key 非空）→ directed
UPDATE ontology_relation_types
    SET direction = 'directed'
    WHERE inverse_key IS NOT NULL AND inverse_key != '';

-- 回填：8 无逆词有向 → directed（显式点名，自文档化+幂等）
UPDATE ontology_relation_types
    SET direction = 'directed'
    WHERE term_key IN ('prerequisite', 'followup', 'instance_of', 'category_of',
                       'attribute_of', 'value_of', 'produces', 'binds');

-- 回填：3 无向词 → undirected（显式，防默认值漂移，幂等）
UPDATE ontology_relation_types
    SET direction = 'undirected'
    WHERE term_key IN ('related', 'similar', 'opposite');
