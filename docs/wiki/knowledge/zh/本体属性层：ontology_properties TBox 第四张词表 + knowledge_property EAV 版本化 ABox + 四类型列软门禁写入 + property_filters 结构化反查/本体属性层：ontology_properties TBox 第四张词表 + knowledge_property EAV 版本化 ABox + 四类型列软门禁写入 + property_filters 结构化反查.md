---
kind: RAG 原子知识卡
name: 本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查

category: 知识图谱与记忆沉淀 / 本体治理
scope:
  - "common/src/ontology_property.rs"
  - "common/src/enums/ontology.rs"
  - "common/src/api/ontology.rs"
  - "common/src/api/neural_tools.rs"
  - "common/src/error/code.rs"
  - "migrations/20261011*.sql"
  - "src/handlers/hr/ontology/create_property.rs"
  - "src/handlers/hr/ontology/list_properties.rs"
  - "src/handlers/hr/ontology/update_property.rs"
  - "src/handlers/hr/ontology/retire_property.rs"
  - "src/handlers/hr/agent/mod.rs"
  - "src/handlers/hr/agent/save_long_term_memory.rs"
  - "src/handlers/hr/agent/update_memory.rs"
  - "src/handlers/hr/agent/search_memory.rs"
  - "src/service/dal/memory/properties.rs"
  - "src/service/dao/memory/*.rs"
  - "src/service/domain/hr/ontology.rs"
  - "src/models/ontology.rs"
  - "src/models/memory.rs"
  - "src/router.rs"
  - "src/pkg/tool_registry/builtin.rs"
  - "frontend/src/pages/hr/knowledge_graph.rs"
  - "frontend/src/pages/hr/ontology_panel.rs"
source_files:
  - migrations/20261011000001_create_ontology_properties.sql
  - common/src/ontology_property.rs
  - common/src/enums/ontology.rs
  - common/src/api/neural_tools.rs#L1-L120
  - src/handlers/hr/agent/mod.rs#L226-L420
  - src/service/dal/memory/properties.rs#L920-L1068
  - src/service/dao/memory/sqlite/properties.rs
  - src/service/domain/hr/ontology.rs#L65-L103
  - docs/design/ontology_knowledge_sedimentation_design.md
  - docs/wiki/zh/content/功能模块/知识图谱管理/本体属性管理.md
  - 【Level 3 兄弟卡】docs/wiki/knowledge/zh/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md
---

# 本体属性层：TBox 第四张词表 + EAV 版本化 ABox + 软门禁写入 + 结构化反查

## §1 整体方案

本体原三张词表（classes / relation_types / synonym_mappings）回答节点与边**「叫什么」**；属性层回答它们**「是什么值」**，由两张表构成严格 TBox/ABox 分层：

- **TBox（定义侧）`ontology_properties`**：本体第 4 张词表。每行一个属性定义——`term_key` 语义锚点（活跃行唯一索引，退役行保留可重建）、`data_type` 六型（`string | number | boolean | date | reference | enum`）、`value_range`（min/max/enum_values 值域）、`cardinality`（single | many）、`domain_classes` / `applies_to`（适用载体，JSON：node 的 node_type 集合 + `relation`）、`unit`、`required`、`status`。
- **ABox（值侧）`knowledge_property`**：通用 **EAV** 值表，节点与边共用。`target_type`（node | relation）+ `target_id`（**裸列无 FK**，图谱表无统一主键可用）定位载体；四类型列 `value_text / value_num / value_bool / value_ref` 按 data_type 落一列；`ordinal` 保序（single 恒 0）；`status` 1=生效 / 0=被取代（修正时旧行降级，历史保留）/ 2=已删除（显式软删除，可恢复），与知识边 `KnowledgeRelationStatus` 同口径，**软取代即版本化**——single 基数重写不删行，旧值置 0、新值置 1；部分唯一索引 `uq_knowledge_property_active` 只约束生效行（同 target+term+ordinal）。

