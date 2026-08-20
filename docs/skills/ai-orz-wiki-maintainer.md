# AI Orz Wiki 维护 Skill（规范全文版）

> 🎯 本文档是 AI Orz 项目「文档单向引用模型」中 wiki 一侧的 Skill spec：负责把代码变更同步到人类百科长文 + RAG 知识卡（**全项目唯一的活文档维护方**），并通过 cite / source_files 单向引用 design / plan / 源码，保证读者从检索入口能一路跳到历史决策与真实代码。
>
> 简短入口版（可直接注册为 Trae Skill）见：[.trae/skills/ai-orz-wiki-maintainer/SKILL.md](.trae/skills/ai-orz-wiki-maintainer/SKILL.md)
>
> 平行 Skill（负责历史快照：design/plan 的全生命周期与精简归档）：[ai-orz-doc-maintainer.md](docs/skills/ai-orz-doc-maintainer.md) + [对应注册版](.trae/skills/ai-orz-doc-maintainer/SKILL.md)
>
> 状态：v2.1（2026-08-16，单向引用模型重构：废弃 v2.0「四类双向互引 + 占位回填协议」——改为本 Skill 单向维护 ③④→①② 引用；新增职责：design/plan 被 doc-maintainer 归档时批量重定向 cite/source_files 旧路径）
> 查阅场景：任何需要把代码变更同步进 docs/wiki/ 的两套知识库、或需要重定向因归档产生的失效引用时打开；如果是 design/plan 的生命周期管理（新建/精简/归档）请走 ai-orz-doc-maintainer。
>
> 关联文档：
> - [AGENTS §2.1 文档编写与维护规范](../DOCUMENTATION.md) — 文件头四件套 + 代码块性质判定铁律（四类文档全部遵守）
> - [docs/skills/ai-orz-doc-maintainer.md](docs/skills/ai-orz-doc-maintainer.md) — 平行 Skill：历史快照维护方；其场景 D 归档流程会触发本 Skill 的引用重定向

---

## 一、适用范围与核心不变量

AI Orz 项目维护**四类文档**，本 Skill 负责**活文档区**（跟随代码变更持续更新的 ③④），并通过单向引用覆盖完整检索链路：

| 类型 | 位置 | 回答问题 | 性质 | 维护者（两个 Skill 分工）| 典型体量 |
|------|------|---------|------|------------------------|---------|
| ① **Design** | `docs/design/*.md` | **为什么做**（设计决策 + 关键决策表）| 历史快照（写定不追代码）| `ai-orz-doc-maintainer` | 单篇 200-400 行 |
| ② **Plan** | `docs/archive/plan-archive/*.md` | **怎么做 + 落地结果快照**（7 章骨架，无 checkbox/命令） | 历史快照（写定不追）| `ai-orz-doc-maintainer` | 单篇 150-250 行 |
| ③ **Wiki 长文（百科）** | `docs/wiki/zh/content/` 8 大板块 | **是什么**（系统化人类百科，10 节目录 + cite + 来源） | **活文档** | 👉 `ai-orz-wiki-maintainer`（本 Skill）| 353 篇 ≈134k 行 |
| ④ **RAG 知识卡（总结+索引）** | `docs/wiki/knowledge/zh/`（+ 两个顶层模块 + E2E 子模块） | **总结 + 索引**（给 Agent RAG 召回的原子知识单元） | **活文档** | 👉 `ai-orz-wiki-maintainer`（本 Skill）| ~70 张 ≈3k 行，按 AGENTS §2.1.3 图谱法则 5 级决策合并/拆分；仅 Level 3 互补视角平行卡允许独立存在且必须显式关联声明，禁止裸重叠 |

**单向引用模型（v2.1 核心）**：

```
     代码变更（唯一输入信号）
        │
        ▼ 本 Skill 同步
  ③ Wiki 长文 ──cite──► ④ RAG 卡          ← 活文档区（本 Skill，持续更新）
        │                   │
     cite / source_files[] 单向指向          ← 唯一维护方向的跨区链接
        ▼                   ▼
  ① Design ◄──────────► ② Plan            ← 历史快照区（doc-maintainer，冻结）
```

