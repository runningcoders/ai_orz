# docs/ 文档地图（模块化知识库）

> **Agent 实现功能需求 → 从 [AGENT_GUIDE.md](./AGENT_GUIDE.md) 进入**（五步流程 / 模块路由表 / status 语义 / 反模式）。
> **人 → 就用本文件**：怎么查、怎么改、怎么维护、怎么把活儿交给 Agent。
>
> 现状规模：**524 篇 / 15 个模块**（其中 `A-设计`+`B-落地方案` 43 篇，`C-知识` 481 篇）。全库清单直接开 [modules/INDEX.md](./modules/INDEX.md)。

---

## 一、30 秒上手

| 我想… | 去哪 |
|---|---|
| 找「XX 是怎么实现的」 | [modules/INDEX.md](./modules/INDEX.md) 按标题扫，或 §六 配方 1 |
| 只看某个模块有什么 | `modules/<模块>/00-INDEX.md` |
| 改某个文件前该读哪篇设计 | §六 配方 3（按代码路径反查 `scope`） |
| 碰某模块前先看有没有人在改 | §六 配方 6（在途方案） |
| 新接一个需求（Agent） | [AGENT_GUIDE.md](./AGENT_GUIDE.md) 五步流程 |
| 改完代码了 | §九 场景表 → `make wiki-check` |

---

## 二、三层结构与权威关系

文档按**能力域**拆成 15 个模块，每个模块一个文件夹，内含三层：

- `A-设计`：**为什么**这么设计（决策快照，写定后不追现状）
- `B-落地方案`：**要去哪**（规划 / 进度 / 任务拆解）
- `C-知识`：**是什么**（代码现状百科长文 + RAG 原子知识卡）

**冲突时的判据**（完整说明见 [AGENT_GUIDE.md §二](./AGENT_GUIDE.md)）：

1. `C-知识` 是**现状权威**——它描述代码现在的样子，一切以它为准。
2. `A-设计` 是**意图权威**——解释为什么是现在这样、边界在哪。
3. 两者冲突 → **以代码（`C-知识`）为准**，并回头把 `A-设计` 修掉或标"已被取代"。
4. `B-落地方案` **没有权威性**——它是进度快照，可能已过期，只用来判断"这块是否已有在途方案"。

---

## 三、索引产物（派生物，禁止手改）

三类索引**全部由脚本生成**，手改会在下次重建时被覆盖：

| 产物 | 面向 | 内容 |
|---|---|---|
| [modules/INDEX.md](./modules/INDEX.md) | 人 | 全库三层清单 + 设计状态 |
| [modules/INDEX.json](./modules/INDEX.json) | Agent / 工具 | 机器可读索引（含 `status` / `scope` / `related`） |
| `modules/<模块>/00-INDEX.md` | 人 | 每模块三层清单 |

> 目录即真源，索引是派生物：**改文档 → 跑 `make wiki-index`**，不要反向维护索引。
> `C-知识` 条目在 JSON 里只有 `tier/path/name/title/bytes`，没有 `status`/`scope`——它的"状态"由代码决定，不需要状态机。

---

## 四、15 个模块

| 模块 | 内容范围 |
|------|----------|
| [00-总览与架构](./modules/00-总览与架构/00-INDEX.md) | 跨模块架构 / 分层 / 安全 / 整体概览 |
| [01-组织与权限](./modules/01-组织与权限/00-INDEX.md) | 组织 / 角色 / 鉴权 / 用户凭证 / 联邦组网 |
| [02-Agent运行时](./modules/02-Agent运行时/00-INDEX.md) | Agent 唤醒 / 思考 / 运行时 / 策略引擎 |
| [03-记忆系统](./modules/03-记忆系统/00-INDEX.md) | 四层记忆 / 沉淀 / 休息 |
| [04-对话与消息](./modules/04-对话与消息/00-INDEX.md) | 消息 / 渠道 / 飞书 / 微信 / SSE / 召回 |
| [05-任务与项目](./modules/05-任务与项目/00-INDEX.md) | 任务 / 项目 / 看板 / 工作流 |
| [06-工具与技能](./modules/06-工具与技能/00-INDEX.md) | 工具三层调用 / MCP / 技能 / 凭据 |
| [07-外部Agent(A2A)](<./modules/07-外部Agent(A2A)/00-INDEX.md>) | A2A 协议 / 外部代理 / 回调 |
| [08-搜索](./modules/08-搜索/00-INDEX.md) | FTS5 / 向量 / 知识图谱 / 语义检索 |
| [09-统计与监控](./modules/09-统计与监控/00-INDEX.md) | DuckDB 统计 / 监控 / 仪表盘 |
| [10-异步基建(AOP)](<./modules/10-异步基建(AOP)/00-INDEX.md>) | 事件中心 / 消费者 / cron / 队列 |
| [11-前端应用](./modules/11-前端应用/00-INDEX.md) | Dioxus / HUD / Canvas / 组件 |
| [12-基础设施与部署](./modules/12-基础设施与部署/00-INDEX.md) | 部署 / Docker / SQLx / 配置 / 初始化 |
| [13-数据模型](./modules/13-数据模型/00-INDEX.md) | 实体 / PO / DAO / DTO / 分页 |
| [14-测试](./modules/14-测试/00-INDEX.md) | 测试指南 / E2E |

