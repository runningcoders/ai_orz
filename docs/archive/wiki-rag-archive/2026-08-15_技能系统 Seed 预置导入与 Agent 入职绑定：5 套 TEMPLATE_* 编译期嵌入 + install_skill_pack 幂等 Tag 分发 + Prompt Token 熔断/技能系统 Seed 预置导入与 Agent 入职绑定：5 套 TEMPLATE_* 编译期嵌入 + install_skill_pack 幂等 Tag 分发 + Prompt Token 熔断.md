> 📦 归档标记（2026-08-15）：被 [Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断](docs/wiki/knowledge/zh/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: RAG 原子知识卡
name: 技能系统 Seed 预置导入 + Agent 入职技能绑定：5 套 TEMPLATE 技能包 + SkillPo Vectorizable + install_skill_pack 幂等 Tag 分发
category: 业务模块 / 技能 HR
scope:
  - "src/models/skill.rs"
  - "src/service/domain/system/seed/**"
  - "src/service/domain/hr/skill.rs"
  - "src/service/dal/skill*.rs"
  - "src/handlers/hr/agent/install_skill_pack.rs"
  - "src/handlers/hr/skill/*.rs"
source_files:
  - src/models/skill.rs#L9-L49 (SkillPo 字段：id/name/description/tags JSON/category/parent_skill_id/author_type/SkillStatus/content_path；parse_tags + to_prompt_summary 双方法)
  - src/models/skill.rs#L88-L92 (SkillPo::to_prompt_summary()：唤醒注入 Prompt 时使用的精简格式 `- 技能名：描述`；不含 skill.md 正文防 Token 膨胀)
  - src/service/domain/system/seed/mod.rs#L1-L16 (Seed 子模块定位：纯工具箱不持 DAL；pub mod default/defs/diff/embedded/store；默认 5 套技能包通过 include_str! 编译期嵌入)
  - src/service/domain/system/seed/default.rs#L1-L9 (embedded_default_snapshot()：include_str!("default.json") 编译期反序列化 SeedSnapshot；解析失败编译报错，杜绝运行时缺文件)
  - src/service/domain/system/seed/skills/TEMPLATE_TOOL_MANAGEMENT/skill.md (技能包 1/5：工具管理模板——工具绑定、工具包安装、工具调用出错时的处理策略)
  - src/service/domain/system/seed/skills/TEMPLATE_MEMORY_COGNITION/skill.md (技能包 2/5：记忆认知模板——save_short_term/save_long_term/settle 工具调用时机与 Prompt 模板)
  - src/service/domain/system/seed/skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md (技能包 3/5：项目管理模板——task 创建、依赖图设计、进度汇报格式；配 PMO Agent)
  - src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md (技能包 4/5：沟通协作模板——用户消息回复格式、Agent 间协作 A2A 协议、SSE 流式输出)
  - src/service/domain/system/seed/skills/TEMPLATE_SKILL_MANAGEMENT/skill.md (技能包 5/5：技能自管理模板——Agent 发现新经验时沉淀新技能的步骤规范)
  - src/service/domain/hr/skill.rs#L12-L60 (SkillManage 子 trait：create_skill 文件元数据 + 写 skill.md 文件两步；update_skill 先路径安全校验 → 元数据 → 写入/删除文件 → 导入文件的顺序五步，严格防失败时产生脏数据)
  - src/handlers/hr/agent/install_skill_pack.rs#L1-L33 (install_skill_pack Handler + register_handler_tool 神经工具暴露：按 tag 查询所有 Published 技能 → 拷贝 Draft 副本到 Agent；idempotent：tag 已存在返回 installed_count=0)
  - src/service/dal/skill/mod.rs (SkillDal trait：query 技能 + search 技能 + 按 tag 批量查；按 id 查内容；写文件走 content_path 相对路径防越权)
  - docs/design/skill_design.md（§数据库设计 skills 表；§文件存储相对路径管理；§三种状态：待沉淀/可用/过期 + 软删除）
  - docs/design/skill_system_enhancement_design.md（§唤醒注入机制：启动时间按 installed_skill_packs 拉技能的 to_prompt_summary 注入 Agent 角色 Prompt）
  - docs/plan/预置基础技能导入重构.md（落地：5 套 TEMPLATE_* 目录结构 + default.json 快照 + 编译期嵌入方式）
  - docs/wiki/zh/content/功能模块/技能系统.md（技能系统总览：沉淀流程 → 标签分类 → 状态流转 → 安装到 Agent 的四步用户故事）
  - docs/wiki/zh/content/功能模块/AI Agent 管理/技能包管理.md（技能包管理面板：Tag 筛选 + 批量安装 + 安装历史记录列表）
  - docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/技能与工具绑定.md（入职流程：创建 Agent → 安装默认技能包 → 配置身份凭证 → 绑定工具包 → 上线）
  - docs/wiki/zh/content/数据模型/Agent 和技能模型/Agent 和技能模型.md（SkillPo 字段表 + Vectorizable skill 向量化标签搜索实现）
  - docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/技能管理系统.md（前端技能编辑器：左侧文件树 + 中间 Markdown 编辑 + 右侧 Prompt 预览）
  - docs/wiki/zh/content/功能模块/用户与组织管理/系统初始化.md（init_base_data 内调用 seed::store::import_default_skills_if_empty()——系统首次启动自动导入 5 套默认技能包为 Published 状态）
  - 【平行卡 1】docs/wiki/knowledge/zh/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md（技能安装事件：InstallSkillPackEvent → 打 Task 维度统计）
  - 【平行卡 2】docs/wiki/knowledge/zh/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md（SkillPo Vectorizable 实现：vectorize_text()=name + description + tags.join，用于技能语义搜索）
---

## §1 概述

**本卡角色**：技能系统种子预置导入 + Agent 入职技能绑定的业务知识卡。覆盖 `domain/system/seed` 子模块的 5 套 `TEMPLATE_*` 技能包（TOOL_MANAGEMENT / MEMORY_COGNITION / PROJECT_MANAGEMENT / COMMUNICATION / SKILL_MANAGEMENT）编译期嵌入导入机制、`SkillPo` 双方法（`parse_tags` 结构化标签 + `to_prompt_summary` 精简 Prompt 注入）、`install_skill_pack` 神经工具的按 tag 幂等分发逻辑，以及 HR Domain 的 `SkillManage::update_skill` 五步文件安全校验顺序。**定位：新增预置技能包、排查技能安装失败、Agent 唤醒后技能没注入 Prompt 的场景时读。**

- **5 套预置技能包编译期嵌入**：不是运行时读文件系统（避免 Docker 打包漏拷贝技能目录），而是 `default.rs` 里 `const DEFAULT_JSON: &str = include_str!("default.json")` + `embedded_default_snapshot()` 在编译期就把技能元数据 + skill.md 内容嵌进二进制。`seed::store::import_default_skills_if_empty()` 在 `init_base_data` 内幂等调用——先查 `skills` 表 count=0 才插入。`default.json` 格式错误编译报错，不会留到运行时 panic。
- **技能安装「Published → Draft 副本 + Tag 映射」两步**：`install_skill_pack(ctx, agent_id, tag="memory")` 内部：① `SkillDal.query(SkillQuery { tags_contains: ["memory"], status: Some(Published) })` 拉所有带该 tag 的全局技能；② 对每条 Published Skill → 复制一份「草稿副本」到 Agent 私有技能目录（content_path 前缀变 agents/{agent_id}/skills/{id}），status 改 Draft；③ 把 tag 写进 Agent `runtime_config.installed_skill_packs` 数组；④ 幂等校验：tag 已在数组里直接 return installed_count=0，绝不重复复制。Agent 唤醒注入时用 installed_skill_packs 拉自己的 Draft 版本而不是全局 Published（Published 是模板，Agent 自己那份可以个性化修改）。
- **SkillPo Vectorizable + to_prompt_summary 双 Token 控制**：唤醒 Prompt 注入只放 `to_prompt_summary()` = `- 技能名：一句话描述`（单条 ≈ 20-50 Token），不放 skill.md 正文（动辄上千 Token 爆上下文）。`vectorize_text()` 向量索引放完整 name+description+tags.join(" ")，保证技能语义搜索能搜到——Prompt 精简与搜索完整两者解耦，不同场景用不同字段。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| models/skill.rs SkillPo | 持久化对象 + 双方法 | 12 字段：id/name/description/tags JSON/category/parent_skill_id/author_type(SkillAuthorType)/status(SkillStatus)/content_path；parse_tags() JSON→Vec<String>；to_prompt_summary() 精简格式；get_tags() 别名 | `:L9-L100` |
| system/seed/mod.rs | Seed 工具箱入口 | 纯工具箱 5 子模块：default(内置快照) + defs(SeedSnapshot/SkillSeed 结构定义) + diff(diff 算法 UI 展示) + embedded(另一份嵌入?) + store(DB 写)；不持 DAL 引用 | `:L1-L16` |
| system/seed/default.rs | 编译期嵌入 | include_str!("default.json") 反序列化为 SeedSnapshot；解析失败编译期 panic（保证 default.json 格式永远正确） | `:L1-L9` |
| seed/skills/TEMPLATE_* 5 份 | 5 套模板 | TOOL_MANAGEMENT（工具绑定）/ MEMORY_COGNITION（记忆写入沉淀）/ PROJECT_MANAGEMENT（项目推进）/ COMMUNICATION（用户 Agent 沟通）/ SKILL_MANAGEMENT（技能自沉淀）；每份 skill.md 是 Markdown 正文 | 见各子目录 |
| domain/hr/skill.rs | SkillManage impl | create_skill = DAL.create PO → 逐个文件写入；update_skill 五步顺序：①文件目标路径安全校验 → ②元数据 → ③写文件 → ④删文件(TODO) → ⑤导入外部文件，防失败脏数据 | `:L12-L60` |
| handler/install_skill_pack.rs | 入职 Handler+工具 | register_handler_tool 神经工具暴露给 Agent 自己安装；tag 参数查询 Published 技能 → Draft 拷贝；幂等返回 installed_count（0=已装过，N=本次新装） | `:L1-L33` |
| dal/skill/mod.rs | 技能业务 DAL | query：tags_contains/status/category 过滤；search：FTS5 关键词 + 向量语义混合；write_file：相对 content_path 写绝对文件路径前防越权（.. 穿透） | 见 DAL trait |

**章节来源**
- [models/skill.rs:L9-L100](src/models/skill.rs#L9-L100)
- [domain/hr/skill.rs:L12-L60](src/service/domain/hr/skill.rs#L12-L60)
- [system/seed/default.rs:L1-L9](src/service/domain/system/seed/default.rs#L1-L9)
- [handler/install_skill_pack.rs:L1-L33](src/handlers/hr/agent/install_skill_pack.rs#L1-L33)

---

## §3 架构约定与扩展模式

### 3.1 新增预置技能包（6 步不走岔）

1. **新建技能包目录**：`domain/system/seed/skills/TEMPLATE_YOUR_SKILL/`，必须大写前缀 `TEMPLATE_` 命名
2. **写 skill.md 正文**：Markdown 格式，一级标题 = 技能显示名称；结构遵循「何时用 / Prompt 模板 / 使用示例 / 注意事项」四节
3. **补充元数据到 default.json**：在 `seed/default.json` 的 skills 数组追加对象 `{ "id": "...", "name": "...", "tags": ["your_tag"], "category": "...", "content_path": "skills/TEMPLATE_YOUR_SKILL", "skill_md_file": "skill.md" }`
4. **运行 `cargo build`**（触发 include_str! 编译期嵌入）—— default.json 格式错误在这里直接报，不必跑集成测试
5. **加集成测试断言**：`tests/integration/seed_skills_import_test.rs` 新增断言 `assert!(skill_ids.contains("TEMPLATE_YOUR_SKILL"))` 验证导入后存在
6. **更新 AGENTS.md / Wiki**：技能标签加到用户入职可选择的标签清单文档，避免「装了但没人知道」

### 3.2 Agent 唤醒技能 Prompt 注入链路

- 位置：`domain/runtime/awakening.rs::build_wake_prompt()` 内 → 读取 Agent.runtime_config.installed_skill_packs → 按 tag 批量查出 Agent 自己所有 Draft 技能 → to_prompt_summary → 拼接成 `## 你已安装的技能\n- xxx\n- yyy` 一段 → 注入 System Prompt 末尾
- **Token 上限熔断**：如果所有 installed skill summaries > 800 Token → 截断前 N 条（按 tags 匹配当前任务相关度排序优先保留），并在 Prompt 末尾加「`(共 M 条技能，前 N 条展示，其余省略)`」防止超出 4k/8k 上下文窗。熔断阈值 800 在 `awakening.rs` 顶部 `const MAX_SKILL_SUMMARY_TOKENS: usize = 800;`

### 3.3 技能向 Vectorizable 约定

- `vectorize_text()` = `format!("{}\n{}\n{}", name, description, tags.join(" "))`
- `vector_collection()` = `"skills"` 固定
- 技能搜索：前端 `/api/v1/skills/search?q=项目管理` → DAL `search(q)` 走三位一体（FTS5 name+description match + 向量语义 match + 合并排序）。注意 FTS5 只搜 Published Draft 不搜（Agent 私有技能不进全局 FTS 索引），避免跨用户数据泄露。

---

## §4 硬约束与回归红线

1. **import_default_skills_if_empty 必须在 init_base_data 内调（铁律 AGENTS.md §4.10）**：consumer::init、handler 里、外部 migration 脚本一律不能插默认技能。测试环境 `init_full_test_env` 走同样顺序，否则 5 套模板技能丢失，「默认 Agent 入职后没技能」的 bug 必现。
2. **写文件绝对要过 content_path + 防路径穿越**：`SkillDal::write_file(po, filename, content)` 先 `let target = base_data_path().join(&po.content_path).join(filename)` → `target.canonicalize()?` → 检查 `target.starts_with(base_data_path())` 为 true → false 则 ErrorCode::PathTraversal，绝不允许 `../` 穿到 base_data_path 外部（如 /etc/passwd）。
3. **install_skill_pack 的 Draft 副本 id 必须重生成**：禁止「把 Published skill.id 直接用」，否则两个不同 Agent 安装同一份 Published skill → id 冲突 DB UNIQUE 错误。正确做法：`format!("{}-{}", published_skill.id, agent_id_short_hash)` 生成新 Draft 副本 id。
4. **技能安装幂等性单元测试覆盖**：`test_install_skill_pack_idempotent`：①第一次安装 → assert installed_count=1；②同 tag 再装一次 → assert installed_count=0；③查 DB 里 skills 表按 category=Draft + agent_id 所属 = 条数=1，绝不重复。
5. **to_prompt_summary 禁止超过 200 字符/条**：单个技能 summary 如果 >200 字符 → 直接 clippy/测试告警；Prompt 精简是这条方法存在的唯一意义，长了就把详细内容塞 skill.md 正文，唤醒时不放。
6. **TEMPLATE_ 前缀 5 套默认技能绝不允许手工删除**：即使业务里觉得某套模板过时，也必须用 SkillStatus=Expired（过期态）软标记，不允许 DELETE。原因：历史 Agent 安装的 Draft 副本仍然引用 parent_skill_id = TEMPLATE_xxx.id，DELETE 会触发 FK 约束或 parent 引用悬空。
