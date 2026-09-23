---
kind: RAG 原子知识卡
name: sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）
category: 运维工具 / 二进制工具
scope:
- tools/src/seed_sync.rs
- tools/src/bin/sync_seed_skill.rs
- scripts/ai_orz.sh（seed-sync 子命令）
- scripts/check.sh（seed-sync 命令实现）
- src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md
- src/service/domain/system/seed/default.json
source_files:
- tools/src/seed_sync.rs#L1-L305（纯逻辑层：parse_seed_json / SeedFileDef / SeedSkillDef / Args / SeedFileSource 枚举 + 校验 + 标签 JSON 生成）
- tools/src/bin/sync_seed_skill.rs#L1-L546（二进制入口：文件系统 + SQLite 编排——读取仓库 skills/**/*.md → 比对运行期数据目录 → 增量导入 → 写 seed_skill_versions 表）
- scripts/ai_orz.sh（seed-sync 子命令：SKILL= APPLY= 透传给 check.sh）
- scripts/check.sh（seed-sync 实现：调用 sync_seed_skill bin → 默认 dry-run，APPLY=1 写盘，SKILL=<ID> 限定单个）
- src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md（2026-09-23 增量：用户接待技能补 Agent 回执的用户代理判定）
- src/service/domain/system/seed/default.json（2026-09-23 增量：种子 Agent 默认配置扩展）
- tools/Cargo.toml（新增 seed-sync binary）
- docs/wiki/zh/content/功能模块/系统管理/种子数据管理.md
- docs/wiki/knowledge/zh/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发.md
- docs/wiki/knowledge/zh/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md
---

## §1 概述

**本卡角色**：sync_seed_skill 预置技能同步工具的独立知识卡。覆盖 tools/src/seed_sync.rs（纯逻辑层：Args / SeedFileDef / SeedSkillDef / parse_seed_json / SeedFileSource 枚举）、tools/src/bin/sync_seed_skill.rs（二进制入口：文件系统 + SQLite 编排——读仓库 skills/**/*.md → 比对运行期 data_dir → 增量导入 → 写 seed_skill_versions 表）、scripts/check.sh + ai_orz.sh 的 seed-sync 子命令（APPLY=1 写盘、SKILL=<ID> 限定、默认 dry-run）。**定位：排查部署根数据目录里预置技能版本过期、新增 TEMPLATE 技能包需要同步到运行期、脚本执行报错需要调试时读。**

- **两阶段分工**（纯逻辑层 vs 二进制入口）：与 `docs_migrate` / `migrate_tool_call_trace` 同一套架构——`tools/src/seed_sync.rs` 只处理 JSON 解析 / 技能定义 / 标签生成（不碰 I/O），`tools/src/bin/sync_seed_skill.rs` 管文件系统 + SQLite 编排；便于单元测试覆盖逻辑层、集成测试覆盖端到端。
- **运行期数据目录增量导入**：仓库 `src/service/domain/system/seed/skills/TEMPLATE_*/skill.md` 是源码事实源，部署根 `~/.ai_orz/data/skills/` 是运行期目录。sync_seed_skill 对比两者版本（或内容哈希），只导入差异部分——避免覆盖运行期 Agent 已定制的技能副本。
- **dry-run 默认安全**：不传 APPLY=1 时只打印计划操作不写盘；SKILL=<ID> 限定单个技能包同步；Makefile / ai_orz.sh / check.sh 三处入口统一转发。

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| tools/src/seed_sync.rs | 纯逻辑层 | Args（SKILL/APPLY/REPO_ROOT/DATA_DIR 参数）+ SeedFileSource（RepoEmbedded / RepoSkillsDir / DeployedSkillsDir 三来源）+ SeedFileDef / SeedSkillDef 结构体 + parse_seed_json 解析函数 + tags_to_json 标签 JSON 生成 | `:L1-L305` |
| tools/src/bin/sync_seed_skill.rs | 二进制入口 | 读仓库 TEMPLATE_*/skill.md → 比对 data_dir/skills/ 下同名文件 → 决定 ADD / UPDATE / SKIP → 写 seed_skill_versions 表；默认 dry-run 只打印 | `:L1-L546` |
| scripts/check.sh | seed-sync 命令实现 | 调 sync_seed_skill bin + APPLY=1 写盘 / SKILL 限定 | 见 check.sh |
| scripts/ai_orz.sh | seed-sync 子命令转发 | `ai_orz.sh seed-sync [SKILL=<ID>] [APPLY=1]` | 见 ai_orz.sh |
| Makefile | seed-sync make 目标 | `make seed-sync SKILL=<ID> APPLY=1` | 见 Makefile |