**写入（随记忆工具寄生，无独立写端点）**：`save_long_term_memory` / `update_memory` 的 `properties[]` → handler 共享 `save_node_properties` → `MemoryDal::save_entity_properties` 五道闸门：门闩 0 未定义属性（软，按值自身类型兜底照写）→ 门闩 1 载体不允许 `allows_target`（软，记 issue 后照写）→ 门闩 2 类型不符 `TypeMismatch` + NaN/无穷防穿（**硬，跳过本条**）→ 门闩 3 值域越界（NumberOutOfRange / NotInEnumValue，软，照写）→ 门闩 4 基数超限 `CardinalityExceeded`（**硬，跳过本条**；single 强制 ordinal=0，新值取代旧值不算冲突）。纯判定全部在 `common::ontology_property`（无 IO）：`PropertyWriteIssue` 六类，仅 TypeMismatch/CardinalityExceeded 两类 `rejects_write()` 为真——硬拦即**跳过该条**（continue，同批其他条目照常），软门禁**逐字段**进 `PropertySaveReport.issues` 但该值照常写入。属性是独立一步：节点/边已建成功时，属性问题永不连累主实体；响应双版本 `property_issues: [String]`（V1 并存）与 `property_issues_v2: [{code,message,field}]`。

**读取**：`search_memory` 结果经 `attach_properties` 批量回显（仅 status=1）；新增 `property_filters[]`（`{term_key, op: Eq|Gt|Gte|Lt|Le, value}`，AND 语义）作为**独立召回通道**——先 `query_properties_by_value` 解析出命中 node id 集合，再与 query 语义召回取**交集**（不是在语义 top_k 上二次过滤）；比较算子（非 Eq）要求 number 值；与 `seed_node_ids` 互斥（种子恒返回，属性过滤无法生效）。