> 模块名含 `()` 的（07 / 10）写链接**必须**用 `](<path>)` 尖括号形式，否则 Markdown 会在首个 `)` 处截断。

---

## 五、常用命令速查

```bash
make help          # 忘了命令时：列出全部 target
make wiki-index    # 重建三类索引（改文档后必跑）
make wiki-check    # 提交前跑这个：索引一致性 + 元数据合规
```

| 命令 | 作用 | 什么时候跑 | 门禁 |
|---|---|---|---|
| `make wiki-index` | 重建 `INDEX.md` / `INDEX.json` / 各 `00-INDEX.md` | 新增·删除·改名文档、改 frontmatter 之后 | ✍️ 写盘 |
| `make wiki-check` | `--check` + `--meta` 聚合 | **每次提交前** | ✅ |
| `make wiki-lint` | 索引必须与目录一致 | 提交前（已含在 `check`/`lint`） | ✅ |
| `make wiki-meta` | `A-设计`/`B-落地方案` frontmatter 完整 | 新增设计 / 计划文档后 | ✅ |
| `make wiki-stale` | 列出文档里引用的**已失效代码路径** | 定期体检、想清理旧引用 | ❌ 纯诊断 |
| `make docs-lint` | 链接规范（禁 `file://` / 绝对路径 / 冒号行号） | 提交前（已挂 `make lint`） | ✅ |

**`make lint` 已包含** `docs-lint` + `wiki-lint` + `wiki-meta`——本地全量静态检查跑它就够，不必单独记三个。

三个坑：

- **裸跑 `python3 tools/wiki_index.py` 不是查询，是默认重建**（等价 `--write`）。想看状态用 `--check`。
- 四个开关 `--write/--check/--stale/--meta` **互斥**，一次只认一个。
- 门禁失败时按 stderr 的修复提示改**文档**，别去动索引文件。

---

## 六、查询配方

> 下面命令都实跑验证过。`jq` / `rg` 本机已装。

**配方 1 · 按主题找**

```bash
# 标题匹配（最快，命中即全路径）
jq -r '.modules[].docs[] | select(.title | test("记忆")) | .path' docs/modules/INDEX.json
# 全文搜（正文里的关键词）
rg -l "HNSW" docs/modules
```

**配方 2 · 按模块看**

```bash
cat "docs/modules/03-记忆系统/00-INDEX.md"
```

**配方 3 · 改某个文件前，该读哪篇设计**（反查 `scope`）

```bash
jq -r '.modules[].docs[] | select(.scope and (.scope|tostring|contains("src/models/agent.rs")))
       | "\(.tier)\t\(.status)\t\(.path)"' docs/modules/INDEX.json
```

输出形如 `A-设计  现行  02-Agent运行时/A-设计/runtime_design.md`。**只把 `现行` 当必读依据**；`进行中`/`草案` 说明这块正有人在改。

**配方 4 · 只看现行设计**（`A-设计` 里真正生效的）

```bash
jq -r '.modules[] as $m | $m.docs[] | select(.tier=="A-设计" and .status=="现行")
       | "\($m.id)\t\(.name)"' docs/modules/INDEX.json
```

**配方 5 · 谁还没写 `status`**（应当恒为空）

```bash
jq -r '.modules[].docs[] | select(.status=="未标注") | .path' docs/modules/INDEX.json
```

**配方 6 · 有哪些在途方案**（碰模块前必查，避免撞车）