**职责划分的理由**：
- 代码变更后**只需**同步 ③④——读者要"当前状态"看 wiki/RAG 卡
- ①② 是冻结的考古快照（"当时为什么这么做"），**不反向维护**指向 ③④ 的链接（wiki 改版后冻结文档无法跟进 = 永久断链）
- 本 Skill 是跨区链接的**唯一维护方**：cite / source_files 单向指向 design/plan；design/plan 被 doc-maintainer 归档时由本 Skill 重定向

### 1.2 两个 Skill 的职责分界与协作

单向解耦，无交叉等待点：
- 写 ③④（本 Skill）时在 ③ `<cite>` 引用区 + ④ `source_files[]` 字段中写清楚：对应 ① design 文档路径 + ② plan 文档路径（**自己 grep docs/design/ + docs/archive/plan-archive/ 按主题找**；找不到对应文档就留空，不强制、不占位、不等对方）
- 写 ①②（doc-maintainer）时**不写**任何指向 ③④ 的链接（v2.1 废除反向互引）
- **新增职责——引用重定向**：当 doc-maintainer 执行归档（场景 D，design/plan 移入 docs/archive/）并报告旧路径→新路径映射时，本 Skill 批量改写 wiki `<cite>` 与 RAG 卡 `source_files[]` 中受影响的旧路径，保证 ③④ 侧 0 断链

### 1.3 单向引用铁律（强制执行；本 Skill 负责的方向）

**路径格式约定**（AGENTS §2.1.2 相对路径统一格式，三环境通跳）：
- 引用**源码文件**（.rs/.ts/.toml/.sql 等）→ 统一写 `相对路径#Ln-Lm`（如 `src/pkg/logging.rs#L15-L42`；wiki cite 区和章节来源段 / RAG 卡 source_files[] 都沿用此格式，GitHub 原生高亮 + IDE 文件级跳转）
- 引用**另外三类文档**（design/plan/wiki 长文/RAG 卡的 .md 路径）→ 统一写**相对仓库根路径**（如 `docs/design/x_design.md`；GitHub 原生解析 + IDE 可点 + 文档中心通跳——**一律相对路径，永不写本机绝对路径与 `file://` 伪协议**）

**⭐【路径格式硬约束】文档与 RAG 卡中所有路径引用（cite 节 / 章节来源 / source_files[]）必须使用 AGENTS §2.1.2 相对路径格式（行号 `#Lx-Ly`）**：出现 `file:///` 绝对路径 / `file://` 伪协议 / legacy 冒号行号 → 执行结果 FAIL，改完再过。

**引用规则矩阵（v2.1 单向模型，只含本 Skill 负责的方向）**：

| 主体类型 | 在哪个位置引用 | 引用什么 & 写法 |
|---------|------|----------------|
| **③ Wiki 长文（本 Skill 职责）**| `<cite>` 引用区（与源码列表并列，放在源码条目之后）| ① design：`[文档标题](docs/design/x_design.md)`<br>② plan：`[文档标题](docs/archive/plan-archive/x.md)`<br>④ RAG 知识卡：`[卡名](docs/wiki/knowledge/zh/<卡目录>/同名.md)` — 同主题有平行卡时至少写 1 张<br>（①② 找不到对应文档可留空，不强制）|
| **④ RAG 卡（本 Skill 职责）**| `YAML source_files[]` 字段（源码之后、行号锚点同级）| ① design：`docs/design/x_design.md`（相对仓库根路径）<br>② plan：`docs/archive/plan-archive/x.md`<br>③ wiki 长文：⭐ **强制至少 1 条**对应长文相对仓库根路径（④→③ 是活文档区内部互链，必须闭环；同主题多组长文至少列主组 1 篇）<br>④（兄弟平行卡）同主题近似卡 0~N 张（按 AGENTS §2.1.3 关系声明）|