**定义 CRUD 与授权分层**：4 个管理端 handler（create/list/update/retire_property），`list_ontology_properties` 工具标签 = `neural` + `ontology_management` + `ontology`（Agent 读词表要能唤醒），写侧三个仅 `ontology_management`；TOOL_REGISTRY 三条护栏测试锁死边界（13 个消费侧工具：读 8 + 写 5）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|--------------|
| [migrations/20261011000001_create_ontology_properties.sql](migrations/20261011000001_create_ontology_properties.sql) | 两表 DDL | ontology_properties（term_key 活跃唯一索引、domain_classes/applies_to JSON、六型 CHECK、status）；knowledge_property（target_type+target_id 裸列、四值列、ordinal、status 1/0/2、uq_knowledge_property_active 部分唯一索引、target/term/num 三索引） |
| [common/src/ontology_property.rs](common/src/ontology_property.rs) | 跨 crate 纯函数层（无 IO） | PropertyValue 六型枚举（tagged 序列化）；PropertyColumns（一行落哪列）；ValueRange{min,max,enum_values} + parse_value_range（**主键 enum_values，兼容旧键 enum**）；validate_property_value（类型硬校验 + NaN 防穿）；allows_append（基数）/allows_target（载体）；PropertyEntry；PropertyWriteIssue 六类 + rejects_write（仅 TypeMismatch/CardinalityExceeded）；PropertySaveReport{accepted,issues}；PropertyFilter + PropertyFilterOp（Eq/Gt/Gte/Lt/Lte） |
| [common/src/enums/ontology.rs](common/src/enums/ontology.rs) | 三枚枚举 | PropertyDataType（六型）/ PropertyCardinality（single/many）/ PropertyTargetType（node/relation）；均 lowercase serde + parse_or_default 兜底 |
| [common/src/api/ontology.rs](common/src/api/ontology.rs) | 定义侧 DTO | OntologyPropertyItem + List/Create/Update/Retire 请求 |
| [common/src/api/neural_tools.rs](common/src/api/neural_tools.rs) | 值侧工具 DTO | KnowledgePropertyParam（写入 tagged value）/ KnowledgePropertyItem（回显）/ PropertyFilterParam / PropertyIssueEntry{code,message,field} / RelationWriteResult（建边逐项回执，详见建边卡）；两 Response 挂 relations_result、property_issues_v2（skip 序列化默认空） |
| [common/src/error/code.rs#L132-L186](common/src/error/code.rs#L132-L186) | 错误码词汇表 | 属性域 6 码（property_undefined / property_target_not_allowed / property_type_mismatch / property_cardinality_exceeded / property_number_out_of_range / property_not_in_enum_value） |
| [src/handlers/hr/agent/mod.rs#L226-L420](src/handlers/hr/agent/mod.rs#L226-L420) | 两写入口共享实现 | save_node_properties(L226) / property_issue_strings(L248 V1) / property_issue_entries(L259 V2) / is_placeholder_node_id(L294 self/kn_self/kn_new_self) / is_valid_node_id(L303 kn_ 前缀) / write_relations_with_receipt(L323) |
| [src/service/dao/memory/sqlite/properties.rs](src/service/dao/memory/sqlite/properties.rs) | 属性写编排 | save_entity_properties：门闩 0~4、批内占位 batch_taken 防同批重复、batch_upsert_properties 整批事务、single 重写触发 supersede |
| [src/service/dao/memory/sqlite/properties.rs](src/service/dao/memory/sqlite/properties.rs) | 值表 DAO | upsert_property(L1729) / batch_upsert_properties(L1742) / list_active_properties(L1761) / list_active_properties_batch(L1785) / supersede_active_property(L252) / query_properties_by_value(L1862，property_filters 召回) |
| [src/handlers/hr/agent/search_memory.rs](src/handlers/hr/agent/search_memory.rs) | 读侧 | property_filters 与 seed_node_ids 互斥校验（L109）；filters 独立召回通道；attach_properties 批量回显 status=1（L261） |
| [src/service/domain/hr/ontology.rs#L65-L103](src/service/domain/hr/ontology.rs#L65-L103) | 定义侧 Domain 严格校验 | validate_property_definition：data_type/cardinality 非法、applies_to 拼错、value_range 形态不符 → 400；trait create_property(L330)/get/list/update/retire(L389) |
| src/handlers/hr/ontology/{create,list,update,retire}_property.rs | 定义侧 4 handler | 管理端 CRUD；路由 src/router.rs#L786-L800；tag 分层：list = neural+ontology_management+ontology，写侧仅 ontology_management |
| [src/models/ontology.rs#L215](src/models/ontology.rs#L215) / [src/models/memory.rs#L374](src/models/memory.rs#L374) | PO | OntologyPropertyPo（TBox）/ KnowledgePropertyPo（ABox EAV）；PO 不出 Domain |
| [src/pkg/tool_registry/builtin.rs](src/pkg/tool_registry/builtin.rs) | 标签护栏测试 | ontology_consumer_tools_carry_ontology_tag / ontology_consumer_tools_are_neural_reachable / ontology_read_tools_are_neural_reachable |
| [frontend/src/pages/hr/knowledge_graph.rs](frontend/src/pages/hr/knowledge_graph.rs) / [frontend/src/pages/hr/ontology_panel.rs](frontend/src/pages/hr/ontology_panel.rs) | 前端 | 节点详情属性区块（property_display_name / format_property_value 六型格式化 + 定义缓存）；本体面板属性 Tab + 定义增删改 Modal |
| 【Wiki 长文】本体属性管理.md | 系统化上下文（10 章） | [docs/wiki/zh/content/功能模块/知识图谱管理/本体属性管理.md](docs/wiki/zh/content/功能模块/知识图谱管理/本体属性管理.md) |
| 【① Design】ontology_knowledge_sedimentation_design.md | TBox/ABox 分层决策 | [docs/design/ontology_knowledge_sedimentation_design.md](docs/design/ontology_knowledge_sedimentation_design.md) |

## §3 架构约定

**【Level 3 对称声明】本卡与兄弟卡「本体词表漂移治理与 Prompt 注入」同域双视角、永久互链**：兄弟卡管三张旧词表（类/关系/同义）的登记、Prompt 注入与漂移治理（**叫什么**）；本卡管第 4 张词表（属性定义）与 EAV 值表的写入/回显/反查（**是什么值**）。两张表族共享「全局无 org_id + term_key 语义锚点 + seed 仅补缺 + common 纯函数」四项本体基座约定，任何一侧改基座约定都必须回另一侧卡片同步。

1. **作用域红线**：两表与三张旧词表一致**不带 org_id**（单部署单库即边界）；属性只挂知识图谱节点/边（target_type node|relation），task/project 仅以节点 node_type 身份出现在 applies_to——**不扩到 agent/task/project 实体表，target_id 不加 FK**。
2. **软门禁 vs 硬拦二分**：只有「类型不符（含 NaN）」与「基数超限」两类硬拦（不落库）；未定义/载体不符/值域越界只记 issue 不拦——节点必须先能沉淀，质量问题结构化回执给 Agent 自纠。
3. **纯判定在 common**：类型/值域/基数/载体全部 common::ontology_property 纯函数，handler/DAL/domain 三处共用；禁止在 handler 私写校验。
4. **版本化=软取代**：single 重写不 DELETE，旧行 status=2 新行 status=1；读路径永远只认 status=1（回显与反查皆然）。
5. **property_filters 是独立召回通道**：先解析命中 node id，再与语义召回交集；禁止在语义 top_k 结果上内存过滤（会被 top_k 截断带偏）；非 Eq 比较算子只接受 number。
6. **属性寄生在记忆写入口**：不发独立工具端点；属性步骤异常不影响节点/边创建结果，问题走回执而非整请求失败。
7. **授权读宽写窄**：list_ontology_properties 对 neural 常驻可读（词表注入/写前引导需要）；定义写侧仅 ontology_management，三条护栏测试防提权。
8. **值域 JSON 双键兼容**：枚举值读取主键 `enum_values`，旧键 `enum` 仅作 fallback；写入只产出 enum_values。

## §4 约束清单

1. ✅ data_type 六型封闭集合，DB CHECK + Rust 枚举 + serde lowercase 三处同口径；非法值 parse_or_default，禁止第七型
2. ✅ 一行值只落四值列中与 data_type 对应的一列；布尔/数值不得塞进 value_text 绕过校验
3. ✅ validate_property_value 必须拦 NaN（serde 可传 NaN，`n == n` 判定）
4. ✅ rejects_write() 为真的只有 TypeMismatch / CardinalityExceeded；新增 issue 类型默认软门禁，除非显式加入 rejects_write
5. ✅ PropertyIssueEntry 必须带 code（机器可读错误码）+ message + field（term_key）；V1 字符串字段 property_issues 保留并存，禁止提前删
6. ✅ 单值重写必须走 supersede（旧行 status=0、新行 status=1），禁止物理删除或同 active 双行（靠 uq_knowledge_property_active 兜底）；status=2 仅用于显式软删除
7. ✅ target_id 裸列：禁止给 knowledge_property 加指向多实体表的 FK；端点评级由写入口按 kn_ 前缀 + Active 批量预检负责
8. ✅ property_filters 与 seed_node_ids 同现 → invalid_request（L109 互斥）；Gt/Gte/Lt/Le 右侧必须是 number
9. ✅ attach_properties 只回显 status=1；批量 IN 查询，禁止逐节点 N+1
10. ✅ 属性表不带 org_id、不扩 target_type 到 node/relation 之外；新载体需求先改本卡再写码
11. ✅ 定义写侧 400 场景由 domain validate_property_definition 统一判定（非法 data_type/cardinality、applies_to 拼写、value_range 形态），handler 不私判
12. ✅ list 工具三标签（neural+ontology_management+ontology），写侧三工具只挂 ontology_management；改标签必须同步三条护栏测试
13. ✅ 预置属性 4 条（confidence number 0~1 node+relation / effective_from date / source string many / status enum 五值适 task/project 节点）随 seed 仅补缺注入；新增预置定义走 seed 快照不走启动注入
14. ✅ 三基础员工的本体能力通过 installed_tags/skill_packs（含 ontology）+ 神经技能 TEMPLATE_ONTOLOGY_COGNITION 装配；PromptBuilder 技能区块「装了就进」，不在 builder 侧二次 tag 过滤
15. ✅ seed_node_ids 召回的节点恒返回（中心节点不丢）——这也是属性过滤不得与种子展开共用的根因

## §5 历史演进

| 日期 | commit | 事件 | 变更内容 |
|------|--------|------|---------|
| 2026-10-11 | 4b4e9e7a（2f42b407→a4525d6c 区间） | 本体属性层首版 + 预置体系重构 | 两表迁移；common 纯函数层；记忆两写入口寄生 properties[] + 五闸门；search_memory 回显与 property_filters；定义 CRUD 4 handler + 三护栏测试；seed 4 预置属性 + TEMPLATE_ONTOLOGY_COGNITION + 第三基础员工知识图谱专家；前端节点详情属性区块 |
| 2026-10-12 | 87c35228 / 5a7cf19e | 写回执 V2 + 读侧修复 | property_issues_v2 结构化（code/message/field）；relations_result 逐项 keyed 回执（建边域，见联想建边卡）；value_range 读侧 enum_values 主键修复（旧键 enum fallback）；属性回读补 status=1 过滤 |
