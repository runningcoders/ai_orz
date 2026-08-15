---
kind: knowledge_card
name: 种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发
category: 基础设施
scope:
  - "src/service/domain/system/seed/**/*.rs"
  - "src/service/domain/system/seed/skills/**/*.md"
  - "src/service/domain/mod.rs"
  - "src/lib.rs"
  - "src/handlers/system/seed/**/*.rs"
source_files:
  - "src/service/domain/system/seed/mod.rs#L1-L16"
  - "src/service/domain/system/seed/defs.rs#L1-L223"
  - "src/service/domain/system/seed/embedded.rs#L1-L68"
  - "src/service/domain/system/seed/diff.rs#L9-L120"
  - "src/service/domain/system/seed/store.rs"
  - "src/service/domain/system/seed/default.rs"
  - "src/service/domain/system/seed/default.json"
  - "src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md"
  - "src/service/domain/system/seed/skills/TEMPLATE_MEMORY_COGNITION/skill.md"
  - "src/service/domain/system/seed/skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md"
  - "src/service/domain/system/seed/skills/TEMPLATE_SKILL_MANAGEMENT/skill.md"
  - "src/service/domain/system/seed/skills/TEMPLATE_TOOL_MANAGEMENT/skill.md"
  - "src/lib.rs#L97-L154"
  - "src/service/domain/mod.rs#L23-L45"
  - "docs/design/seed-config-migration.md"
  - "docs/plan/Agent管理集成测试.md"
  - "docs/wiki/zh/content/功能模块/用户与组织管理/系统初始化.md"
  - "docs/wiki/zh/content/功能模块/系统管理/种子数据管理.md"
  - "docs/wiki/zh/content/功能模块/AI%20Agent%20管理/技能包管理.md"
  - "docs/wiki/zh/content/核心模块/处理器层/System模块处理器/种子数据处理器.md"
---

# §1 概述与定位

本知识卡沉淀 AI Orz 的**种子配置（Seed）系统**与**系统两阶段初始化**架构：覆盖 5 套预置技能模板编译期嵌入（TEMPLATE_*）、SeedSnapshot 纯数据结构与 diff 增量导入算法、两阶段初始化（`init_all` 静态单例 → `init_base_data` 异步基础数据 → `aop init_all` 事件调度器启动）严格分离原则、以及 `init_all_base_data` 跨 domain 的域派发模式。

Seed 系统采用「纯工具箱 Domain」架构：seed 子模块只提供数据视图（snapshot 结构 + diff 算法 + 文件存储 + 编译期嵌入），不持有任何 DAL 引用，不调用其他 domain；跨 domain 的 DB 读写由 Handler 层编排各 domain 完成，保证 seed 算法可独立单元测试（零 DB 依赖）。

# §2 关键文件表

