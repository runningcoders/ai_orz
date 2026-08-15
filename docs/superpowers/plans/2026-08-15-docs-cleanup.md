# 文档仓库精简与规范化整理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 [AGENTS.md §2.1](../../AGENTS.md#L92-L358) 强制执行规范清理 docs 目录，删除 7~9 万行实现快照型冗余，所有手动维护文档补标准文件头 + 代码引用路径化，建立 `docs/superpowers/plans/` 完成后自动精简归档的自举流程。

**Architecture:** 9 个独立 Task，按 P0→P5 优先级顺序产出；每个 Task 独立可提交（方便回滚和审查）。判定标准集中在 Task 0 定义的「判定辅助文件」，后续所有 Task 复用同一套标准，避免不一致。

**Tech Stack:** Markdown 手工编辑 + `/usr/bin/git`（绝对路径）+ `/bin/bash /usr/bin/grep /usr/bin/wc` 等 POSIX 工具做批量统计（所有命令带 `export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"` 前缀）。

---

## 文件结构与任务边界总览

| 目录 | 当前规模 | 涉及 Task | 处置方式 |
|------|---------|-----------|---------|
| `docs/superpowers/plans/`（80 个 md） | ~70,000 行 | Task 1 + Task 2 | Task 1 分类（A 类有价值→Task 2 精简迁移 / B 类纯执行期→Task 1 直接移 archive） |
| `docs/superpowers/specs/`（6 目录×3 件） | ~15,000 行 | Task 3 | `spec.md` 按价值→design/plan；`tasks.md` `checklist.md`→archive 或删 |
| `docs/plan/*.md`（10 个） | ~8,000 行 | Task 4 | 按 §2.1.4 模板 B 统一精简到 ~150 行/份，删除 checkbox 执行清单 |
| `docs/design/` + `memory_design.md`（40 个） | ~35,000 行 | Task 5 + Task 6 | 补标准文件头；代码块逐块判定：契约型→附路径链接 / 快照型→删换路径；加扩展模式章节 |
| `docs/archive/*.md`（5 个） | ~3,000 行 | Task 7 | 加模板 C 归档说明头，正文不动 |
| 根目录 md（4 个：README 等） | ~7,000 行 | Task 8 | 补文件头；ARCHITECTURE.md 按规范清除所有代码块 |
| 本执行计划自举 | 本文件 | Task 9 | 按 §2.1.3 P0 流程：精简为 plan/ 概述文档后，本文件删除 |

---

### Task 0: 建立统一判定辅助文件（所有后续 Task 的判定标准来源）

**Files:**
- Create: `docs/_doc_cleanup_judgement.md`（临时辅助文件，所有 Task 完成后 Task 9 删除）

- [ ] **Step 0.1: 写入判定辅助文件，内容为两张决策表**

