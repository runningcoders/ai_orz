---
kind: rag_card
name: Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt
  Token 熔断
category: 基础设施
scope:
- src/service/domain/hr/skill.rs
- src/service/domain/hr/agent.rs
- src/service/domain/system/seed/**
- src/service/dal/skill*.rs
- src/handlers/hr/agent/install_skill_pack.rs
- src/handlers/hr/skill/*.rs
- src/models/skill.rs
- common/src/enums/tool_tag.rs
- common/src/api/hr.rs
source_files:
- src/models/skill.rs#L9-L100
- src/models/skill.rs#L88-L92
- src/service/domain/system/seed/mod.rs#L1-L16
- src/service/domain/system/seed/default.rs#L1-L9
- src/service/domain/hr/skill.rs#L16-L190
- src/service/domain/hr/skill.rs#L12-L60
- src/service/domain/hr/agent.rs#L419-L520
- src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md
- src/service/domain/system/seed/skills/TEMPLATE_MEMORY_COGNITION/skill.md
- src/service/domain/system/seed/skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md
- src/service/domain/system/seed/skills/TEMPLATE_SKILL_MANAGEMENT/skill.md
- src/service/domain/system/seed/skills/TEMPLATE_TOOL_MANAGEMENT/skill.md
- src/models/skill.rs#L22-L160
- src/handlers/hr/agent/install_skill_pack.rs#L1-L33
- src/handlers/hr/skill/install_skill_pack.rs#L19-L85
- src/service/dal/skill/mod.rs
- docs/archive/design-archive/skill_design.md
- docs/archive/design-archive/skill_system_enhancement_design.md
- docs/archive/plan-archive/Agent管理集成测试.md
- docs/archive/plan-archive/预置基础技能导入重构.md
- docs/wiki/zh/content/功能模块/技能系统.md
- docs/wiki/zh/content/功能模块/AI Agent 管理/技能包管理.md
- docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/技能与工具绑定.md
- docs/wiki/zh/content/数据模型/Agent 和技能模型/Agent 和技能模型.md
- docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/技能管理系统.md
- docs/wiki/zh/content/功能模块/用户与组织管理/系统初始化.md
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/HR 领域编排.md

---

# §1 概述（一句话定位 + 解决什么问题）

**定位**：技能系统四层增强——① 5 套 TEMPLATE 预置技能包（Communication/MemoryCognition/ProjectManagement/SkillManagement/ToolManagement，每个 skill.md 结构化 6 字段 + `include_str!` 嵌入式注入 HRDomain init）；② `install_skill_pack` 幂等 Tag 批量分发（按 SkillTag 标签分组已发布技能 → 批量 find_by_tag → 为 Agent 逐个 create_agent_skill_private → 重名跳过 warn）；③ Agent 入职流程绑定（onboard_agent 调 install_default_skill_packs：默认 5 套全装，安装失败不阻断入职只打 warn + 记录缺失清单）；④ Prompt Token 熔断与分层注入（Core Role + System Capabilities + Skills Prompt + Current Task 四层，每层有独立 Token 预算上限，超限自动从 Current Task 开始反向裁剪）。

**解决三类存量缺口**（对应 Design §1.1）：
1. **缺批量安装 + 标签分发能力**：只能逐个技能绑定 Agent，10 个技能要 10 次 API；没技能包/tag 维度管理
2. **冷启动无默认技能**：新 Agent 创建后技能面板空空如也，用户手工逐个点绑定体验极差；缺少默认 5 套「行业通用技能集」
3. **Prompt 过长导致 LLM 拒绝服务**：N 多技能 Prompt 注入无上限 → Token 爆表 429 或 LLM 忽略系统提示；缺少分层预算 + 熔断裁剪机制

---

# §2 关键文件与核心锚点速查表

| 文件锚点（点击跳转） | 角色 | 核心契约 / 红线 |
|---------------------|------|-----------------|
| [HRDomain skill 操作（update/install_to_agent）](src/service/domain/hr/skill.rs#L16-L190) | 技能包 CRUD 领域契约 | update_skill（含 tag 同步 + Prompt 重计算）；install_to_agent（幂等：同 agent_id+name 已存在 skip；is_system 权限校验）；find_by_tag 批量查询 |
| [AgentDomain install_skill_pack 主入口](src/service/domain/hr/agent.rs#L419-L520) | 技能包批量安装实现 | ① SkillDomain.find_by_tag(tag) 查出已发布技能 → ② for_each create_agent_skill_private（拷贝 payload + 版本）→ ③ 重名/已绑定 skip warn → ④ 返回 {installed, skipped, total} 统计；事务保证部分成功不回滚 |
| [5 套 TEMPLATE 预置 skill.md 文件](src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md) | 技能模板嵌入式注入 | skill.md 6 字段结构（name / description / tags[] / prompt_template / system_constraints / usage_scenarios）；include_str! 编译期嵌入；HRDomain init_base_data 幂等插入 |
| [SkillPo + Prompt 摘要 to_prompt_summary](src/models/skill.rs#L22-L160) | 技能实体 SSOT + Prompt 注入 | SkillPo 字段（skill_id/agent_id/name/prompt_template/prompt_summary/version/tags/is_published/is_system）；to_prompt_summary 生成 Prompt 注入格式（"### 技能 {name}\n{prompt_template}..."） |
| [Handler install_skill_pack HTTP 接口](src/handlers/hr/skill/install_skill_pack.rs#L19-L85) | 前端技能包管理入口 | InstallSkillPackRequest{agent_id, tag, skill_ids(Option)}；skill_ids 缺省走 tag 全量；返回统计清单 installed/skipped/total |
| [技能系统增强 Design 四层架构](docs/archive/design-archive/skill_system_enhancement_design.md) | 为什么 / 决策 6 条 | §决策 2：按 tag 安装 vs 按 id 列表安装；§决策 4：入职默认绑定哪些包；§决策 5：Prompt 分层预算分配 |
| [Agent集成测试 Plan 落地快照](docs/archive/plan-archive/Agent管理集成测试.md) | 怎么做 + 结果 | §技能安装幂等 §入职绑定失败降级 §Prompt Token 预算断言 |
| [技能系统 Wiki 长文](docs/wiki/zh/content/功能模块/技能系统.md) | 人类百科 | §5 技能包管理 §8 故障排查（安装失败 / 重名 / Token 超限裁剪日志定位） |

---

# §3 架构约定与数据流（业务语义层面，不贴实现代码）

**入职绑定技能包端到端流程**：
```
新 Agent 创建 → onboard_agent 生命周期
  → Step 1：创建 Agent Po + 角色 + 默认 ModelProvider
  → Step 2：install_default_skill_packs(ctx, agent_id)
       顺序安装 5 套：
         ① COMMUNICATION      (tag = "Communication")
         ② MEMORY_COGNITION   (tag = "MemoryCognition")
         ③ PROJECT_MANAGEMENT (tag = "ProjectManagement")
         ④ SKILL_MANAGEMENT   (tag = "SkillManagement")
         ⑤ TOOL_MANAGEMENT    (tag = "ToolManagement")
       每套 install_skill_pack：
         → SkillDomain.find_by_tag(tag, is_published=true)
         → 技能为空？log_warn 记入 skipped，不中断
         → for 每个 skill：
              create_agent_skill_private(agent_id, skill)
                → 同 agent_id + name 已存在？→ skip
                → 否则 INSERT（拷贝 prompt_template/prompt_summary/version/tags）
       某套安装失败？catch_unwind → log_error → 继续下一套
       最终返回 installed_count + skipped_skills[] 清单
  → Step 3：返回入职完成，技能已绑定

Prompt Token 熔断分层架构（唤醒时组装）：
  ┌──────────────────────────────────────────────────────────┐
  │ Layer 1：Core Role（固定不变，永不裁剪）                  │ 预算上限：≈2K Token
  │ Agent 角色设定、身份、工作模式、对外沟通风格              │
  ├──────────────────────────────────────────────────────────┤
  │ Layer 2：System Capabilities（固定不变，永不裁剪）        │ 预算上限：≈3K Token
  │ 可用工具清单（只列名称 + 一描，不含 Prompt）、            │
  │ 记忆能力说明、策略引擎约束                                │
  ├──────────────────────────────────────────────────────────┤
  │ Layer 3：Skills Prompt（熔断层 1，超限先按重要度裁剪）     │ 预算上限：≈4K Token
  │ 按优先级从高到低注入 to_prompt_summary（SKILL_PRIORITY 表）│
  │ 超限按序跳过最后几个技能 + log_warn 记录                  │
  │ SOP：skill_mgmt(必装，管理其他技能)                       │
  │      → tool_mgmt(必装，知道自己会什么工具)                 │
  │      → memory(必装，基础记忆沉淀)                         │
  │      → comm(必装，基础沟通)                               │
  │      → project(可选，若项目相关场景才启用)                 │
  ├──────────────────────────────────────────────────────────┤
  │ Layer 4：Current Task / Context（熔断层 2，超限最先裁剪） │ 预算上限：≈8K Token
  │ WorkingMemory、当前对话历史、ShortTerm 摘要、当前意图分析 │
  │ 超限时裁剪 30 轮历史 → 20 → 10 → 5 逐步降级              │
  └──────────────────────────────────────────────────────────┘
          四层总和上限 ≈ 17K Token（默认 128K 上下文绰绰有余）
          任何单层超出预算 → 自动裁剪下一层 + 打熔断日志
```

**技能包幂等核心规则**（3 条行为红线）：
1. **按 tag 安装不等于全量覆盖**：已绑定的同名 skill 不重复创建（仅跳过不报错）；新 skill 增量追加；已绑定的 skill 不会因按 tag 重装而被「解绑」
2. **安装部分失败不回滚**：10 个技能装到第 6 个报错，前 5 个保留，6-10 记到 skipped[]，用户可在技能面板手动重试
3. **技能与工具严格解绑**：技能是「Prompt 片段 + 使用说明」，不是工具集合；Agent 有无权限用某个工具由 ToolBinding 表决定，安装技能不会自动授予工具权限（反过来也一样）

---

# §4 硬约束 / 必守红线 / 扩展入口

**§4.1 必守红线（9 条，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 1 | **5 套 TEMPLATE 名常量严格对齐**：`TEMPLATE_COMMUNICATION` / `TEMPLATE_MEMORY_COGNITION` / `TEMPLATE_PROJECT_MANAGEMENT` / `TEMPLATE_SKILL_MANAGEMENT` / `TEMPLATE_TOOL_MANAGEMENT` 5 个常量名必须等于对应目录名 + skill.md `name` 字段 + SkillTag 枚举名；任一处不一致按模板找不到 = 安装为空 | 集成测试 5 套常量名 = 目录名 = skill.md name 字段 = SkillTag 枚举名（4 处全等） | [system/seed/skills/mod.rs 常量定义](src/service/domain/system/seed/skills/mod.rs) + common/src/enums/tool_tag.rs SkillTag 枚举 |
| 2 | **install_skill_pack 幂等性**：同一 agent_id + tag 连续调用 N 次，installed_count 第 2 次起必须为 0；数据库 AgentSkill 总数不增长（不产生重复行） | 连续两次 install 断言第二次 installed=0 且 COUNT 无变化 | [domain/hr/agent.rs install_skill_pack 重名 skip 分支](src/service/domain/hr/agent.rs#L450-L470) |
| 3 | **入职绑定失败不阻断入职**：5 套中任一套出错只打 log_warn + 记录缺失清单；Agent 必须成功创建 + 返回成功响应；onboard_agent 失败不返回 Err | 故意让某套安装抛错集成测试：Agent 仍创建成功 + 响应 200 + 日志含缺失清单 | [domain/hr/agent.rs onboard_agent install 包裹 catch_unwind](src/service/domain/hr/agent.rs#L470-L490) |
| 4 | **System 技能发布红线**：is_system=true 预置技能禁止普通管理员 EDIT/DELETE；仅超级管理员可改；修改后 prompt_template 变 → Prompt Hash 变 → Agent 私有副本版本自动+1 重新发布时才升级 | 普通管理员账号调 update_skill is_system=true → 权限错误 | [domain/hr/skill.rs update_skill 权限分支](src/service/domain/hr/skill.rs#L35-L80) |
| 5 | **Skill 与 Tool 严格解绑红线**：install_skill_pack 内部绝不调用 create_tool_binding / 任何工具相关 DAO；技能安装成功后 Agent 的 ToolBinding 列表 COUNT 不变 | install_skill_pack 前后查 ToolBinding COUNT 断言相等 | [domain/hr/agent.rs install_skill_pack 内部 grep 工具关键字应为 0](src/service/domain/hr/agent.rs#L419-L520) |
| 6 | **Prompt Token 熔断永不抛错**：即使总 Token 超限（极端场景 50 个技能），也仅裁剪 + 打 log_warn，绝不中断 awaken 流程；Layer1+Layer2 总和必须 ≤ 5K（预算保障永不裁剪）；Layer3 注入按优先级排序后超出部分逐个跳过 | 构造 50 个超长 Prompt 技能后调用 awaken 断言 200 OK 且日志有熔断记录 | [dal/agent.rs build_full_prompt 裁剪循环](src/service/dal/agent/mod.rs) Prompt Builder 组装 |
| 7 | **find_by_tag 必须按 published 过滤**：未发布技能绝不通过 install_skill_pack 分发到 Agent；技能包「草稿态」不应污染正式安装 | 技能 status=Draft 时 find_by_tag 不返回；install 断言 installed_count=0 | [dao/skill/sqlite.rs find_by_tag WHERE 条件](src/service/dao/skill/sqlite.rs) |
| 8 | **Agent 私有技能深度拷贝**：create_agent_skill_private 必须拷贝 prompt_template + prompt_summary + tags 全量字段；禁止用 skill_id 外键引用（Agent 必须独立可定制不被全局更新牵连） | 全局 skill 更新 prompt_template 后 Agent 私有副本字段不变（断言相等旧值） | [domain/hr/skill.rs install_to_agent INSERT 列清单](src/service/domain/hr/skill.rs#L141-L190) |
| 9 | **TEMPLATE skill.md 必须有完整 6 字段**：name / description / tags[] / prompt_template / system_constraints / usage_scenarios；解析失败时 init_base_data 跳过打 log_error 不 panic 中断启动 | 5 套 TEMPLATE skill.md grep 6 字段完整无缺；故意破坏字段启动打 log_error 返回正常 | [domain/system/seed/skills/ 每个 skill.md 结构](src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md) |
| 10 | **import_default_skills_if_empty 必须在 init_base_data 内调**：consumer::init、handler、外部 migration 一律禁止；测试环境 init_full_test_env 走同样顺序 | grep import_default_skills_if_empty 调用点仅在 domain::init_base_data | [system/seed/mod.rs](src/service/domain/system/seed/mod.rs) 注入链路 |
| 11 | **写文件必须过 content_path + 防路径穿越**：SkillDal::write_file 先 target = base_data_path().join(content_path).join(filename) → canonicalize → 检查 target.starts_with(base_data_path())；否则 ErrorCode::PathTraversal | 故意构造 `../../etc/passwd` 文件名应返回 PathTraversal 错误 | [dal/skill/mod.rs](src/service/dal/skill/mod.rs) write_file 方法 |
| 12 | **Draft 副本 id 必须重生成**：禁止直接复用 Published skill.id，否则多 Agent 安装同模板 UNIQUE 冲突；正确做法：`format!("{}-{}", published.id, agent_id_short_hash)` | 集成测试两个不同 Agent 安装同模板，断言 skills 表两条不同 id 记录 | [domain/hr/skill.rs install_to_agent](src/service/domain/hr/skill.rs#L141-L190) id 生成逻辑 |
| 13 | **to_prompt_summary 禁止超过 200 字符/条**：超过则 clippy/测试告警；详细内容塞 skill.md 正文，唤醒时不放 | 5 套模板 to_prompt_summary 断言单条 ≤ 200 字 | [models/skill.rs to_prompt_summary](src/models/skill.rs#L88-L92) |
| 14 | **TEMPLATE_ 前缀模板技能绝不允许硬删除**：即使过时也 SkillStatus=Expired 软标记，DELETE 会破坏历史 Draft 副本的 parent_skill_id FK 约束 | 单元测试对某 TEMPLATE 调 delete 应返回错误或降级为 Expired | [domain/hr/skill.rs update_skill 权限分支](src/service/domain/hr/skill.rs#L35-L80) Expired 分支 |

**§4.2 扩展入口速查**

| 扩展需求 | 改动位置（N 处同步） | 参考锚点 |
|---------|---------------------|---------|
| 新增第 6 套 TEMPLATE（如 CodeReview / DataAnalysis 行业专用） | ① 新建 TEMPLATE_XXX 目录 + skill.md（6 字段结构对齐）→ ② seed/skills/mod.rs 追加常量 + include_str! → ③ HRDomain init_base_data INSERT 追加（幂等 LIKE 检查）→ ④ install_default_skill_packs 追加对应 install_skill_pack(tag) → ⑤ Prompt 预算 Layer3 SKILL_PRIORITY 表追加优先级 | [seed/skills/mod.rs 模板注册](src/service/domain/system/seed/skills/mod.rs) |
| Prompt 熔断层新增「按需加载技能」机制（大技能包只在特定 ThinkingScene 注入） | ① DefaultPromptBuilder 追加 build_scene_skills_prompt(scene, available_skills) → ② awakening.rs 场景匹配时替换默认 Layer3 注入（Awaken 全部 + IntentAnalyze 只 neural/memory）→ ③ SKILL_PRIORITY 表按场景拆 AWAKE_PRIORITY / INTENT_PRIORITY / SETTLE_PRIORITY | [awakening.rs 工具白名单 is_tool_allowed](src/service/domain/runtime/awakening.rs) |
| 技能版本升级通知（Agent 私有技能落后全局版本 N 个时提醒管理员升级） | ① AgentSkillPo 追加 global_skill_version 字段（记录安装时的全局版本号）→ ② SkillDomain.update_skill 时版本号+1 → ③ 新增 Handler list_outdated_agent_skills(skill_id) 查所有落后版本 Agent → ④ 前端技能详情页展示「落后版本 N 个，点此批量升级」 | [models/skill.rs AgentSkillPo 字段定义](src/models/skill.rs#L22-L120) |
| 技能包分享导入导出（跨组织迁移，含 Prompt 模板 + 标签 + 使用场景） | ① Handlers 新增 export_skill_pack(skill_ids) / import_skill_pack(zip) → ② 定义 SkillPackArchiveFormat{version, skills[]} YAML 结构 → ③ 导入时 name+tag 冲突走「重命名 / 覆盖 / 跳过」三选一策略 → ④ 导入事务完整（半成功能回滚） | handlers/hr/skill/ 新增目录 |