```bash
jq -r '.modules[].docs[] | select(.tier=="B-落地方案" and (.status=="进行中" or .status=="草案"))
       | "\(.status)\t\(.title)\t\(.path)"' docs/modules/INDEX.json
```

**配方 7 · 体检失效代码引用**

```bash
make wiki-stale | rg "←" | head -30
```

---

## 七、怎么把活儿交给 Agent（引导手册）

Agent 的默认行为是「挑几篇看起来最像的读」，通常会**跳过 `A-设计`（漏掉约束）、把 `草案` 当权威（实现错方案）、改完代码不回填（知识库立刻腐烂）**。知识库守不守得住，主要取决于交代任务时有没有把约束写进去。

### 7.1 一句话 brief 模板（可直接复制）

```
需求：<一句话说清要做什么>

知识库约束：
1. 按 docs/AGENT_GUIDE.md 的五步流程走，第 2 步「读现行设计」不许跳。
2. 只有 status=现行 的 A-设计 能当权威；草案 / 落地中 / 未标注 一律先来问我。
3. 改代码前，用 docs/modules/INDEX.json 的 scope 反查受影响的文档（README §六 配方 3）。
4. 改完必须回填 C-知识，并跑 make wiki-check；索引产物一起提交。
5. 发现文档与代码不一致：以代码为准、改文档，并在汇报里单独列出来。
```

### 7.2 Agent 的必读顺序

```
AGENT_GUIDE §一（流程）
  → 本模块 A-设计 中 status=现行 的文档（为什么）
  → 本模块 C-知识 中覆盖同一块的现状长文（是什么）
  → 跨模块规范：CODE_STANDARDS.md / LAYERED_ARCHITECTURE_PRACTICE.md
  → 在途清单（AGENT_GUIDE §五），确认没人在改同一块
```

### 7.3 三条硬约束（相当于知识库的使用条款）

1. **派生索引禁手改。** `INDEX.md` / `INDEX.json` / `00-INDEX.md` 都是 `make wiki-index` 的产物，手改会在下次重建时丢失，还会让 `make wiki-lint` 报错。
2. **权威分级不能混。** `C-知识` = 现状权威；`A-设计` = 意图权威；冲突以代码为准并回头修设计；`B-落地方案` 只作进度参考。
3. **`未标注`/`草案` ≠ `现行`。** 见到就停下确认，不要"先按它做，回头再说"。

### 7.4 怎么判断 Agent 真的用了知识库（验收信号）

| 信号 | 结论 |
|---|---|
| 汇报里引用了具体文档路径 | ✅ 读过 |
| 说"文档里没写"，却没提 `INDEX.json` | ⚠️ 没查索引，让它按配方 3 重查 |
| 改了代码但 `C-知识` 没动 | ❌ 违反回填契约（AGENT_GUIDE §八） |
| `git status` 里索引产物无变化 | ⚠️ 很可能没跑 `make wiki-index` |
| 引用某个 `草案` 方案并当成定论 | ❌ 读错 status |
| 手改了 `INDEX.md` | ❌ 违反 §7.3 第 1 条 |

### 7.5 给 Agent 的两个入口，别给错

- **机器入口**：`docs/modules/INDEX.json` —— 让它用 `jq` 查，不要让它读 500 行的 `INDEX.md` 再猜。
- **流程入口**：`docs/AGENT_GUIDE.md` —— 路由表、status 语义、反模式都在里面。

---

## 八、元数据规范（`A-设计` / `B-落地方案` 必填）

这两层的文档**必须**在文件最顶端带 YAML frontmatter，否则 `make wiki-meta` 直接失败。
`C-知识` 的 RAG 卡沿用既有 `kind/name/category/scope/source_files` 格式，不在本条校验范围。

```yaml
---
kind: 设计决策            # 设计决策 | 落地方案
module: 02-Agent运行时    # 所在模块目录名，须与实际路径一致
name: runtime_design      # 文件名（不含 .md）
title: Runtime Domain 设计 # 中文标题，用于索引展示
status: 现行              # 见下表
scope:                    # 本篇涉及的代码路径（可空数组 []）
- src/models/agent.rs
related:                  # 关联文档，相对 docs/modules/ 的路径（可空数组 []）
- 02-Agent运行时/A-设计/thinking_task_policy_engine_design.md
---
```

`status` 取值与语义：