**①② → ③④ 方向由 ai-orz-doc-maintainer 侧禁止新建（历史快照不反向链活文档），本 Skill 不检查、不要求。**

```
                        ┌──────────────┐
                        │   代码源码   │
                        └──────────────┘
                          ▲          ▲
                    cite/来源      source_files
                          │          │
             ┌────────────┤          ├────────────┐
             │            │          │            │
             ▼            ▼          ▼            ▼
    ┌──────────────┐ ───────► ┌──────────────┐
    │ Wiki 长文 ③ │ ───────► │ RAG 知识卡 ④ │  ← 本 Skill 负责（活文档）
    │ 是什么·百科 │  cite    │ 总结·索引·RAG│
    └──────────────┘          └──────────────┘
          │ cite / source_files[]（单向，不反向）│
          ▼                     ▼
    ┌──────────────┐  ①②互引  ┌──────────────┐
    │   Design ①   │ ◄──────► │    Plan ②    │  ← doc-maintainer 负责（冻结快照）
    │  为什么·决策  │          │怎么做+结果快照│
    └──────────────┘          └──────────────┘

     ↑ 箭头只从活文档指向历史快照；①② 不指回 ③④ ↑
```

**引用覆盖率底线（每次 wiki 同步自审时检查）**：
- 本次新增的所有 RAG 卡 → 100% 至少有 1 条 wiki 长文相对仓库根路径出现在 source_files[] 中（0 条 = 失败）
- 本次新增的所有 Wiki 长文 → 100% cite 区至少 1 条 RAG 卡路径；design/plan 路径有则写（找不到对应文档可留空，不算失败）

**本 Skill v2.0 旧结论已作废（v2.1 升级）**：v2.0 中「design/plan 文头强制列 wiki/RAG 路径 + 占位回填协议 + 双向闭环」——**完全废弃**，改为上面的单向引用矩阵。

---

## 二、人类百科长文（zh/content/ 8 大板块）维护规范

### 2.1 单篇 5 段式固定骨架（顺序不可变）

样本参考：
- [AOP 核心架构.md](docs/wiki/zh/content/%E5%9F%BA%E7%A1%80%E8%AE%BE%E6%96%BD/AOP%20%E4%BA%8B%E4%BB%B6%E7%B3%BB%E7%BB%9F/AOP%20%E6%A0%B8%E5%BF%83%E6%9E%B6%E6%9E%84/AOP%20%E6%A0%B8%E5%BF%83%E6%9E%B6%E6%9E%84.md)
- [PO 与业务实体分层.md](docs/wiki/zh/content/%E6%9E%B6%E6%9E%84%E8%AE%BE%E8%AE%A1/%E5%88%86%E5%B1%82%E6%9E%B6%E6%9E%84%E8%AE%BE%E8%AE%A1/PO%20%E4%B8%8E%E4%B8%9A%E5%8A%A1%E5%AE%9E%E4%BD%93%E5%88%86%E5%B1%82.md)

```
# H1 页面标题（与最末层目录名语义对齐，用中文）
<cite>
**本文引用的文件**
- [显示名(可选中文注释)](src/相对路径/file.rs#Ln-Lm)
- …（源码路径 8-20 条；按"核心→次要→工具→测试"粗排）

**本文关联的文档（单向引用 v2.1：活文档 → 历史/兄弟文档）**
- [Design 文档标题（为什么做）](docs/design/x_design.md)
- [Plan 文档标题（怎么做+落地结果）](docs/archive/plan-archive/x.md)
- [RAG 知识卡：主题名（总结+索引）](docs/wiki/knowledge/zh/<卡目录名>/同名.md)
- （Design/Plan 在 docs/design/ + docs/archive/plan-archive/ 中 grep 主题找得到才写，找不到直接省略该行——不占位、不注"暂无"；RAG 卡同主题有多张平行卡时至少列 1 张主卡）
</cite>
「可选，仅增量更新场景」## 更新摘要
  **变更内容**
  - 本次变更 1（一句话+受影响端点/模块/路径）
## 目录（10 节编号锚点 1..10，不可缺编号）
1. [引言](#引言)
2. [项目结构](#项目结构)
3. [核心组件](#核心组件)
4. [架构总览](#架构总览)
5. [详细X分析](#详细x分析)  ← 按领域换：组件/接口/功能/工具参数
6. [依赖关系分析](#依赖关系分析)
7. [性能与X](#性能与x)      ← 按领域换：可维护性/速率限制/考量/容量限制
8. [故障排查指南](#故障排查指南)  ← 绝对不能缺！哪怕 2-3 条最小路径也要写
9. [结论](#结论)
10. [附录【：可选后缀】](#附录)

## 引言 / 简介
…

## 2-7 正文节（写完内容后，空一行写）
章节来源
- [fileA](fileA#Ln-Lm)

（若该节中插了 mermaid 图，在图闭合后立刻加）
图表来源
- [fileB](fileB#Ln-Lm)
```