| 角色 | 路径 | 关键锚点 |
|------|------|----------|
| Seed 子模块入口（纯工具箱） | src/service/domain/system/seed/mod.rs | L1-L16 声明 default/defs/diff/embedded/store 5 子模块；pub use defs::*；不引用任何 DAL/domain |
| 核心数据结构 + 占位符常量 | src/service/domain/system/seed/defs.rs | L1-L223 `PENDING_INPUT/INHERIT_CURRENT/RANDOM_GENERATE` 三占位符；`SeedSnapshot {org/users/model_providers/agents/skills}`；`SeedDiff` Diff 报告；DiffEntry Same/Updated/New/Removed 四态；`SkillFileDef {content > ref_path > url}` 三优先级内容来源 |
| 编译期嵌入 5 套技能模板 | src/service/domain/system/seed/embedded.rs | L1-L68 `EMBEDDED_SKILL_FILES` 静态数组 5 条目：TOOL_MANAGEMENT/SKILL_MANAGEMENT/MEMORY_COGNITION/COMMUNICATION/PROJECT_MANAGEMENT；`read_embedded_file(ref_path)` 读取接口 |
| diff 纯函数算法 | src/service/domain/system/seed/diff.rs | L9-L120 `diff_snapshots(base, target)` 入口；`diff_vec + collect_changes` 通用对比；输出 `SeedDiff {summary: new/updated/same/removed 统计 + 字段级 FieldChange}` |
| 文件系统存储 CRUD | src/service/domain/system/seed/store.rs | 快照文件 CRUD + 路径穿越防护（禁止 `../`）；列出/读取/保存/删除 seed 快照 JSON 文件 |
| 默认模板编译期内嵌 | src/service/domain/system/seed/default.rs + default.json | `POST /seed/apply-default` 一键初始化数据源；与 Rust 二进制一同编译，无需外部文件 |
| 5 套 TEMPLATE_* 技能模板（含主文件 skill.md） | src/service/domain/system/seed/skills/TEMPLATE_*/skill.md | TEMPLATE_COMMUNICATION / TEMPLATE_MEMORY_COGNITION / TEMPLATE_PROJECT_MANAGEMENT / TEMPLATE_SKILL_MANAGEMENT / TEMPLATE_TOOL_MANAGEMENT 5 套完整技能定义 |
| 两阶段启动调用链 | src/lib.rs | L97-L154 `run()` 函数：①pkg::init_all → ②service::init → ③producer::init → ④consumer::init → ⑤service::init_base_data（第二阶段） → ⑥AOP 统计注入 → ⑦aop::init_all（第三阶段调度器启动） |
| init_all_base_data 域派发 | src/service/domain/mod.rs | L23-L45 `init_all_base_data()` 派发：system::init_base_data（cron triggers） + finance::init_base_data（内置工具同步），未来新域需在此注册 |
| Seed Handler 跨 domain 编排 | src/handlers/system/seed/*.rs | assemble_snapshot_from_db（拉取各 domain 数据→组装 Snapshot）/apply_snapshot_to_db（跨域 CRUD 编排）+ 10 个 HTTP handler（list/get_file/save/load/delete/diff/diff_files/get_default/apply_default） |
| Design 规范 | docs/design/seed-config-migration.md | 4 种导入策略、敏感字段占位符与字段级 diff 设计、模块结构图、架构原则（纯工具箱，不跨域调用）、8 API 列表 |
| Plan 集成测试（初始化对齐） | docs/plan/Agent管理集成测试.md | §init_full_test_env 启动顺序与 run() 严格对齐；基础数据缺步导致 cron baseline 断言为 0 的历史教训 |
| Wiki 系统初始化长文 | docs/wiki/zh/content/功能模块/用户与组织管理/系统初始化.md | 用户视角的两阶段启动解释 + System Initialization 流程说明 |
| Wiki 种子数据管理长文 | docs/wiki/zh/content/功能模块/系统管理/种子数据管理.md | Seed 导入导出 GUI 说明 + 4 策略选择指导 |
| Wiki 技能包管理长文 | docs/wiki/zh/content/功能模块/AI%20Agent%20管理/技能包管理.md | 预置技能（TEMPLATE_*）与 Seed 的关系 |

# §3 架构与约定

## 3.1 Seed 纯工具箱架构（不跨 domain 调用）

```
┌─────────────────────────────────────────────────────────┐
│  Handler 层 (编排所有 CRUD)                              │
│  src/handlers/system/seed/                              │
│  ├─ assemble_snapshot_from_db: organization+finance+hr   │
│  └─ apply_snapshot_to_db:  跨 domain 依次写入             │
└───────────┬─────────────────────────────────────────────┘
            │ 调用 Seed 纯函数 + 调用各 Domain CRUD
            ▼
┌─────────────────────────────────────────────────────────┐
│  Seed 子模块 (纯工具箱, 不持有任何 DAL 引用)              │
│  src/service/domain/system/seed/                         │
│  ├─ defs.rs: SeedSnapshot / SeedDiff / SkillFileDef 结构  │
│  ├─ diff.rs: diff_snapshots + resolve_password 纯函数     │
│  ├─ embedded.rs: 5 TEMPLATE 编译期 include_str!()         │
│  ├─ store.rs: 文件系统 CRUD（路径穿越防护）               │
│  └─ default.rs: get_default_snapshot() 内置模板           │
└─────────────────────────────────────────────────────────┘
```

**关键原则**：seed 模块只处理自己的业务逻辑，**不调用任何其他 domain**（agent/finance/organization 等）；跨 domain 的 DB 读取与写入由 Handler 层编排完成。这保证 seed 算法可独立单元测试（seed_test.rs 纯函数测试，无 DB 依赖）。

## 3.2 5 套 TEMPLATE_SKILL 编译期嵌入

通过 `include_str!()` 宏将 5 套技能模板在编译期嵌入二进制，无需部署时的文件系统：

| 模板名 | 对应 skill.md 路径 | 定位 |
|--------|-------------------|------|
| TEMPLATE_TOOL_MANAGEMENT | skills/TEMPLATE_TOOL_MANAGEMENT/skill.md | 工具注册/调用/诊断方法论技能包 |
| TEMPLATE_SKILL_MANAGEMENT | skills/TEMPLATE_SKILL_MANAGEMENT/skill.md | 技能安装/调试/版本管理技能包 |
| TEMPLATE_MEMORY_COGNITION | skills/TEMPLATE_MEMORY_COGNITION/skill.md | 长短期记忆保存/检索/沉淀技能包 |
| TEMPLATE_COMMUNICATION | skills/TEMPLATE_COMMUNICATION/skill.md | 多 Agent 协作/消息/渠道技能包 |
| TEMPLATE_PROJECT_MANAGEMENT | skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md | 项目拆解/任务分派/跟进技能包 |

`embedded.rs:EMBEDDED_SKILL_FILES` 静态数组注册表必须与 `skills/` 目录下文件**一一对应**，新增技能模板需要：新增目录+skill.md → 在 embedded.rs 添加 include_str! 条目 + list_embedded_skill_files_count 测试断言值+1。

## 3.3 SeedSnapshot 三占位符语义

| 占位符 | 含义 | 解析执行点 |
|--------|------|-----------|
| `PENDING_INPUT` | 导入时强制要求管理员填写（password_hash / api_key） | Handler 从 `sensitive_values` HashMap 中取值 |
| `INHERIT_CURRENT` | 保留 DB 当前值（配置回滚场景） | Handler 先调 domain 取 DB 当前值，传入 `resolve_*` 纯函数 |
| `RANDOM_GENERATE` | 随机生成并显示一次（临时密码） | `seed::diff::resolve_password` 内部生成，在响应中返回明文一次 |

敏感字段（password_hash / api_key）**永远不导出**；导出快照时统一填 `PENDING_INPUT` 占位符。

## 3.4 两阶段初始化严格分离

**第一阶段（同步静态初始化，无 DB IO）**：service::init()
- 调用 7 个 domain `init_all()`：hr + finance + organization + message + runtime + project + system
- 各域内部：DAO init → DAL init → Domain 单例注册（全是 OnceLock::set，idempotent）
- 特点：同步函数，测试中可用 `Once::call_once` 包装调用

**第二阶段（异步基础数据注入，DB IO）**：service::init_base_data() → domain::init_all_base_data() 派发
- system::init_base_data()：幂等注入 2 条系统级 cron triggers（agent_rest 4h / project_followup 1h）
- finance::init_base_data()：内置工具定义同步（代码新增内置工具 → 启动自动对齐 DB）
- 特点：async 函数，失败仅记 log_warn 不 panic（保证部分启动成功）

**AOP 调度器（第三阶段）**：在 init_base_data 之后才启动 `aop::init_all()` worker，避免基础数据初始化阶段的事件被「消费者已注册但订阅者未就绪」情况误吞。

**调用链严格顺序**：
```
pkg::init_all → service::init → producer::init → consumer::init →
service::init_base_data → AOP metrics hook inject → aop::init_all
```

# §4 硬约束与红线

1. **Seed 不跨 domain 调用红线**：`src/service/domain/system/seed/` 下所有模块的 `.rs` 文件中，**禁止**出现 `use crate::service::domain::agent` / `...finance` / `...organization` 等其他 domain 的 use 语句；禁止调用其他 domain 的任何方法。跨 domain CRUD 必须在 Handler 层编排
2. **5 套技能模板编译期嵌入红线**：预置技能模板**必须**通过 `include_str!()` 编译期嵌入二进制，**禁止**在运行时从文件系统读取默认技能（部署环境可能无 seed/skills 目录）；`list_embedded_skill_files_count()` 单元测试断言总数与实际条目匹配
3. **敏感字段永不导出红线**：`SeedSnapshot` 导出时，`UserDef.password_ref` 和 `ModelProviderDef.api_key_ref` 字段**绝对不能**包含真实密码或真实 API Key；统一导出为 `PENDING_INPUT` 占位符。任何 diff/export 路径出现真实明文敏感值 = 高危安全漏洞
4. **两阶段 init / aop 顺序红线**：启动阶段 **必须** 先执行完 `service::init_base_data()`（含 system cron triggers + 内置工具同步），才允许调用 `aop::init_all()` 启动事件调度器 worker；禁止颠倒顺序（否则 cron triggers 注册前调度器已开始轮询，首条基准断言 0）
5. **init_all_base_data 域派发红线**：新增业务域需要启动时注入默认数据时，**必须**在 `src/service/domain/mod.rs:init_all_base_data()` 中追加一行 `xxx::init_base_data().await;`；禁止各域在自己的 `init()`（同步函数）中执行 `.await` DB IO
6. **路径穿越防护红线**：store.rs 文件读写中必须校验用户传入的快照文件名中**不包含** `../` 或绝对路径；禁止直接拼接 `SEED_DIR.join(user_input)` 而不做净化（防止写入任意系统路径）
7. **SkillFileDef 内容优先级红线**：技能文件内容解析必须严格按 `content（内嵌）> ref_path（编译期内嵌引用）> url（运行时抓取）` 优先级；ref_path 不存在时走 embedded.rs 查询，URL 必须是 HTTPS（拒绝 HTTP 明文抓取）
8. **apply_default 幂等红线**：`POST /seed/apply-default` 多次调用必须结果一致（幂等）；对已存在 ID 的条目走 `Update(INHERIT_CURRENT)` 而非覆盖，避免管理员二次初始化破坏已有配置