| 层 | 允许值 | 语义 | Agent 怎么办 |
|---|---|---|---|
| `A-设计` | `现行` / `落地中` / `草案` / `已废弃` / `未标注` | 设计是否仍具权威性 | 只有 `现行` 可当权威 |
| `B-落地方案` | `已完成` / `进行中` / `草案` / `已废弃` / `未标注` | 方案推进到哪 | 只作进度参考，无权威性 |

- **`未标注` 不是合法目标状态**，只是"作者尚未声明"的诚实标记：`make wiki-meta` 会告警，Agent 读到它必须以 `C-知识` 现行实现为准。
- 设计被取代时**不要删旧文**——给它标 `status: 已废弃`，新建一篇 `status: 现行` 的替代文档，并用 `related` 互链。旧文记录的是"为什么当初不那么做"，这本身是知识。
- `scope` 是**自动抽取的初值**，可人工校正；它的价值就是配方 3 的反查能力，填准了才好用。

---

## 九、维护操作手册（按场景照做）

| 场景 | 动作 | 收尾 |
|---|---|---|
| 代码实现变了 | 改对应 `C-知识` 长文（**唯一高频动作**） | `make wiki-index` |
| 新增一个原子知识点 | 新建 `C-知识` RAG 卡，带 `kind/name/category/scope/source_files` | `make wiki-index` |
| 新功能立项 | 新建 `A-设计`（带 frontmatter）+ 需要时加 `B-落地方案` | `make wiki-meta && make wiki-index` |
| 设计被取代 | 旧篇 `status: 已废弃`，新篇 `status: 现行`，两者 `related` 互链 | `make wiki-index` |
| 方案推进 / 完成 | 改 `B-落地方案` 的 `status` 与 checkbox（`进行中` → `已完成`） | `make wiki-index` |
| 文档改名 / 移动 | 直接改，索引会自动跟上 | `make wiki-index && make wiki-lint` |
| 删除文档 | 先 `rg "<文件名>" docs/modules` 确认无链接引用，再删 | `make wiki-index && make wiki-check` |
| 纯历史存档 | 移入 `archive/`（冻结、不拆模块） | — |
| 定期体检 | 查文档里引用的失效代码路径 | `make wiki-stale` |

**一次典型收尾：**

```bash
make wiki-index && make wiki-check && make docs-lint
```

> 提交时**把索引产物一起提交**（`INDEX.md` / `INDEX.json` / `00-INDEX.md`），否则下一个人跑到 `make wiki-lint` 就会红。

---

## 十、维护原则

1. 代码现状变化 → 增量修订对应模块 `C-知识/` 并 `make wiki-index`；**不**回头改 `A-设计/` 旧文（不一致时以 `C-知识` 为准）。
2. 开发规范变化 → 更新根目录 `AGENTS.md` 与 `LAYERED_ARCHITECTURE_PRACTICE.md`。
3. 架构核心概念变化 → 更新 `ARCHITECTURE.md`。
4. 新功能设计 → 先写模块 `A-设计/` 新文档（**带 frontmatter**），落地后由 `C-知识/` 承接现状描述。
5. 被取代的方案 → 原 `A-设计` 标 `status: 已废弃` 并互链替代文档；只把**纯历史存档**移入 `archive/`。
6. 架构图等可视化产物放 `diagrams/`，重新生成后覆盖同名文件；`index.html` 是 GitHub Pages 落地页、`.nojekyll` 关闭 Jekyll 渲染，二者属站点基建，不参与文档体系。
7. **三类索引与模块 `00-INDEX.md` 全自动生成，禁止手改**；新增/删除/改名文档后必跑 `make wiki-index`。
8. 提交前自检：`make wiki-check`（索引一致性 + 元数据完整性）。
9. **不复述原则。** 全局性的平台定位 / 技术栈 / 架构总述只在 `00-总览与架构` 与根目录主文档里写一次，其他文档引用即可——方向文档只对齐自己的设计，避免同一段话在 N 篇里漂移。

---

## 十一、旧模型 → 新模型

| 旧目录 | 新位置 |
|--------|--------|
| `wiki/`（现状百科） | 各模块 `C-知识/` |
| `design/`（设计决策） | 各模块 `A-设计/` |
| `plan/`（规划快照） | 各模块 `B-落地方案/` |
| `archive/`（被取代方案） | `archive/`（保持冻结，不拆分） |
| 权威纲要 / 分层实践 / 编码规范 / 记忆设计 | 根目录主文档（`ARCHITECTURE` / `LAYERED` / `CODE_STANDARDS` / `memory_design`） |