**章节来源**
- [seed_sync.rs](tools/src/seed_sync.rs)
- [sync_seed_skill.rs](tools/src/bin/sync_seed_skill.rs)

## §3 架构约定

本卡按 **Level 5（纯新主题）** 保留——scope 完全不与现存 RAG 卡重叠（现存卡覆盖 src/service/domain/system/seed/ 源码目录，但 **不覆盖 tools/** 目录下的二进制工具代码）。语义别名自检：「种子配置与系统两阶段初始化」和「Skill 系统增强」两张卡覆盖 seed 源文件编译期嵌入 + install_skill_pack 运行期加载，但本卡覆盖的是**运维期同步工具**（独立二进制 + 运行期数据目录），是互补而非同主题不同叫法。

**同步完整链路**：

```
仓库 seed 源文件（源码事实源）
  src/service/domain/system/seed/skills/TEMPLATE_*/skill.md
       ↓
  sync_seed_skill bin（tools/src/bin/sync_seed_skill.rs）
    ├─ 读仓库 skills/**/*.md
    ├─ 计算内容哈希 / 版本号
    ├─ 查 data_dir/skills/ 下同名文件
    │   ├─ 不存在 → ADD
    │   ├─ 哈希不同 → UPDATE
    │   └─ 相同 → SKIP
    ├─ [APPLY=1] 写 data_dir/skills/（只增量，不覆盖运行期定制）
    └─ [APPLY=1] 写 seed_skill_versions 表（记录当前版本）
       ↓
运行期 Agent 下一次入职时 install_skill_pack 读取更新后的 data_dir/skills/
```

## §4 硬约束与回归红线（4 条）

1. **sync_seed_skill 必须默认 dry-run，禁止默认写盘**：不传 APPLY=1 时只打印计划操作（ADD/UPDATE/SKIP 各计数），不触碰 data_dir。make seed-sync / ai_orz.sh seed-sync 默认都是 dry-run。
2. **SKILL=<ID> 限定必须精确匹配 TEMPLATE_* 目录名**：SKILL=TEMPLATE_COMMUNICATION 只同步 TEMPLATE_COMMUNICATION/skill.md，不涉及其他包；拼写错误时 sync_seed_skill 必须报错（panic 或 Err 返回），禁止静默跳过。
3. **增量导入禁止覆盖运行期已定制的技能副本**：若 data_dir/skills/ 下某技能存在但内容哈希与仓库不同 → UPDATE 只写差异部分（或标记为 CONFLICT 由人工处理），禁止整体覆盖；运行期定制的副本有专门字段标记，工具必须识别。
4. **纯逻辑层 seed_sync.rs 禁止 I/O**：所有文件系统 / SQLite 操作必须在 bin/sync_seed_skill.rs 入口；逻辑层只做 JSON 解析 / 结构体定义 / 校验；单元测试覆盖逻辑层，集成测试覆盖端到端。

## §5 历史演进（变更摘要）

- **tools/ 二进制工具框架**：tools/Cargo.toml 新增 sync_seed_skill bin + seed_sync.rs 纯逻辑层（与 docs_migrate / migrate_tool_call_trace 共享"逻辑层 vs 入口"两阶段分工模式）。
- **仓库 seed 源文件增量**：TEMPLATE_COMMUNICATION/skill.md 补 Agent 回执的用户代理判定；default.json 种子 Agent 默认配置扩展。
- **scripts 入口统一**：Makefile + ai_orz.sh + check.sh 三处新增 seed-sync 子命令；SKILL= / APPLY= 参数透传。
