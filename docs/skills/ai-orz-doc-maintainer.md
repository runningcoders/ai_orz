# AI Orz 文档编写与精简归档 Skill（规范全文版）

> 🎯 本文档是 AI Orz 项目「文档单向引用模型」中 doc 一侧的 Skill spec：负责 design 决策快照 + plan 落地结果快照的**全生命周期**（新建 → 冻结 → 完成后精简归档）+ superpowers 执行蓝图归档精简。文档互引遵循**单向模型**：③ wiki / ④ RAG 卡（活文档）单向引用 ① design / ② plan（历史快照），历史快照**不**反向维护指向活文档的链接（会随 wiki 改版腐烂）。
>
> 简短入口版（可直接注册为 Trae Skill）见：[.trae/skills/ai-orz-doc-maintainer/SKILL.md](.trae/skills/ai-orz-doc-maintainer/SKILL.md)
>
> 平行 Skill（负责活文档：wiki 长文 + RAG 知识卡的代码同步，以及引用重定向）：[ai-orz-wiki-maintainer.md](docs/skills/ai-orz-wiki-maintainer.md) + [对应注册版](.trae/skills/ai-orz-wiki-maintainer/SKILL.md)
>
> 状态：v2.1（2026-08-16，单向引用模型重构：v2.0 的「四类双向互引闭环 + 占位回填协议」废弃——历史快照反向链活文档会腐烂，且占位路径反推同步任务造成两 Skill 耦合死锁；改为 wiki/RAG 单向指向 design/plan，design/plan 冻结不追、完成后精简归档且归档件不写引用）
> 查阅场景：任何需要写 design/plan 文档、需要精简归档 superpowers 执行蓝图、需要判断"一段代码能不能保留在文档里"、或需要把已完成的 design/plan 精简归档的时候打开；如果是 wiki 长文/知识卡的同步（跟随代码变更）请走 ai-orz-wiki-maintainer。
>
> 关联文档：
> - [AGENTS.md §2.1 文档编写与维护规范（强制执行）](AGENTS.md#L92-L358) — 本文档的 SSOT，所有细则以 AGENTS 为准；本 Skill 是把 AGENTS §2.1 翻译成可执行的 agent 工作流步骤 + 判定表 + 归档模板
> - [docs/skills/ai-orz-wiki-maintainer.md](docs/skills/ai-orz-wiki-maintainer.md) — 平行 Skill：活文档维护方；本 Skill 归档 design/plan 后通知它做引用重定向

---

## 一、适用范围与四象限总览

### 1.1 四类文档与单向引用模型（SSOT）

AI Orz 项目维护**四类文档**，分为两类性质：**活文档**（跟随代码变更持续更新）与**历史快照**（写定即冻结）：

| 类型 | 位置 | 回答问题 | 性质 | 维护者（两个 Skill 分工）| 典型体量 |
|------|------|---------|------|------------------------|---------|
| ① **Design** | `docs/design/*.md` | **为什么做**（设计决策 + 关键决策表）| 历史快照（写定不追代码）| 👉 `ai-orz-doc-maintainer`（本 Skill）| 单篇 200-400 行 |
| ② **Plan** | `docs/archive/plan-archive/*.md` | **怎么做 + 落地结果快照**（7 章骨架，无 checkbox/命令） | 历史快照（写定不追）| 👉 `ai-orz-doc-maintainer`（本 Skill）| 单篇 150-250 行 |
| ③ **Wiki 长文（百科）** | `docs/wiki/zh/content/` 8 大板块 | **是什么**（系统化人类百科，10 节目录 + cite + 来源） | **活文档**（跟随代码增量同步）| 👉 `ai-orz-wiki-maintainer` | 353 篇 ≈134k 行 |
| ④ **RAG 知识卡（总结+索引）** | `docs/wiki/knowledge/zh/`（+ 两个顶层模块 + E2E 子模块） | **总结 + 索引**（给 Agent RAG 召回的原子知识单元） | **活文档**（跟随代码增量同步）| 👉 `ai-orz-wiki-maintainer` | ~70 张 ≈3k 行，按 AGENTS §2.1.3 图谱法则 5 级决策合并/拆分；仅 Level 3 互补视角平行卡允许独立存在且必须显式关联声明，禁止裸重叠 |

**单向引用模型（v2.1 核心）**：

```
     代码变更（唯一输入信号）
        │
        ▼ wiki-maintainer 同步
  ③ Wiki 长文 ──cite──► ④ RAG 卡          ← 活文档区（持续更新）
        │                   │
     cite / source_files[] 单向指向          ← 唯一维护方向的跨区链接
        ▼                   ▼
  ① Design ◄──────────► ② Plan            ← 历史快照区（写定即冻结）
  （①② 之间互引允许：两者同期冻结，链接不腐烂）
  （①② 不反向链接 ③④：wiki 改版后冻结文档无法跟进 = 永久断链）
```

**职责划分的理由**：
- 代码变更后**只需**同步 ③④（wiki-maintainer 的事）——读者要"当前状态"看 wiki/RAG 卡
- ①② 是考古入口：读者要"当时为什么这么做"才主动打开 design/plan
- ③④ 的 cite / source_files[] 单向指向 ①② → 检索链路（RAG 召回 → 长文 → 源码 → 考古 design/plan）完整覆盖，**无需** ①② 反向再链一遍
- 功能完成 → ①② 按 §六 场景 D 精简归档（归档件不写任何引用）

传统 docs 四象限（wiki/design/plan/archive/superpowers）依然有效，下表是对四象限的补充视图（与上表不冲突，上表关注「互引链路」，下表关注「生命周期」）：

| 象限 / 目录 | 性质与维护方式 | 生命周期结束后的处理 |
|------------|--------------|-------------------|
| **`docs/wiki/`（知识百科 + RAG 卡）** | 【本 Skill 不碰！→ 由 ai-orz-wiki-maintainer 专管】跟随代码演进持续增量同步 | 永不归档 |
| **`docs/design/*.md`** | 手工写：§设计目标+§架构思路+§涉及文件表+§关键决策表+§行为红线+§扩展模式 | **写定不追赶代码现状**；接口不一致时以代码为准，文头补一句「决策快照，细节以代码为准」 |
| **`docs/archive/plan-archive/*.md`** | 手工写；禁止含 checkbox/命令/代码快照/失败测试块 | 写定后永不主动改；新版本功能则新建 plan 文档 |
| **`docs/archive/*.md`** | 历史方案归档 | 只进不出：文头加一句话归档说明，正文永不修改 |
| **`docs/superpowers/plans/*.md`**（执行蓝图，临时存在）| writing-plans skill 输出，仅开发期间有效 | **功能完成 7 天内必须处置（二选一）**：(a) 有参考价值 → 精简为 plan 7 章模板后迁 docs/archive/plan-archive/；(b) 纯执行期 → 删除或移 docs/archive/superpowers-archive 永久封存 |
| **`docs/superpowers/specs/*/`（三件套，临时存在）** | Spec 形式的开发需求 | 同上 | 完成后：spec.md 有设计决策 → 精简迁 design/ 或 plan/；tasks + checklist → 删除或封存 |

### 1.2 本 Skill 覆盖的动作（与 ai-orz-wiki-maintainer 严格隔离）

✅ 做：
- design 文档新建 / 修文头 / 补关键决策表 / 移除过期的代码快照
- plan 文档新建 / 从 superpowers 执行蓝图按 7 章模板精简落成
- archive 文档归档标记 / 从 superpowers 迁移历史方案
- superpowers 目录的 7 天过期处置（精简或封存）
- **已完成 design/plan 的精简归档**（场景 D：判定活规范 vs 历史决策 → 精简 → 移 archive，归档件不写引用）
- 任何文档中的「代码块性质判定」与「代码引用路径引导替换」
- AGENTS §2.1 的 4 件套文件头（🎯定位 / 状态 / 查阅场景 / 关联文档）补全
- 文档中 placeholder 清除（TBD / TODO / 酌情 / 参考 Task 等）

❌ **不做（转给 ai-orz-wiki-maintainer）**：
- `docs/wiki/` 下的人类长文更新
- `docs/wiki/knowledge/zh/` 下的知识卡创建
- 「根据代码变更同步更新 wiki」→ 直接转给 ai-orz-wiki-maintainer
- 在 design/plan 中维护指向 wiki/RAG 卡的链接（v2.1 废除：历史快照反向链活文档会腐烂；wiki/RAG 卡侧的 cite / source_files 已单向覆盖检索链路）

### 1.3 单向引用铁律（强制执行；doc 侧视角）

**路径格式约定**（AGENTS §2.1.2 相对路径统一格式，三环境通跳；与 wiki-maintainer 完全一致，四类文档统一规则）：
- 引用**源码文件**（.rs/.ts/.toml/.sql 等）→ 统一写 `相对路径#Ln-Lm`（如 `src/pkg/logging.rs#L15-L42`；design/plan 的涉及文件清单表 / 代码块路径引导都沿用此格式，GitHub 原生高亮 + IDE 文件级跳转）
- 引用**另外三类文档**（design/plan/wiki 长文/RAG 卡的 .md 路径）→ 统一写**相对仓库根路径**（如 `docs/design/x_design.md`；GitHub 原生解析 + IDE 可点 + 文档中心通跳——**一律相对路径，永不写本机绝对路径与 `file://` 伪协议**）

**⭐【路径格式硬约束】文档与 RAG 卡中所有路径引用（关联文档头部 / §三涉及文件清单 / 代码块路径引导）必须使用 AGENTS §2.1.2 相对路径格式（行号 `#Lx-Ly`）**：出现 `file:///` 绝对路径 / `file://` 伪协议 / legacy 冒号行号 → 执行结果 FAIL，改完再过。

**引用规则矩阵（v2.1 单向模型）**：

| 链接方向 | 位置 | 规则 | 维护方 |
|---------|------|------|--------|
| ③ wiki cite → ①② + ④ | wiki `<cite>` 区 | ✅ 强制（活→历史/活→活，唯一维护方向）| ai-orz-wiki-maintainer |
| ④ RAG 卡 source_files[] → ①②③ | 卡 YAML | ✅ 强制（同上）| ai-orz-wiki-maintainer |
| ① design ↔ ② plan | 两者文头「关联文档」段 | 🔸 可选（两者同期冻结，链接不腐烂；plan 强烈建议链对应 design，考古链路入口）| 本 Skill |
| ①② → ③④ | — | ❌ **禁止新建**（历史快照反向链活文档，wiki 改版后永久断链）；存量已有的此类链接**不要求回删**，但永不主动补 | — |

```
                        ┌──────────────┐
                        │   代码源码   │
                        └──────────────┘
                          ▲          ▲
                    涉及文件表   cite/来源/source_files
                          │          │
             ┌────────────┤          ├────────────┐
             │            │          │            │
             ▼            ▼          ▼            ▼
    ┌──────────────┐ ───────► ┌──────────────┐
    │   Design ①   │  ①②互引  │    Plan ②    │  ← 本 Skill 负责（历史快照，冻结）
    │  为什么·决策  │ ◄─────── │怎么做+结果快照│
    └──────────────┘          └──────────────┘
            ▲                     ▲
            │ cite / source_files[]（单向，不反向）│
            │                     │
    ┌──────────────┐ ───────► ┌──────────────┐
    │ Wiki 长文 ③ │ ───────► │ RAG 知识卡 ④ │  ← wiki-maintainer 负责（活文档）
    │ 是什么·百科 │  cite    │ 总结·索引·RAG│
    └──────────────┘          └──────────────┘

     ↑ 箭头只从活文档指向历史快照；①② 不指回 ③④ ↑
```

**本 Skill v2.0 旧结论已作废（v2.1 升级）**：v2.0 中「design/plan 文头强制列 wiki 长文 + RAG 卡路径（0 条 = FAIL）+ 占位路径回填协议」——**完全废弃**，改为上面的单向引用矩阵。检索入口统一收敛在 wiki `<cite>` 与 RAG 卡 `source_files[]`。

---

## 二、文件头元信息四件套（所有手动维护文档强制第一行标题后紧跟）

**无论 design / plan / archive，只要是手动维护（即不是 wiki 自动生成、也不是 AGENTS 这类架构总纲——但 AGENTS 本身也有文头注释）都必须加。**

```markdown
# [文档标题]

> 🎯 **本文档定位**：[一句话说明角色，属于哪个象限，精度是概览级还是细节级]
>
> 状态：[草稿 / 定稿 / vX.Y（YYYY-MM-DD）/ 归档 YYYY-MM-DD]
> 查阅场景：[一句话告诉读者什么时候应该打开本文档，否则直接读代码]
>
> 关联文档：
> - [上层权威文档](../../AGENTS.md) — 简要说明关联点
> - [横向 design/plan 文档](../design/xxx.md) — 简要说明关联点（①② 同为冻结快照，互链不腐烂；可选，plan 建议链对应 design）
```

> v2.1 说明：关联文档段**不再要求**列 wiki 长文 / RAG 卡路径（历史快照不反向链活文档）。指向 wiki/RAG 的检索入口统一在 wiki `<cite>` 与 RAG 卡 `source_files[]` 侧维护。

### 状态枚举只能用 4 个值

| 枚举值 | 使用场景 |
|--------|---------|
| `草稿` | design 设计阶段写，尚未落地；plan 规划中未验收 |
| `定稿` | design 设计落地完成，不再主动追赶代码现状 |
| `vX.Y（YYYY-MM-DD）` | 多版本演进的文档（如 runtime_design 等）；每次大幅变更加版本并写日期 |
| `归档 YYYY-MM-DD` | 只用在 docs/archive/；文头补一句话归档说明（被谁取代/保留原因/当前生效替代文档路径）|

### AGENTS §2.1 明确列出的「归档文档中必须删除的 writing-plans 产物」

| 残留类型 | 例子 | 处理动作 |
|---------|------|---------|
| Skill 执行期标记 | `For agentic workers: REQUIRED SUB-SKILL to execute this blueprint` | ✅ 从归档 plan 中整段删除 |
| Step / Task checkbox 执行清单 | `- [ ] Step 1.xxx / - [x] Task 2.1.xxx` | ✅ 一律删除；归档 plan 用 §5 验收清单的无状态表 |
| 具体实现代码快照块 | `impl trait for X { ... }` 整段、完整测试代码、cargo test 参数命令 | ✅ 一律删，替换为路径引导 |
| Placeholder | TBD / TODO / 酌情 / 参考 Task X / 如果需要则 / 待确认 | ✅ 逐一替换为确定性描述或删除该条 |

---

## 三、代码块性质判定表（强制执行；99% 的文档保留/删除争议用本表现场判定）

> AGENTS §2.1 的判断口诀：**"粘到编辑器里能直接编译/运行 ⇒ 实现快照型 ❌；只声明接口形状不含内部逻辑 ⇒ 契约表达型 ✅"**

| 代码块场景 | 判定 | 可以保留吗？ | 处理方式（如果判 ❌）|
|-----------|------|------------|-------------------|
| **1. trait 方法签名列表（无 `{...}` 实现体）** | 契约 ✅ | 可保留（但非强制） | 保留 → 必须紧跟源码路径引导 |
| **2. struct 字段列表** | 契约 ✅ | 同上 | 同上 |
| **3. enum 变体列表**（全是 `Variant(i32)` 或带字段声明但无 impl）| 契约 ✅ | 同上 | 同上 |
| **4. SQL `CREATE TABLE` Schema** | 契约 ✅ | 同上（设计文档常用，保留可读性强）| 同上 + 附 migrations/ 目录对应路径 |
| **5. 目录树 ASCII 图** / 数据流 ASCII 图（纯文本矩形图）| 契约 ✅ | 强烈建议保留（无对应源码，这是架构意图） | 不用附源码路径 |
| **6. 函数 `{ }` 内部逻辑 / match 分支体 / for 循环 / if 控制流** | 实现快照 ❌ | **禁止** | 删 → 一句话路径引导：`> 字段级逻辑见 [xxx.rs::func](src/.../xxx.rs#Lxx-Lxx)` |
| **7. 完整测试代码 / `cargo test` 具体参数 / `git commit` 命令 / bash 执行脚本** | 实现快照 ❌ | **禁止** | 删 → plan 文档 §6 执行结果摘要只写"X passed / 0 failed"表格，无命令细节 |
| **8. Superpowers 蓝图中的 Task/Step 流程 checklist**（带 `- [x]` 语法）| 实现快照 ❌（plan/archive 中禁止）| **禁止**（仅 superpowers 临时文档中允许存在） | 删 → 用 §5 验收清单 或 §6 执行结果摘要表格替代 |
| **9. design 文档中完整函数调用栈示例（能直接编译的）**| 实现快照 ❌ | **禁止** | 删 → 替换为 ASCII 数据流图（是契约型）或路径引导 |
| **10. 极短的 1-2 行 pattern（如 `foo(ctx, ..)` 签名示例，但没有 `{ }` 实现体）**| 契约 ✅（行内代码 token 不算代码块）| 允许保留 | 不需要单独代码块，直接写在行内 `foo(ctx, ..)` |

### 契约代码块保留时「源码路径引导」的标准写法

```
> 当前实现：[file.rs::StructName](src/path/to/file.rs#L起始行-L结束行)
```

- 必须是**相对仓库根路径 + `#Ln-Lm` 行号**（AGENTS §2.1.2 统一格式；GitHub 原生高亮 + IDE 可点 + 文档中心通跳——不再区分 doc 侧 / wiki 侧两套引用格式）
- 放在代码块**紧邻下一行**，不要隔开
- 允许省略行号，但不推荐（文档是决策快照，写行号方便未来读者核对"当时的版本是哪段"）

---

## 四、代码引用首选格式（优先级远高于贴代码块）

无论什么文档、什么场景，**优先用「文字短描述 + 可点击路径链接」**，不要贴代码块（哪怕是契约型也可以不贴，用路径引导替代更轻量、零维护、与代码同步）。

标准格式（与 AGENTS §2.1 完全一致）：
```
[简短描述](src/相对路径/到/file.rs#L起始行-L结束行)
```

- 文字描述要把"读者为什么要点这个链接"的动机写出来
- 路径是相对仓库根路径（如 `src/service/domain/finance/identity_credential.rs`），GitHub 原生高亮 + IDE 可点 + 文档中心通跳
- 行号范围**推荐写**（让读者跳到精确位置，而不是自己搜文件）

**错误对比**：
| 错（贴代码块，20 行） | 对（路径引导，1 行） |
|---|---|
| 完整 impl 块代码（20 行） | `> IdentityCredentialManage::update 分发逻辑见 [identity_credential.rs::update](src/service/domain/finance/identity_credential.rs#L80-L210)` |

---

## 五、章节模板（从 AGENTS §2.1 抽取，可当工作流用）

> 本 Skill 最常用的是**模板 B（plan/执行蓝图精简归档）**——每做完一个功能都要走一次。模板 A 写 design 时用。模板 C 归档时用。

### 5.1 模板 A：docs/design/*.md（设计决策文档）

```
# [模块名] 设计

> 🎯 本文档定位：[领域] 设计大纲与关键决策思路（为什么这样做）
> 状态：草稿 / 定稿 / vX.Y（YYYY-MM-DD）
> 查阅场景：理解设计动机、边界条件、扩展模式时打开；字段级跳代码
>
> 关联文档：
> - [AGENTS.md §X.X](../../AGENTS.md) — 适用的架构规范
> - [相关 plan 文档](../plan/xxx.md) — 落地实施计划与结果（若无则写「暂无对应 plan 文档」）

---

## 一、设计目标 / 设计哲学
（表格/对比/类比优先；回答"为什么做""核心原则"）

### 1.X 关键设计决策表（必须有）
| 问题 | 方案 | 原因（为什么不选 B）|
|------|------|-------------------|
| Q1 | A1 | ... |

## 二、架构思路（ASCII 分层图 / 数据流图；贴契约代码块时必须附路径）
```
ASCII 图
```
> 关键结构定义见：[mod.rs::Name](src/.../mod.rs#Lxx-Lxx)

## 三、涉及文件清单（按分层索引：DAO/DAL/Domain/Adapter/Models/Frontend/Common）
| 文件 | 角色 | 内容摘要 |
|------|------|---------|
| [path.rs](src/...) | DAO 层 | 负责什么数据访问 |
| **零改动面** | [说明] | — |

## 四、关键边界 / 行为红线（回归必保，编号列表 1 句 1 条）
1. xxx update 后必须触发 yyy；失败仅告警，不阻断
2. xxx delete 前必须做 zzz 引用检查，被引用时 Conflict 拒删

## 五、扩展模式（新增同类功能走哪条路）
### 5.1 [场景一]
步骤 1 → 改哪个文件 → 参考 [某现有变体](src/.../file.rs#Lxx-Lxx)
步骤 2 → 改哪个 match 分支 → ...
```

### 5.2 模板 B：docs/archive/plan-archive/*.md（规划 + 落地结果快照；本 Skill 最高频）

```
# [功能名] [重构/落地/优化]

> 🎯 本文档定位：[功能] 规划与落地结果快照（概览级，不含代码细节；字段级以代码路径为准）
> 状态：进行中（YYYY-MM-DD 启动） / 完成（YYYY-MM-DD 验收通过）
> 查阅场景：新增同类功能时回看「改动清单 + 扩展模式」即可
>
> 关联文档：
> - [相关设计文档](../design/xxx_design.md) — 设计动机；若无则写「暂无对应 design 文档（强烈建议补写）」
> - [AGENTS §X.X](../../AGENTS.md) — 适用的架构规范

---

## 一、目标（为什么做）
| 问题维度 | 解决方式 |
|---------|---------|
| 问题 1 | 方案 1 |

**收敛后效果**：一句话架构收益（如「trait 封顶 N 个方法，新增类型时 trait 零改动」）

## 二、架构思路（怎么做的）
```
上层（保持不变）
  │只改调用方式
  ▼
中层（收敛点）
  │差异分发
  ▼
下层（知识下沉）
```
**关键边界 / 行为红线（回归必保）**：1. … 2. …

## 三、涉及文件清单（每行带可点击路径 + 变更摘要）
| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [common/src/models/xxx.rs](common/src/models/xxx.rs) | 模型层 | 新增 N 个方法 + 2 枚举 |
| **零改动面** | 前端/DTO/路由/集成测试 | 对外契约不变 |

## 四、[分发点 / 改动入口] 速查表（新增同类功能第一站）
### 4.1 [改动点 1 名称]
| 现有分支 | 处理逻辑 | 新增类型时参考 |
|---------|---------|--------------|
| 类型 A | ... | 如需 xxx 走此分支体 |
> 代码入口：[file.rs::fn 尾段](src/.../file.rs#Lxxx-Lxxx)

## 五、验收清单（YYYY-MM-DD 达成情况，无 checkbox 语法用表格）
| 验收项 | 结果 |
|-------|------|
| 架构验收项 1 | ✅ 通过 |

## 六、执行结果摘要（YYYY-MM-DD，写表格不写命令）
| 模块 | 验证结果 |
|------|---------|
| common 单测 | X passed |
| 集成测试 | X 套全部 PASS |
| Clippy 双端 | 零错误 |

### 与计划的偏离（如有）
1. 偏离点 1（说明 + 影响评估）

## 七、后续扩展路径（4 步模板）
> **核心不变量**：trait/DTO/路由机制不动
1. common 模型：[path.rs](src/...) — 做什么
2. domain 分发：[path.rs](src/...) — 做什么
3. handler 目录：复制 [模板](src/...) 改字段名
4. 前端：api + 区块组件，复制 [模板](frontend/src/...)
```

### 5.3 模板 C：docs/archive/*.md（归档，只改文件头）

```markdown
# [历史文档标题]

> 📦 **归档标记（YYYY-MM-DD）**：本文档描述的方案已被 [新文档名或提交 SHA] 取代。
> 保留原因：[方便对比新旧设计思路 / 历史审计 / 未来回退参考]。
> 当前生效方案请参考：[../design/新文档.md](../design/新文档.md) 或提交 [abc1234](https://github.com/xxx/commit/abc1234)

[原正文保留不动，绝不修改]
```

---

## 六、完整执行 SOP（四大流程分场景）

### 场景 A：新建一份 design 文档
| Step | 动作 | 检查点 |
|-----|------|--------|
| A1 | 放对位置：`docs/design/<主题名>_design.md`（或与既有 design 一致的命名风格）| 路径正确 |
| A2 | 套模板 A（§5.1），填充 5 大章 + §1.X 关键决策表（必须有，不能缺）| 文件头四件套齐全；状态先写 `草稿` |
| A3 | 涉及文件清单表（§三）：**逐个写**当前真实存在的源文件路径 + 可点击链接；不要瞎编还没创建的文件 | 至少 50% 行有路径链接 |
| A4 | 判定所有代码块：契约型保留并附路径下引导 + 实现快照型删除并换路径引导（§三判定表）| 0 快照块 |
| A5 | 功能落地后：状态改 `定稿`/`vX.Y`；若已有对应 plan，在关联文档段补一条 plan 链接（①→②，冻结互链不腐烂）| 状态正确 |

### 场景 B：superpowers 执行蓝图 → plan 归档精简（最高频，7 步）
| Step | 动作 | 检查点 |
|-----|------|--------|
| B1 | 确认功能已**完全完成并通过验收**（否则不归档，保留在 superpowers）| 功能真的 done |
| B2 | 复制原 `superpowers/plans/<date>-<主题>.md` → 新文件 `docs/archive/plan-archive/<中文主题名>.md`（**去掉日期前缀**，命名按 AGENTS §文件落位与命名约定表；先备份再改）| **绝不原地修改**（防止精简错了没退路）|
| B3 | **删除所有 checkbox**（Task 1/ Step 1.x 全部）：§5 改成验收清单表格、§6 改成执行结果摘要表格、其它 section 删除；删除 writing-plans skill 的「For agentic workers」执行期标记段；删除所有 TBD/TODO/酌情/参考Task placeholder | 0 checkbox、0 skill 标记 |
| B4 | **删除所有实现快照代码块**：函数体/测试代码/cargo test 命令/git push 参数/失败测试块——全部删，按 §四代码引用首选格式替换为「文字短描述 + 路径链接」1 行 | 0 快照块 |
| B5 | **文件头四件套**：🎯定位写 plan 定位；状态写「完成（YYYY-MM-DD 验收通过）」；查阅场景写"新增同类功能时回看改动清单"；关联文档写相关 design + AGENTS §X.X | 四件套齐全 |
| B6 | 套模板 B（§5.2）补齐缺失章节：§一目标表、§二架构 ASCII 图+行为红线、§三涉及文件清单表（每行带可点击路径！）、§四分发速查表、§七后续扩展路径（4 步模板）| 7 章齐全 |
| B7 | 最后 `git rm docs/superpowers/plans/<原文件>`；或有价值 → 移 archive 路径；或纯执行期无价值 → 直接 git rm | 该计划的"原始蓝图"不再存在于 superpowers 目录 |

### 场景 C：文档「代码块合规性扫雷 + placeholder 大扫除」全仓
| Step | 动作 | 检查点 |
|-----|------|--------|
| C1 | 扫描所有 design + plan + archive md，统计每篇代码块数 + 行数 + 类型 | 得基础基线（便于给用户同步"释放了多少行"） |
| C2 | 代码块按 §三判定表逐个过：契约→留+加路径下一行引导；快照→删+换1行路径引导 | 0 快照残留 |
| C3 | grep 全仓 placeholder：`TBD\|TODO\|待确认\|酌情\|参考 Task\|如果需要\|如果合适` → 逐一消除 | 0 placeholder 命中 |
| C4 | 扫文件头四件套缺失：🎯定位 / 状态 / 查阅场景 / 关联文档——任一缺失则按文档性质补全 | 覆盖率 100%（AGENTS 和 wikis 除外，它们是自动生成或架构总纲）|

### 场景 D：已完成 design/plan 的精简归档（v2.1 新增；功能完成、文档转为纯历史时执行）

| Step | 动作 | 检查点 |
|-----|------|--------|
| D1 | **判定活规范 vs 历史决策**：被 AGENTS.md 正文 / 文档索引表持续引用的规范类 design（如 sqlx_guide、logging_design、api_protocol_convention 等）→ **留在 docs/design/ 不归档**（归档会打断 AGENTS 内链接）；其余已落地的历史决策 design + 已完成 plan → 进入归档流程 | 留档清单先给用户过目 |
| D2 | **精简**：按 §三代码块判定表清快照块、删 checkbox、清 placeholder（TBD/TODO/酌情等）、按模板 B 7 章骨架收敛（plan）| 0 快照块 / 0 checkbox / 0 placeholder |
| D3 | **移动**：`git mv docs/design/xxx.md docs/archive/design-archive/xxx.md`（plan 同理进 `docs/archive/plan-archive/`），文件头套模板 C 归档标记 | git mv 保留历史 |
| D4 | **归档件不写跨象限引用**：删除文头指向 wiki/RAG 卡的链接；design↔plan 互链可保留（同为冻结历史，考古链路）| 归档件自包含 |
| D5 | **通知 wiki-maintainer 重定向**：报告被归档文件的旧路径→新路径映射，由 ai-orz-wiki-maintainer 批量改写 wiki `<cite>` 与 RAG 卡 `source_files[]` 中的旧路径 | wiki/RAG 侧 0 断链 |

---

## 七、与 ai-orz-wiki-maintainer 的协作接口

经常遇到「我改完代码，既需要写 design/plan 文档，又需要同步 wiki 长文+知识卡」的场景。**两者不合并，协作是单向解耦的：**

```
（1）新代码写好 → 跑通测试 → 推送 commit ✅
（2）调用 【ai-orz-doc-maintainer】→ 落地 design 文档 + 把 superpowers 蓝图精简成 plan ✅
       ↳ 只写自己的四件套 + design↔plan 互链；不写任何指向 wiki/RAG 的链接
（3）调用 【ai-orz-wiki-maintainer】→ 同步 wiki 长文 + 知识卡 ✅
       ↳ 在 ③ wiki 长文的 <cite> 区、④ RAG 卡的 source_files[] 中写 ① design + ② plan 的真实相对仓库根路径
       ↳ wiki-maintainer 自己 grep docs/design/ + docs/archive/plan-archive/ 找对应主题的文档，找不到就不写（不强制）
（4）后续 design/plan 被归档（场景 D）→ doc-maintainer 报告路径映射 → wiki-maintainer 重定向 cite/source_files
```

**v2.1 变更要点**：
- 废弃 v2.0 的「占位路径 + 最后执行方回填」协议——那会造成两 Skill 双向耦合与死锁预防成本
- 顺序仍然推荐 doc → wiki（design/plan 先存在，wiki 的 cite 才有东西可指），但**不再是硬依赖**：wiki-maintainer 找不到对应 design/plan 时允许留空，之后 doc 补写文档时 wiki 下次同步自然补上
- 两 Skill 无任何"等对方"的交叉点，各自可独立执行

---

## 八、自审校验清单（每次文档修改完成后的 checklist）

- [ ] 四件套文件头：🎯定位 / 状态（枚举正确）/ 查阅场景 / 关联文档，全部齐全？
- [ ] 单向引用合规：**本次新建的 design/plan 中没有新增指向 wiki/RAG 卡的链接**（存量不要求回删）？
- [ ] 代码块性质判定：**0 个实现快照块**（函数体/测试/命令/checkbox）残留？
- [ ] 所有契约代码块，紧邻下一行都有 `> 当前实现：[相对路径#Ln-Lm]` 引导？
- [ ] 所有文档中的路径链接 100% 指向真实存在的文件？
- [ ] Placeholder：0 命中（TBD / TODO / 酌情 / 参考 Task）？
- [ ] superpowers 下超 7 天的旧蓝图——有没有处置（迁移 plan / 删 / 封存）？
- [ ] Plan 文档 §三 涉及文件清单——**每一行都有可点击路径 + 变更摘要**（不能有裸文件名）？
- [ ] Archive 文档——文头加了归档说明一句话？正文没改？
- [ ] （场景 D 专用）归档件已断开 wiki/RAG 链接、已向 wiki-maintainer 报告路径映射？