### 2.2 8 条硬性约束

| 编号 | 约束 | 细则 |
|-----|------|------|
| H-1 | **引用区路径统一写相对仓库根路径 + `#Ln-Lm` 行号** | 不要写绝对路径 `/Users/...`，也不要写 `file://` / `file:///` 伪协议；GitHub 原生解析 + IDE 可点 + 文档中心通跳（AGENTS §2.1.2） |
| H-2 | **10 节编号锚点必须齐全** | 第 8「故障排查指南」是底线，哪怕只有 2-3 条最短路径也要写；新增页绝不能缺节 |
| H-3 | **节标题允许语义定制**（§5/§7） | 但编号必须严格 1..10；目录锚点必须与正文节标题完全一致（大小写/全半角）|
| H-4 | **每节末尾附「章节来源」纯中文标题段**（不加粗） | 只有第 1/8/9/10 节通常省略；mermaid 图后紧贴「图表来源」段（独立于章节来源） |
| H-5 | **mermaid 只允许 3 种图** | `graph TB/LR` 架构/组件关系；`sequenceDiagram` 时序调用；`classDiagram` PO/实体类图。**不允许写流程图、循环栈、参数枚举大图** |
| H-6 | **严禁任何代码快照块（除 mermaid 外）** | 与 AGENTS §2.1 一致：trait impl 体/函数逻辑/测试代码/命令参数/DTO完整定义 全部删，改为路径引导跳真源文件 |
| H-7 | **允许行内 1-2 个短 token 的 inline code 标记** | 如「接受 `limit` 与 `offset` 两个分页参数」——这不算"代码块"；不要展开成独立代码块 |
| H-8 | **新增页必须建 3 层目录同名 md** | 例：`基础设施/策略引擎/混合模式组合/混合模式组合.md`（目录名=md名=H1标题语义对齐） |

### 2.3 8 大板块新内容归属映射

| 新增内容类型 | 归属板块（8 大之一）| 子主题建议路径 |
|------------|------------------|---------------|
| 新 RESTful/A2A/SSE 端点 / DTO | API 参考 | `API 参考/<领域>/<动作>/<动作>.md` |
| 新 DAO/DAL/Domain 模块 / 流程编排 | 核心模块 | `核心模块/<处理器 7 大域|AOP|存储>/<子主题>/同名.md` |
| 新 pkg 基础设施（AOP/存储/JWT/日志/统计/边界检查）| 基础设施 | `基础设施/<领域>/<核心概念>/同名.md` |
| 跨层架构设计 / 约定 / 分层改造 | 架构设计 | `架构设计/<对应大架构>/<分支概念>/同名.md` |
| 新 Agent 工具 / 工作区 / 任务 / 记忆 / 技能 | 功能模块 | `功能模块/<工具生态|Agent管理|记忆系统|项目与任务>/同名.md` |
| 新增 PO / 枚举 / 跨层共享模型 | 数据模型 | `数据模型/<对应域模型>/同名.md` |
| 新前端页面 / 组件 / 样式主题 / 状态管理 | 前端应用 | `前端应用/<UI组件|页面模块|API客户端|架构设计>/同名.md` |
| 用户使用 / 开发者心智 / 排错 | 顶层两篇（开发指南 / 故障排除与监控）| 直接追加内容到已有 md 最后（或第 8 故障排查指南节内）|