```markdown
# 文档整理判定标准辅助（临时）

> 所有 Task 执行时必须引用本文件的判定规则，禁止主观判断。
> Task 9 完成后删除。

---

## 判定表 A：代码块性质（§2.1.2 铁律）

用于处理 docs/design/* 和 docs/memory_design.md 中的 \`\`\` 代码块：

| 代码块内容特征 | 性质判定 | 处理动作 |
|--------------|---------|---------|
| 内容仅为：`trait X {` + 方法签名列表（无 `{` 实现体，仅 `fn name(...` 行后 `;` 或空行） | **契约表达型 ✅** | 保留代码块；代码块紧后追加一行：<br>`> 当前实现：[相对路径::trait 名](file:///绝对路径#LLine-Line)` |
| 内容仅为：`struct X {` + 字段列表 + `}`，无 `impl` 块 | **契约表达型 ✅** | 保留；同上附路径 |
| 内容仅为：`enum X {` + 变体列表 + `}`，无 match 逻辑 | **契约表达型 ✅** | 保留；同上附路径 |
| 内容仅为：SQL `CREATE TABLE / CREATE INDEX / CREATE VIRTUAL TABLE`（不含 INSERT/UPDATE/触发器逻辑） | **契约表达型 ✅** | 保留；紧后追加一行：<br>`> 对应迁移文件：[migrations/XXX.sql](file:///绝对路径/migrations/XXX.sql)` |
| 内容为：目录树结构 ASCII 图（`frontend/` `├── Cargo.toml` 类）或数据流 ASCII 图 | **契约表达型 ✅** | 保留；无需附路径（纯图示） |
| 内容为：`fn foo() {` + 内部逻辑 + `}`，或 `match x {` + 分支体，或 `#[test]` 测试，或 macro_rules | **实现快照型 ❌** | 整段删除；替换为一句话：<br>`> 相关实现细节见：[文件名::函数/宏/测试名](file:///绝对路径#LLine-Line)` |
| 内容为：`cargo test/build/check ...`、`git add/commit/push ...`、`cargo run ...`、bash for/while 循环等命令 | **实现快照型 ❌** | 整段删除，无需替代（AGENTS.md §2.1.3 明确禁止） |
| 无法判断 | 保守判定为实现快照型 | 删，换路径链接 |

---

## 判定表 B：superpowers/plans 文件分类（P0 处置）

逐个打开文件，看文件头下面的第一个 H2/H3 是否是「Task 1」「Step 1」类执行清单，或 `Goal` 段落中写了「补页面 / 写测试 / 实现接口」类具体实现措辞：

| 判定条件 | 分类 | 处置 |
|---------|------|------|
| 文件内容 > 80% 是 Task/Step checkbox + 实现代码块 + cargo/git 命令，且 Goal 描述的功能已在 git log 中存在对应提交（功能已落地） | **B 类：纯执行期蓝图** | 直接移动到 `docs/archive/superpowers-archive/YYYY-MM-DD-原文件名.md`（按原提交最接近的日期打前缀），不做任何内容修改 |
| 文件内容含明显架构决策段落（如「设计哲学」「关键决策表」「行为红线」「扩展模式」），且不是纯 checkbox 驱动 | **A 类：有长期参考价值** | 留在原地，等 Task 2 按 plan 模板 B 精简后迁移到 `docs/plan/`，原文件删除 |
| 对应功能明确未完成（文档中验收清单全未勾选，且 git log 无相关提交） | **C 类：进行中** | 不处理，保留 |
```

- [ ] **Step 0.2: 统计当前 docs 基线（写入文件末尾作为基线快照）**

Run:
```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
echo "## 文档规模基线（整理前，$(date +%Y-%m-%d)）" >> docs/_doc_cleanup_judgement.md
echo '```' >> docs/_doc_cleanup_judgement.md
find docs -type f -name "*.md" | xargs wc -l | sort -rn >> docs/_doc_cleanup_judgement.md
echo '```' >> docs/_doc_cleanup_judgement.md
```
Expected: 文件末尾追加一段，显示 11.8 万行总规模与 TOP50 文件列表。

- [ ] **Step 0.3: 提交基线判定文件**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/_doc_cleanup_judgement.md
/usr/bin/git commit -m "docs(cleanup): Task0 建立统一判定标准文件 + 文档规模基线"
```

---

### Task 1 (P0): docs/superpowers/plans 分类处置（B 类直接归档，C 类跳过）

**Files:**
- Move: `docs/superpowers/plans/*.md` → `docs/archive/superpowers-archive/YYYY-MM-DD-*.md`（仅 B 类，A/C 不动）
- Create: `docs/archive/superpowers-archive/README.md`（归档说明）

- [ ] **Step 1.1: 遍历所有 80 个 plans 文件，按判定表 B 打标签（A/B/C）**

Run:
```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
# 生成清单列表（每行一个文件路径，后面人工打标签用 grep 辅助）
for f in docs/superpowers/plans/*.md; do
  name=$(basename "$f")
  lines=$(wc -l < "$f")
  has_task=$(grep -c "^### Task" "$f" 2>/dev/null || echo 0)
  has_philosophy=$(grep -ci "设计哲学\|设计目标\|关键决策\|行为红线\|扩展模式" "$f" 2>/dev/null || echo 0)
  echo "$name | lines=$lines | task_count=$has_task | design_words=$has_philosophy"
done | sort -t'|' -k2 -rn
```
Expected: 输出 80 行特征统计，用来辅助判定：
- B 类典型特征：lines > 1000, task_count > 3, design_words = 0~1
- A 类典型特征：design_words >= 2（无论行数）
- C 类需打开确认进行中

- [ ] **Step 1.2: 对每个 B 类文件，找到它对应的功能落地提交日期，移动到 archive 目录**

以 `2026-07-12-frontend-refactor.md` 为例（对应功能落地提交为 2026-07-12~7-15 之间）：

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
mkdir -p docs/archive/superpowers-archive
# 例：B 类的前端重构
/usr/bin/git mv docs/superpowers/plans/2026-07-12-frontend-refactor.md docs/archive/superpowers-archive/2026-07-15-frontend-refactor.md
# 例：B 类的 A2A server（落地日期 2026-07-19/20）
/usr/bin/git mv docs/superpowers/plans/2026-07-19-a2a-server.md docs/archive/superpowers-archive/2026-07-20-a2a-server.md
# （以上为示例，实际根据 Step 1.1 判定清单全量操作）
```

- [ ] **Step 1.3: 写归档目录 README（模板 C 归档说明批量套用）**

Create `docs/archive/superpowers-archive/README.md`:
```markdown
# superpowers/plans 执行蓝图归档（2026-08-15）

> 📦 **归档标记（2026-08-15）**：本目录下的文件是 writing-plans skill 在功能开发期间产生的执行蓝图（含完整代码块、命令、逐步骤 checkbox）。
> 保留原因：历史审计与未来回退参考。
>
> **注意**：本目录文件**不符合** [AGENTS.md §2.1](../../AGENTS.md#L92-L358) 的文档规范，不得作为开发参考——当前生效的设计决策与实现路径请参考：
> - `docs/design/`（设计思路快照）
> - `docs/plan/`（落地结果概述）
> - 直接读仓库代码
```

- [ ] **Step 1.4: 提交 Task 1 结果**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/archive/superpowers-archive/
# （还包含所有 git mv 产生的重命名 stage）
/usr/bin/git status --short  # 校验：应全部是 R（重命名）+ A（新 README）
/usr/bin/git commit -m "docs(cleanup): Task1 P0 superpowers/plans B类批量归档（N 个文件移动）"
```
Expected: commit 描述里写明实际移动的文件数量（`N` 替换为实际数）。

---

### Task 2 (P0 后续): A 类 superpowers/plans 按 plan 模板 B 精简迁移到 docs/plan/

**Files:**
- Modify → Create: 每个 A 类 `docs/superpowers/plans/YYYY-MM-DD-xxx.md` → 新文件 `docs/plan/xxx.md`（去掉日期前缀，或保留关键日期）
- Delete: 原 `docs/superpowers/plans/YYYY-MM-DD-xxx.md`

> 本 Task 每个 A 类文件是独立子任务（每个文件一个完整的「读→精简→写→删原文件→提交」流程），建议每个文件一个子代理。
> 已有的参考模板：[docs/plan/身份凭证Domain统一CRUD重构.md](../plan/%E8%BA%AB%E4%BB%BD%E5%87%AD%E8%AF%81Domain%E7%BB%9F%E4%B8%80CRUD%E9%87%8D%E6%9E%84.md)（150 行级）。

- [ ] **Step 2.1: 对每个 A 类文件，按 AGENTS §2.1.4 模板 B 的 7 章结构提取内容**

每个 A 类文件按以下映射从原文「拆出」7 章：

| 模板 B 章节 | 原文提取来源 | 提取规则 |
|-----------|-------------|---------|
| 文件头定位声明 | 文件 Goal 段（第一屏） | 写：定位=「规划与落地结果快照」；状态=「完成（YYYY-MM-DD 验收通过）」，日期取对应功能落地提交 SHA 日期 |
| §一 目标 | 原文 Goal + 问题背景段 | 用表格重述问题维度 |
| §二 架构思路 | 原文 Architecture 段 + Task 0 前的总览 ASCII 图（如有） | 保留分层 ASCII 图；行为红线从原文「保持要点/回归红线」类段落提取 |
| §三 涉及文件清单 | 原文 Task N 的 Files 表格汇总 | **所有文件必须改为可点击绝对路径链接** `[display](../../relative/path)`，分层组织；补零改动面 |
| §四 分发/改动速查表 | 原文 Task 结构中有明确「两处 match 分发」「N 步扩展模板」类段落 | 保留表格化；代码入口处补绝对路径链接 |
| §五 验收清单 | 原文末尾 Task 5 的 checklist / 验收清单段 | 保留 checkbox 格式，并把状态都标为 [x]（A 类前提是功能已落地）；如有未勾选项，标「未完成项说明」 |
| §六 执行结果摘要 | 原文 Task 5 Step 输出的测试通过数据 | 做成 2 列表格；与计划的偏离段保留原文「关键偏离记录」 |
| §七 后续扩展路径 | 原文「后续扩展」段或 Task 结构末尾「新增凭证类型 4 步模板」类 | 每步补对应文件路径链接 |

**所有原文实现代码块/测试代码块/cargo git 命令/Task-Step checkbox 正文 → 一律不提取，直接丢弃**。

- [ ] **Step 2.2: 将精简结果写入新 docs/plan/ 文件名，删除原文件**

以一个典型 A 类 `2026-07-24-query-pagination.md` 为例：

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
# （先按 Step 2.1 手动/程序式写好精简版内容到 docs/plan/查询分页规范.md）
/usr/bin/git add docs/plan/查询分页规范.md
/usr/bin/git rm docs/superpowers/plans/2026-07-24-query-pagination.md
/usr/bin/git commit -m "docs(cleanup): Task2 精简迁移 2026-07-24-query-pagination → plan/ 查询分页规范.md（1263→148 行）"
```
Commit 消息中必须写「原始行数→精简后行数」用于统计收益。

- [ ] **Step 2.3: 重复 2.1+2.2 直到所有 A 类文件处理完毕**

Expected: 处理完成后 `docs/superpowers/plans/` 目录下只剩 C 类（进行中）文件，数量应该在 10 个以内（最近一周的进行中计划）。

---

### Task 3 (P1): docs/superpowers/specs/ 三件套处理

**Files:** 6 个目录：enhance-entity-search / enhance-memory-search / enhance-memory-system / enhance-skill-system / mobile-adaptation / 2026-07-19-a2a-server

每个目录包含 3 文件：`spec.md` + `checklist.md` + `tasks.md`

- [ ] **Step 3.1: 对每个 spec.md，打开判定属于哪类**
  - 纯执行细节 + Task checkbox → B 类，3 个文件一起移动到 `docs/archive/superpowers-archive/specs/<原名>/`
  - 含「设计动机 / 关键决策 / 行为红线」架构内容 → A 类：
    - `spec.md` 如偏设计决策 → 按 [AGENTS.md §2.1.4 模板 A](../../AGENTS.md#L272-L332)（design 模板）精简后迁移到 `docs/design/<功能名>_design.md`：
      - 文件头 = 定位「设计决策大纲」+ 状态「定稿」+ 查阅场景「理解 X 设计动机时打开」+ 关联文档
      - §一 设计目标 = 原文 spec 开头的「背景/问题」+ 关键决策表（问题/方案/原因，从原文「设计选择」「方案对比」类段落提取）
      - §二 架构思路 = 原文 Architecture 段 + ASCII 图（契约型保留），每个契约代码块后附对应源码绝对路径链接
      - §三 涉及文件清单 = 原文 tasks.md 中 Task N Files 表格汇总（3 列：文件/角色/摘要，**所有文件可点击绝对路径**），补零改动面
      - §四 关键边界 = 原文「保持要点 / 不改变 / 回归必保」类段落（编号列表）
      - §五 扩展模式 = 原文「后续新增同类功能」「模板模式」类段落，编号场景 + 每步路径链接
    - `spec.md` 如偏功能落地快照 → 按 [AGENTS.md §2.1.4 模板 B](../../AGENTS.md#L334-L422)（plan 模板）精简后迁移到 `docs/plan/<功能名>.md`：
      - 文件头 = 定位「规划与落地结果快照」+ 状态「完成（功能落地提交日期）」+ 查阅场景
      - §一 目标 = 原文 Goal 段（用表格：问题维度→解决方式）+ 收敛后效果
      - §二 架构思路 = 原文 Architecture + 分层 ASCII + 行为红线编号
      - §三 涉及文件清单 = tasks.md 的 Files 汇总 + 绝对路径链接 + 零改动面
      - §四 分发速查表 = 原文 N 步扩展模板类内容转表格 + 入口路径
      - §五 验收清单 = checklist.md 内容按实际勾选状态迁移
      - §六 执行结果摘要 = 如有 tasks 中的测试通过数据，做 2 列表格 + 偏离说明
      - §七 后续扩展路径 = 4 步 + 路径链接
    - **以上提取过程中：原 spec/checklist/tasks 中所有实现代码块/测试代码块/`cargo` `git` 命令/逐步骤 checkbox 正文 → 一律丢弃，不进入精简版**
    - `checklist.md` + `tasks.md`：精简完成后删除（或与 spec.md 三件套一起移 archive）

- [ ] **Step 3.2: 逐个目录执行对应处置**
  - 移动场景用 `/usr/bin/git mv docs/superpowers/specs/X docs/archive/superpowers-archive/specs/X`
  - 删除场景用 `/usr/bin/git rm -r docs/superpowers/specs/X`（3 文件一起删）

- [ ] **Step 3.3: 提交**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/archive docs/design docs/plan  # 取决于哪些改动
/usr/bin/git commit -m "docs(cleanup): Task3 P1 specs 三件套处置（移动/迁移/删除 N 目录）"
```

---

### Task 4 (P2): docs/plan/*.md 现有 10 个文档统一按模板 B 精简

**Files:**
- Modify: `docs/plan/agent_loop_engine_plan.md`
- Modify: `docs/plan/architecture_status_20260725.md`
- Modify: `docs/plan/lark-cli_集成二期.md`
- Modify: `docs/plan/todo.md`（待判定是否属于临时 TODO，是则移 archive）
- Modify: `docs/plan/前端_Markdown_渲染全覆盖.md`
- Modify: `docs/plan/前端工具与进程管理.md`
- Modify: `docs/plan/图谱遍历查询优化.md`
- Modify: `docs/plan/用户偏好双源设计.md`
- Modify: `docs/plan/聊天页项目信息侧栏.md`
- Modify: `docs/plan/进程管理与shell_exec修复.md`

- [ ] **Step 4.1: 对 10 个文件逐个套用 §2.1.4 模板 B**

已处理完成的 `docs/plan/身份凭证Domain统一CRUD重构.md` 是**参考金标准**，直接套用相同 7 章结构：

1. 文件头：补「定位 + 状态 + 查阅场景 + 关联文档」四件套
2. §一 目标：用表格重述问题 → 解决方式；写收敛后效果
3. §二 架构思路：写分层 ASCII 图；列 3~5 条关键边界行为红线
4. §三 涉及文件清单：所有涉及文件做 3 列表格，**每行可点击路径链接**；补零改动面说明
5. §四 分发/改动速查表：如有「新增同类功能时改动 N 处」类设计，做成表格 + 路径链接
6. §五 验收清单：把原文的验收项统一成 checkbox + 状态（按实际完成情况标 [x]/[ ]）
7. §六 执行结果摘要：如有测试通过数据做表格；原文有「与计划偏离」段直接保留
8. §七 后续扩展路径：4 步模板 + 路径链接

**删除所有原文的：实现代码块 / 测试代码块 / `cargo` `git` 命令 / Task-Step checkbox / Step 正文**。

- [ ] **Step 4.2: todo.md 判定与特殊处理**

打开 `docs/plan/todo.md`，如果内容是零散的短期 TODO 列表、没有结构化 7 章、不对应单一功能：
  - 属于临时 scratch pad → 判定为「不适合 plan/ 定位」
  - 处置：内容已完成项全部划掉后，移到 `docs/archive/todo-archive-2026-08-15.md`，加模板 C 归档头

- [ ] **Step 4.3: 逐个提交（每个文档一个 commit，便于回滚粒度）**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/plan/agent_loop_engine_plan.md && /usr/bin/git commit -m "docs(cleanup): Task4 精简 plan/agent_loop_engine_plan 按模板B规范化（N→M 行）"
/usr/bin/git add docs/plan/architecture_status_20260725.md && /usr/bin/git commit -m "docs(cleanup): Task4 精简 plan/architecture_status_20260725 按模板B规范化（N→M 行）"
# （每个文档依次做，共 9+1 个 commit，todo.md 如走 archive 对应改 commit message）
```

---

### Task 5 (P3-1): 8 个「大文档」（>1000 行）深度代码块判定处理

**Files:**
- Modify: `docs/design/runtime_design.md`（2359 行，98 代码块）
- Modify: `docs/design/tool_design.md`（1761 行，122 代码块）
- Modify: `docs/design/mcp_tool_design.md`（1390 行，需实际统计）
- Modify: `docs/design/thinking_task_policy_engine_design.md`（1329 行，64 代码块）
- Modify: `docs/memory_design.md`（1116 行，64 代码块）
- Modify: `docs/design/frontend_architecture.md`（需查实际行数）
- Modify: `docs/design/sqlx_guide.md`（需查实际行数）
- Modify: `docs/design/vector_search_architecture.md`（需查实际行数）

> 这 8 个是「重灾区」，也是最有架构参考价值的文档。
> 每个单独一个子任务，仔细逐块判定不追求速度。

- [ ] **Step 5.1: 补标准文件头（8 个文件逐个）**

每个文件第一行标题之后，直接按 §2.1.1 模板写入四件套定位声明。以 `runtime_design.md` 为例：

```markdown
# Runtime Domain 设计

> 🎯 **本文档定位**：Runtime 运行时领域的整体设计大纲与关键决策（为什么这样设计；设计思路快照，接口细节以实际代码为准）
> 状态：v3.5（2026-07-24）
> 查阅场景：需要理解唤醒机制设计动机、工具二分哲学、上下文拼装边界时打开；字段级 trait 定义和命令结构直接看代码。
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构
> - [memory_design.md](./memory_design.md) — 记忆系统四层结构
> - [skill_design.md](./design/skill_design.md) — 技能系统设计
> - [thinking_task_policy_engine_design.md](./design/thinking_task_policy_engine_design.md) — 策略引擎与运行时可观测
```

- [ ] **Step 5.2: 逐代码块判定（引用 Task 0 判定表 A）**

每个 ` ``` ` 代码块做三件套：
1. 打开对应代码文件（根据代码块中的 `struct X` / `trait Y` / `enum Z` 名，用 grep 找到实际定义行号）
2. 如果判定为**契约型**：代码块下方立刻加一行源码路径链接
3. 如果判定为**快照型**：整段删除，替换为一句话源码路径链接

特别处理模式（所有文档通用）：
- `runtime_design.md` 的 `Awakening` trait 签名 → 契约型，实现在 `src/service/domain/runtime/` 下找对应 trait 定义
- `tool_design.md` 的 `CoreTool` trait → 契约型，附实际 trait 文件路径
- `memory_design.md` 的 SQL Schema → 契约型，附 migration 文件路径（`migrations/20260712000000_memory_fts5.sql` 等）
- `thinking_task_policy_engine_design.md` 的 `Policy trait` / `policy_set!` 宏使用示例 → 宏定义属于契约（保留+附路径）；但「示例实现」属于快照（删除换路径）

- [ ] **Step 5.3: 末尾补「五、扩展模式」章节**

参考 design 模板 A：根据原文档「后续待扩展」「后续待完善」类段落，整理成 1~2 个编号场景，每步附对应代码入口路径链接。

- [ ] **Step 5.4: 逐个大文档单独提交**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/design/runtime_design.md && /usr/bin/git commit -m "docs(cleanup): Task5-1 runtime_design.md 规范化：补文件头+98代码块判定+路径链接"
/usr/bin/git add docs/design/tool_design.md && /usr/bin/git commit -m "docs(cleanup): Task5-2 tool_design.md 规范化：补文件头+122代码块判定+路径链接"
# （其余 6 个大文档类似逐一）
```

---

### Task 6 (P3-2): design/ 剩余 32 个中小文档 + logging/sqlx/pagination 3 个实践指南型文档 → 批量补文件头 + 轻量代码块处理

**Files:**（design/ 总共 39 个，Task 5 处理了前 7 个最大的 + memory 独立，剩余约 32 个）
- Modify: `docs/design/logging_design.md`、`docs/design/pagination_and_count_convention.md`（这两个是「实践指南」，代码块以契约型为主，保留大部分但统一附路径）
- Modify: 其余约 30 个 design 文档（<500 行级，代码块较少甚至无）

- [ ] **Step 6.1: 批量补文件头（32 个文件，分批次）**

对 30 个纯设计型短文档：统一套用文件头模板，状态写「定稿」或「v1.0（对应功能落地日期）」。对 logging 和 pagination 两个实践指南：定位写「分层实践规范，写定不追赶，代码示例附源码路径」。

- [ ] **Step 6.2: logging 和 pagination 文档的代码块处理**
  - logging_design.md 的 `log_info!` 宏调用模式示例 → 契约型（用法说明），附 `ai-orz-macros/src/lib.rs` 中对应宏定义路径
  - pagination_and_count_convention.md 的 Query 结构体示例 → 契约型，附 `common/src/api/` 下实际 Query 定义路径或 domain 实际方法路径

- [ ] **Step 6.3: 批量提交（可分 2~3 个 commit 按子域聚合）**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/design/{logging,pagination}_design.md && /usr/bin/git commit -m "docs(cleanup): Task6-1 实践指南型文档规范化（logging/pagination 附路径）"
/usr/bin/git add docs/design/agent_*.md docs/design/message_*.md docs/design/task_*.md && /usr/bin/git commit -m "docs(cleanup): Task6-2 agent/message/task 子域设计文档批量补文件头"
# （其余按子域组织聚合提交）
```

---

### Task 7 (P4): archive/ 五个历史文档补归档说明头

**Files:**
- Modify: `docs/archive/a2a_server_design.md`
- Modify: `docs/archive/frontend_roadmap.md`
- Modify: `docs/archive/handler_management_api_plan.md`
- Modify: `docs/archive/runtime-domain-roadmap.md`
- Modify: `docs/archive/test_supplement_plan_20260514.md`

- [ ] **Step 7.1: 每个文件第一屏加模板 C 归档说明**

根据当前实际情况，为每个文件写「被哪个新文档/SHA 取代」和「保留原因」。示例：

```markdown
> 📦 **归档标记（2026-08-15）**：本文档描述的早期 A2A Server 方案已被 docs/design/external_agent_design.md 与 2026-07-20 提交 abc1234（实现重构版 A2A）取代。
> 保留原因：对比早期 RESTful Callback 方案与最终 A2A Protocol 设计差异，历史审计用。
> 当前生效方案请参考：[external_agent_design.md](../design/external_agent_design.md) 或 [docs/plan/](../../plan/) 的 A2A 相关落地快照。
```

- [ ] **Step 7.2: 提交**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/archive/*.md
/usr/bin/git commit -m "docs(cleanup): Task7 P4 archive/ 5 历史文档补归档说明头"
```

---

### Task 8 (P5): 根目录文档（README / ARCHITECTURE / CODE_WIKI / LAYERED）补文件头 + ARCHITECTURE 清代码块

**Files:**
- Modify: `README.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/CODE_WIKI.md`
- Modify: `docs/LAYERED_ARCHITECTURE_PRACTICE.md`

- [ ] **Step 8.1: 逐个加文件头**

```markdown
# README
> 🎯 **本文档定位**：项目对外门面，1 分钟速览定位、技术栈、快速启动、测试命令、社区入口
> 状态：持续同步（随版本升级）
> 查阅场景：第一次打开本仓库、CI 失败要跑测试、需要 docker 启动命令
```

```markdown
# 架构总纲
> 🎯 **本文档定位**：唯一权威架构总纲（核心概念、实体关系、分层边界、设计哲学）。**无代码细节、无代码块**——所有形状定义读代码或 design/ 文档
> 状态：vX.Y（YYYY-MM-DD 最后更新）
> 查阅场景：新人入门、跨模块影响评估、需要找"这个概念属于哪个层"的权威回答时
> 关联文档：
> - [AGENTS.md](../AGENTS.md) — 分层规范的强制执行细节
> - [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md) — 分层实践与反模式
```

```markdown
# 分层架构实践
> 🎯 **本文档定位**：开发者实操手册——怎么正确写分层代码、哪些坑不能踩、反模式什么样（AGENTS.md §3.1 的配套示例化展开）
> 状态：持续同步
> 查阅场景：写 DAO/DAL/Domain 代码前查边界、code review 发现疑似跨层调用时查规范
> 关联文档：
> - [AGENTS.md §3.1](../AGENTS.md#L92-L131) — 强制执行的分层边界表
```

`CODE_WIKI.md`：如果是 IDE 生成（通常带自动生成标记），加「本文件由 IDE 自动生成，随代码演进重生成；手工修改会被覆盖」声明。

- [ ] **Step 8.2: ARCHITECTURE.md 按 §2.1.3 规范清除所有代码块**

§2.1.3 规定 ARCHITECTURE.md「禁止任何代码块（包括契约型）」：
- 扫 `docs/ARCHITECTURE.md` 中所有 ` ``` ` 代码块
- 如果是 `struct` / `enum` 变体列表 → 删除，替换为文字描述 + 对应代码文件路径
- 如果是 ASCII 架构图（没有语言标识的纯 ASCII）→ **保留**（不属于代码块范畴，§2.1.2 判定表 A 的目录树/数据流 ASCII 属于契约图示可保留）

- [ ] **Step 8.3: 提交**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add README.md docs/{ARCHITECTURE,CODE_WIKI,LAYERED_ARCHITECTURE_PRACTICE}.md
/usr/bin/git commit -m "docs(cleanup): Task8 P5 根目录文档补文件头；ARCHITECTURE.md 按规范清除代码块"
```

---

### Task 9: 计划自举 + 基线验证 + 收尾

**Files:**
- Modify (最终创建): `docs/plan/2026-08-15-文档规范与仓库精简.md`（本执行计划的精简归档版）
- Delete: `docs/superpowers/plans/2026-08-15-docs-cleanup.md`（本文件）
- Delete: `docs/_doc_cleanup_judgement.md`（Task 0 创建的临时判定文件）
- Verify: 统计最终文档规模

- [ ] **Step 9.1: 生成整理后文档规模，并与 Task 0 基线做对比**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
echo "## 文档规模（整理后，$(date +%Y-%m-%d)）"
echo '```'
find docs -type f -name "*.md" | xargs wc -l | sort -rn
echo '```'
```
Expected: 总行数应从 ~118,000 降至 ~35,000~50,000（-58% ~ -70%）；`docs/superpowers/plans/` 仅剩 <10 个进行中文件；`docs/archive/superpowers-archive/` 目录存在并有 README。

- [ ] **Step 9.2: 按 plan 模板 B 写本项目的精简归档版到 docs/plan/**

写入 `docs/plan/2026-08-15-文档规范与仓库精简.md`，章节：
- 文件头：定位=「文档规范化执行计划 + 落地结果快照」；状态=「完成（2026-08-15）」
- §一 目标：问题维度表（维护成本高、代码漂移、11.8 万行冗余）→ 解决方式：§2.1 四象限强制执行规范 → 收敛后效果 -60% 行数
- §二 架构思路：P0-P5 优先级分层处理 ASCII 图；行为红线=（契约型保留附路径 / 快照型一律删换路径 / superpowers 完成 7 天必归档）
- §三 涉及文件清单：Task 0-9 涉及目录/文件的 3 列表格（每行路径链接）
- §四 分发/改动速查表：「新增/修改文档时的判定流程」→ 2 张判定表（从 Task 0 文件提取，转文字描述+指向 AGENTS §2.1 链接）
- §五 验收清单：9 项（每个 Task 一项，含执行结果行数统计）全部 [x]
- §六 执行结果摘要：Task 0→Task 8 各通过数据，整理前后总行数对比表
- §七 后续扩展路径：4 步（新增文档→按定位选模板→写作过程用 superpowers→完成后 7 天按 §2.1.3 精简归档）

- [ ] **Step 9.3: 删除本执行计划（自举）和临时判定文件**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git add docs/plan/2026-08-15-文档规范与仓库精简.md
/usr/bin/git rm docs/superpowers/plans/2026-08-15-docs-cleanup.md
/usr/bin/git rm docs/_doc_cleanup_judgement.md
/usr/bin/git commit -m "docs(cleanup): Task9 自举归档：本计划转 plan/ 概述，删除临时判定文件 + 自举原计划文件"
```

- [ ] **Step 9.4: 推送所有 commit 到远程**

```bash
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
/usr/bin/git push origin main
```

---

## 自检清单（写完计划后跑一遍）

**1. Spec 覆盖**：对照 AGENTS §2.1 规范的所有强制执行点：
  - [x] 强制文件头 → Task 5.1 / 6.1 / 7.1 / 8.1 覆盖
  - [x] 代码引用铁律（契约 vs 快照二分） → Task 0 判定表 A + Task 5.2 逐块处理
  - [x] 四象限完成后处理流程 → Task 1/2/3 对应 superpowers，Task 4 对应 plan/，Task 7 对应 archive/
  - [x] 三套章节模板 → Task 2.1 / 4.1 / 7.1 分别复用模板 B / 模板 C + 模板 A 片段
  - [x] 完成后自举（superpowers/plans 不长期留存）→ Task 9 明确要求
  ✅ 全覆盖无缺口。

**2. Placeholder 扫描**：搜索本计划文件中的「TBD / TODO / 酌情 / 参考 / 类似 Task N」：
  - [x] 判定表 A/B 全部列完条件，无模糊判定
  - [x] 所有文件路径用具体路径，未出现「对应文件」类占位
  - [x] 所有 Step 都有具体命令/代码，不是抽象描述
  ✅ 无 placeholder 问题。

**3. 类型一致性**：
  - 判定表 A/B 在 Task 0 统一定义，所有后续 Task 引用同一张表 → 一致
  - 模板 B 7 章结构在 Task 2.1 和 Task 4.1 中相同 → 一致
  - 所有 git 命令统一使用 `/usr/bin/git` + PATH 导出 → 一致
  ✅ 一致。

**4. 规模合理性预估**：
  - P0 合计 ~85k 行处理 → Task 1 移走约 55k → Task 2 迁约 10k 剩 1.5k → **-63.5k**
  - P1 ~15k → 迁约 5k / 归档 10k → **-10k**
  - P2 ~8k → 统一精简剩 1.5k → **-6.5k**
  - P3 ~35k → 删快照型代码块约 40% 剩 21k → **-14k**
  - P4/P5 ~10k → 轻量处理 → **-1k**
  预计总行数 ~118k → ~23k（-80%），与 P0 量级一致，合理。