### 2.4 增量更新时的「更新摘要」段写法

放在 `<cite>` 之后、`## 目录` 之前（全新页面不加）：

```markdown
## 更新摘要
**变更内容**
- 身份凭证 Finance Domain 统一 CRUD（`IdentityCredentialManage` trait 收口，按 `kind` 分发替代 N 组重复方法）
- 新增 GitHub 凭证 CRUD 前后端完整链：DTO+handler+前端页+Dao 占位
```

- 按 bullet 逐条写；不要写 commit SHA、不要写日期（git log 能查）；主要给未来读者判断"我要的能力在这页里有吗/加过没"。

---

## 三、Agent RAG 知识卡（knowledge/zh/）维护规范

### 3.1 知识卡 =「YAML Front Matter + 4 标准章节」固定模板

**绝对不能改结构**（RAG 解析引擎依赖这些字段和章节标题来做结构化召回）。

样本参考（最标准的两张）：
- [日志系统卡.md](docs/wiki/knowledge/zh/%E5%9F%BA%E4%BA%8E%20tracing%20%E7%9A%84%E7%BB%93%E6%9E%84%E5%8C%96%E6%97%A5%E5%BF%97%E7%B3%BB%E7%BB%9F%EF%BC%88%E5%AE%8F%20+%20%E8%87%AA%E5%8A%A8%E4%B8%8A%E4%B8%8B%E6%96%87%E5%AD%97%E6%AE%B5%E6%B3%A8%E5%85%A5%EF%BC%89/%E5%9F%BA%E4%BA%8E%20tracing%20%E7%9A%84%E7%BB%93%E6%9E%84%E5%8C%96%E6%97%A5%E5%BF%97%E7%B3%BB%E7%BB%9F%EF%BC%88%E5%AE%8F%20+%20%E8%87%AA%E5%8A%A8%E4%B8%8A%E4%B8%8B%E6%96%87%E5%AD%97%E6%AE%B5%E6%B3%A8%E5%85%A5%EF%BC%89.md)
- [统一错误模型卡.md](docs/wiki/knowledge/zh/%E7%BB%9F%E4%B8%80%E9%94%99%E8%AF%AF%E6%A8%A1%E5%9E%8B%EF%BC%9AErrorCode%20+%20ErrorType%20+%20ErrorField%20%E7%9A%84%E8%B7%A8%E5%B1%82%E9%94%99%E8%AF%AF%E5%A4%84%E7%90%86%E4%BD%93%E7%B3%BB/%E7%BB%9F%E4%B8%80%E9%94%99%E8%AF%AF%E6%A8%A1%E5%9E%8B%EF%BC%9AErrorCode%20+%20ErrorType%20+%20ErrorField%20%E7%9A%84%E8%B7%A8%E5%B1%82%E9%94%99%E8%AF%AF%E5%A4%84%E7%90%86%E4%BD%93%E7%B3%BB.md)

```markdown
---
kind: <snake_case 分类标签，如 workspace_security / identity_credential_unified_crud>
name: <中文一句话精准描述，也是 目录名.md 文件名，三者完全一致>
category: <通常与 kind 相同；跨类时写最主要类>
scope:
    - 'src/pkg/tool_registry/**'
    - 'src/pkg/paths.rs'
    - '<glob 模式数组：覆盖本卡锚定的源码文件范围，RAG用scope过滤相关文件>'
source_files:
    # ⭐ 单向引用 v2.1，顺序推荐：源码锚点 → wiki长文 → design → plan → 兄弟平行卡
    - src/service/domain/finance/identity_credential.rs#Ln-Lm          ← 源码锚点（3-8 个，可附 #Ln-Lm）
    - common/src/models/identity_credentials.rs#Ln-Lm
    - docs/wiki/zh/content/核心模块/Finance 处理器/身份凭证统一CRUD分发/身份凭证统一CRUD分发.md   ← ③⭐【强制至少1条】对应长文（④→③ 活文档区内部闭环）。若本主题长文在本次 SOP Step 6 中才会新建，先写最终目标路径，Step 6 完成后确保真实存在
    - docs/design/identity_credential_design.md                       ← ① design（有则写：grep docs/design/ 按主题找到才列；找不到省略）
    - docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md                          ← ② plan（同上，有则写）
    - docs/wiki/knowledge/zh/<同主题兄弟卡目录>/同名.md  ← ④（可选）同主题平行卡，按 AGENTS §2.1.3 关系声明（Level 3 互补卡必写）
---

## 1. 整体方案 / 使用的框架与工具
（1-3 段自然语言，讲清楚"本卡覆盖什么能力、解决什么问题、架构上的核心约束是什么"）

## 2. 关键文件与位置
| 文件 | 职责 |
|---|---|
| `相对路径A` | 一句话职责描述（源码文件） |
| `相对路径B` | 一句话职责描述 |
| `[对应 Wiki 长文名](docs/wiki/zh/content/<板块>/<路径>/同名.md)` | ⭐ 本卡对应的人类百科长文（推荐加一行；与 source_files[] 中强制 1 条 wiki 路径呼应，让人类读者一眼能跳）|
| …（4-12 行，与 source_files 对齐，但不必全相同；要给人类一眼扫到入口）|

## 3. 架构与设计约定
### 3.1 约定一
（文字描述，按 3.1 / 3.2 / 3.3 小节分；支持极短的 inline 代码 token；严禁独立代码快照）
### 3.2 约定二

## 4. 约定与约束 / 最佳实践 / 常见坑
- 强制规定 1：一句话 + 必要时附参考源码路径
- 强制规定 2：…
- （5-12 条 bullet；这张卡**真正的检索价值点**，Agent 召回后会优先读这一节）
```

### 3.2 7 条硬性约束

| 编号 | 约束 | 说明 |
|-----|------|------|
| K-1 | **YAML Front Matter 5 字段必须齐全（不允许增删字段）** | `kind / name / category / scope[] / source_files[]`——RAG 解析器强依赖 |
| K-2 | **`name` = 目录名 = 文件名**（三者完全一致，中文） | 例：目录「用户工作区 vs Agent 工作区」→ md 同名 → name 字段内容相同 |
| K-3 | **`scope[]` 写 glob 数组，不要写具体文件路径（写路径是 source_files 的职责）** | scope 用于 RAG 引擎在「用户传入一组文件」时快速匹配套路；`**` 表示所有后代 |
| K-4 | **`source_files[]` 写核心锚点 3-10 个** | 不要全列（长文 cite 才列全），要让 RAG 召回后通过这 3-10 个入口点继续反查源码；允许附 `#Ln-Lm` 行号后缀 |
| K-5 | **4 章节标题必须一字不差：1.整体方案 2.关键文件表格 3.架构与约定(3.X 子小节) 4.约定与约束bullet** | RAG 引擎可能按"第 4 节"语义匹配强制检索；改标题会破坏召回 |
| K-6 | **同主题多卡必须按 AGENTS §2.1.3 图谱法则判定关系** | 禁止裸重叠：Level 1 完全重复 → 合并+归档副卡；Level 3 互补视角 / Level 4 总细卡 → 允许平行保留但必须双向声明关联（source_files 互引 + §3 首句声明）|
| K-7 | **禁止写长文式的 10 节目录 / mermaid 图 / <cite> 区** | 知识卡是原子短卡，平均 40-80 行，§1 不超 3 段，§4 不超 15 条 bullet |

### 3.3 新卡 vs 更新旧卡的选择策略

| 场景 | 做法 |
|-----|------|
| 某主题卡描述的能力**彻底被重写**（核心流程/锚点文件 ≥70% 不同） | 新增一张平行卡，YAML `name` 加后缀区分，如「策略引擎（重构后：policy_set!宏混合）」 |
| 某主题卡描述的能力**新增了 1-2 个接口 / 字段**但核心流程未变 | 在原卡上原地增量更新：source_files[] +新路径；§4 约定与约束 +新 bullet；不要新建平行卡 |
| 某主题从来没覆盖过（新能力：GitHub DAO / 两阶段唤醒 / 工作区路径体系） | 必须新增一卡，不能硬塞到相近的卡中 |
| 卡的 YAML 中 source_files 指向的文件**已重命名/移动** | 原地改路径就行，不需要新卡（是路径修复，不是内容变更）|

---

## 四、完整执行 SOP（8 步）

> 说明：你作为 agent，当用户说「同步最近代码到 wiki / 更新 wiki 知识库 / 同步 XX commits 到 wiki」时，就按下面 8 步走。

| Step | 动作 | 交付物 / 检查点 |
|-----|------|----------------|
| 0. **前置查重（强制，不可跳过）** | 对每个候选主题按 AGENTS §2.1.3 图谱法则走 5 级决策算法（扫描现存卡做 name/category/scope 三维匹配 → Level 1 合并归档 / Level 2 优先合并 / Level 3 平行卡+互声明 / Level 4 总细卡+互声明 / Level 5 新建）| 判定结果表 + commit 消息带 §2.1.3.6 自我声明 |
| 1. 收集变更范围 | 确定 BASE_SHA→HEAD；用 `git log BASE..HEAD` 排除纯文档类提交（前缀 `docs`/`docs(cleanup)`/`docs(plan)`/`docs(readme)`/`docs(skill-communication)`），保留 feat / refactor / fix / test / style 类 | 提交清单（按前缀分组+作者+日期）|
| 2. 变更文件清单 | `git diff --name-only BASE..HEAD \| grep -v "^docs/"` 得变更文件 F，按模块聚合（`src/service/domain/*` 等） | TOP 模块分布表（给用户看最受影响板块）|
| 3. 候选长文页命中 | 反向 grep 353 长文的 `<cite>` 区 + 「章节来源」段：引用的路径 ∈ F → 标记待更新；再按模块语义补 TOP 板块根页（避免新模块根页漏更） | 候选长文清单（目标：353→≈40-60 页命中）|
| 4. 长文增量更新 | 按 §2 规范逐页更新：+cite/→新引用、+"更新摘要"段、+§5 详细内容、+每节来源路径指向最新行号、+更新 mermaid 图+来源 | 格式合规：H1+cite+[更新摘要]+10节目录+来源段 齐全；所有相对路径引用真实存在；0 代码快照（除 mermaid）|
| 5. **生成新知识卡**（本 SOP 核心）| **按本次代码变更的 TOP 语义主题生成**（不是按长文页 1:1 映射）。经验值：每 500-1500 行净代码变更 → 1 张新卡；每次同步典型产出 5-15 张。严格按 §3 模板：YAML 5 字段齐全 + scope 填正确 glob + 4 章节完整。**禁止裸重叠：与旧卡的关系必须经 Step 0 判定（Level 1 合并 / Level 2 合并或拆分声明 / Level 3-4 互声明平行保留）** | 新增 N 张卡；name/目录/文件名 三者一致；`git diff --name-only` 变更文件应被新卡的 scope 覆盖 ≥90% |
| 6. 创建全新长文页 | 若 5 步中出现了全新能力（知识卡已建，但 8 大板块中尚无对应长文）→ 按 §2.3 归属映射建目录+md+10节骨架+逐节写作 | 至少第 8「故障排查指南」2-3 条最短路径不缺 |
| 7. 提交推送 | commit 前缀：`docs(wiki): <范围> — 长文更新X页 + 知识卡新增Y张（BASE..HEAD 摘要）`；量大可按 4 板块拆 commits（长文基础设施/长文业务域/长文前端/知识卡）| 最终可推送（用户没说不推就默认 push）|
| 8. **引用重定向（事件触发，非每次）** | 当 doc-maintainer 报告 design/plan 归档路径映射（旧 `docs/design|x.md` → 新 `docs/archive/.../x.md`）时：全局 grep wiki `<cite>` 与 RAG 卡 `source_files[]` 中的旧路径 → 批量替换为新路径 | ③④ 侧 0 断链 |

---

## 五、失败时的回退路径（异常处理 SOP）

| 异常 | 判定 | 回退动作 |
|-----|------|---------|
| 候选长文命中后，某页「章节来源的行号全错」（指向的代码已经不在该行了）| 是 → 因为 BASE→HEAD 的变更移动了源码行 | 全部重新指向最新的行号；不要保留陈旧范围 — **宁可不写 #Ln-Lm 后缀，也不要写错误的范围** |
| 新知识卡的 scope 写太宽（`['**']`）导致召回噪音大 | 是 → scope 字段不允许 `['**']`（只有两张顶层模块 overview 卡允许这么写）| 收敛到本次变更真覆盖的 2-5 个 glob 模式 |
| 某篇长文 10 节目录和正文标题对不上（锚点失效）| 是 → 文档中心点击跳转会失败 | 快速修复：目录锚点的 `#xxx` 必须和标题的 `## xxx` 完全相同（中文、全半角、空格一致）|
| 用户说"这次只同步某一个 commit，不要跑全量 SOP" | 是 → 用户是权威 | 直接 Step 2（只拿该 commit 的变更文件）→ Step 3-7；跳过 Step 1 |

---

## 六、与其它 Skill 的协作边界

| 场景 | 应该调用的 Skill | 不应该调用本 Skill |
|-----|-----------------|------------------|
| 新功能开发完成后 → 写 design/plan 文档归档 | [ai-orz-doc-maintainer](.trae/skills/ai-orz-doc-maintainer/SKILL.md) | ❌ 本 Skill 是维护现状百科，不是写设计决策快照 |
| superpowers-archive 里的执行蓝图 → 精简为 docs/archive/plan-archive/ 7 章归档版 | ai-orz-doc-maintainer | ❌ 本 Skill 不处理 plan/archive |
| 已完成 design/plan 精简归档后 → wiki/RAG 引用重定向 | ✅ 本 Skill（SOP Step 8，被动触发）| ❌ doc-maintainer 不直接改 wiki/RAG 文件 |
| 用户说"更新最近代码到 wiki" | ✅ 就是本 Skill | ❌ 不要用 ai-orz-doc-maintainer |
| 用户说"给 agent 增加一张知识卡" | ✅ 本 Skill §3（只做知识卡那部分） | 不要求一起更新长文（用户明确说"只加知识卡"时）|

---

## 七、校验清单（每次 wiki 同步完成后的自审清单）

- [ ] **双知识库都更新了吗？** 至少 1 篇长文 + 至少 1 张卡（同步 0 长文或 0 卡 = 不完整）
- [ ] **Step 0 前置查重执行了吗？** 每个候选主题都有 5 级判定结果；Level 2/3/4 拆分卡的关联声明写齐了？
- [ ] **长文 8 硬约束**：H1+cite+目录 10 锚+每节来源+0 代码快照（除 mermaid），都满足？
- [ ] **知识卡 7 硬约束**：YAML 5 字段齐全 + name=目录=文件名 + scope glob 正确 + source_files 3-10 锚点 + 4 章节标题一字不差？
- [ ] **新增 RAG 卡 source_files[] 至少 1 条 wiki 长文路径**（④→③ 活文档区闭环）？
- [ ] **所有相对路径引用都存在吗？**（提取 cite 区 / 章节来源 / source_files[] 中的 `](路径)` 链接目标 → 逐个检查路径是否真实存在；必要时删掉损坏的路径或指向正确文件）
- [ ] **新增长文页挂对了 8 大板块目录吗？**（3 层目录同名结构）
- [ ] **提交 message 前缀是 docs(wiki): 吗？**（保持与 IDE 历史一致，便于后续 grep 范围）
- [ ] （Step 8 重定向专用）归档路径映射中的每条旧路径，在 ③④ 中已 grep 清零？
