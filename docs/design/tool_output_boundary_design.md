# 工具输出三分边界设计（运行时输出 / 最终结果 / 产物）

> 🎯 **定位**：① Design 决策快照——定义 shell_exec 等工具执行输出的三种处理方式（运行时输出 / 最终结果 / 产物）的边界、判定法则与升级协议，作为所有新增工具输出处理的统一依据。
> 状态：定稿（2026-08-19）
> 触发场景：新增任何会产生输出的工具（尤其长输出/文件型输出）时，设计其输出处理策略前必读；做日志清理、产物归档相关改动时必读。
>
> 关联文档：
> - [runtime_design.md](./runtime_design.md) — P1「ToolCallResult 产物化引用策略」条目由本文展开落地
> - [web_search_and_browser_tools_design.md](./web_search_and_browser_tools_design.md) — 截图产物先例（红线 11）纳入本文统一模型
> - [AGENTS.md](../../AGENTS.md) — 分层架构与两阶段初始化规范

---

## 一、设计目标

### 1.1 核心问题

工具（尤其 shell_exec）执行产生的输出，本质上是一份**原始输出流**，但系统中存在三类截然不同的消费者，各自有不同的容量约束与生命周期。历史上容易把它们混为一谈（全量塞进结果、或结果与日志脱节），导致：LLM 上下文爆炸、观测数据丢失、有价值输出无处沉淀。

**设计哲学**：同一份原始输出流的**三个投影**——不冲突、不同时、不同主。分界的根本锚点是 ② 的 token 预算约束：正因 ② 必须截断，才需要 ① 全量留盘兜底；而 ③ 的判定权不在工具，必须声明式升级。

### 1.2 关键决策表

| # | 问题 | 方案 | 原因 |
|---|------|------|------|
| 1 | 三层各自的本质定位？ | ① 事实（发生了什么）/ ② 决策依据（LLM 需要知道什么）/ ③ 资产（值得保留什么） | 消费者、容量约束、生命周期三者完全不同，任何单一通道都无法同时满足 |
| 2 | ② 超预算输出怎么处理？ | 摘要截断 + `log_path` 引用回传，禁止静默截断 | LLM 拿到引用可自助取全量（tail / shell_status），框架不代劳；引用的底气是 ① 已全量留盘 |
| 3 | ③ 产物升级的判定权归谁？ | 三层结构：天然产物型工具自带语义 / Agent 声明式提议（mark_artifact）/ 用户事后治理 | 单一判定方都有缺陷：Agent 独断产垃圾、用户逐条审批在异步任务下不可行 |
| 4 | ③ 产物升级的成本模型？ | **复制晋升**（一次拷贝物化到产物目录，与 ① 解耦） | ① 有 TTL 可被清理，若 ③ 仅引用则清理会造成死链；归档 = 数据从可清理观测层物化为持久资产层，复制成本低（KB~MB 级）且删 artifact 即删副本，仍可逆 |
| 5 | mark_artifact 默认 ControlMode？ | Auto（Agent 自行归档 + 用户事后治理） | 归档是低频小文件复制、删 artifact 即删副本，试错成本低；符合 Agent 自治哲学；个别高价值场景可配 Manual 兜底 |
| 6 | ① 的日志如何组织与清理？ | 按天目录 `{YYYYMMDD}/{call_id}.log` + retention 配置化 + cron 系统任务清理 | 对齐系统日志 retention_days 先例与 daily_jsonl 的 YYYYMMDD 日期分区；按天目录使清理退化为删日期目录；cron 比启动时清理更及时 |
| 7 | 清理时如何保护活跃进程？ | Running 状态进程的 call_id 对应日志跳过不删 | 长驻后台进程日志被删会导致 shell_status 观测断流 |
| 8 | 前端观测入口？ | 系统健康监控页新增「工具日志存储」卡片：占用统计 + 按天分布 + 手动清理（可临时覆盖保留天数） | 观测数据（①）本就该有 TTL 与可视化治理入口，与 ③ 的项目管理分属不同域；保留天数持久配置走 ai_orz.toml `[tool_log].retention_days` |

### 1.3 三层对照速查

| | ① 运行时输出 | ② 最终结果 | ③ 产物 |
|---|---|---|---|
| 本质 | 事实 | 决策依据 | 资产 |
| 消费者 | 观测者（前端/调试/LLM 自助） | LLM（下一轮决策） | 用户 / 项目 |
| 形态 | append-only 日志流 | JSON + 摘要 + 引用 | Artifact 引用登记 |
| 容量 | 无限（全量留盘） | token 预算（超限截断） | 无限（持久存储） |
| 生命周期 | retention 天数（可清理） | 随对话轮次 / Trace | 随项目 / 任务 |
| 判定法则 | 过程有观测价值？ | LLM 决策需要内容本身？ | 值得保留为交付物？ |
| 关联键 | call_id（文件名） | call_id + trace_ref | call_id + artifact_id |

## 二、架构思路

### 2.1 数据流全景

```
工具执行（如 shell_exec spawn）
    │
    │  stdout/stderr 从 spawn 起重定向
    ▼
┌─────────────────────────────────────────────────────┐
│ ① 运行时输出（事实层）                                │
│  tools/shell_exec/logs/{YYYYMMDD}/{call_id}.log      │
│  append-only · 全量 · 按天分目录 · retention 可配     │
│  清理：cron 系统任务（保护 Running 进程日志）           │
└──────────────┬──────────────────────────┬───────────┘
               │ 结束后读摘要（受预算截断）    │ mark_artifact 声明式升级
               ▼                          │ （复制晋升入产物目录）
┌──────────────────────────────┐          │
│ ② 最终结果（决策层）           │          │
│ ToolCallEntry.output          │          │
│ 摘要 + truncated + log_path   │          │
│ + call_id + trace_ref         │          │
└──────────────────────────────┘          ▼
                              ┌───────────────────────┐
                              │ ③ 产物（资产层）        │
                              │ GeneratedContent 产物  │
                              │ source=agent 可治理    │
                              └───────────────────────┘

单一关联键：call_id / trace_ref 贯穿三层
LLM 自助闭环：② 里的 log_path → Agent 可再调 shell 工具取全量
```

### 2.2 background 模式下的时间轴分离

三层天然不同时：background 立即回 ② 第一帧（pid + log_path）→ 运行中 ① 持续产生 → shell_status 轮询取增量 → 结束后 ③ 可归档。sync 模式则是同一时点的三个投影。现有 detach/kill 设计已验证此模型。

## 三、涉及文件清单

已有文件（零改动或已实现）：

| 文件 | 角色 | 状态 |
|------|------|------|
| [shell_exec.rs](../../src/pkg/tool_registry/shell_exec.rs#L334-L379) | ① 统一日志流式模型（sync/background 均重定向到日志文件，按天目录 `{YYYYMMDD}/{call_id}.log`） | ✅ 已实现 |
| [shell_exec.rs](../../src/pkg/tool_registry/shell_exec.rs#L439-L472) | ② 摘要截断 + log_path/truncated/full_output_bytes 回传 | ✅ 已实现 |
| [tool_call/impl.rs](../../src/service/dao/tool_call/impl.rs#L70-L202) | ② ToolCallEntry 审计 + trace_ref 强类型携带 + 幂等防重 | ✅ 已实现 |
| [process/mod.rs](../../src/pkg/process/mod.rs) | ① 进程注册中心（Running 状态判定依据） | ✅ 已实现 |
| [project/mod.rs](../../src/service/domain/project/mod.rs#L365-L379) | ③ ArtifactManage.create_attachment_artifact 引用登记 | ✅ 已实现 |
| [logging.rs](../../src/pkg/logging.rs#L46-L53) | retention 先例（rolling::daily + cleanup_old_logs） | ✅ 模式参考 |
| [config.rs](../../common/src/config.rs#L216-L229) | LoggingConfig.retention_days 配置先例 | ✅ 模式参考 |
| [system/mod.rs](../../src/service/domain/system/mod.rs#L419-L474) | cron 系统默认任务幂等注入点（ensure_system_cron_triggers） | ✅ 扩展点 |
| [scheduler.rs](../../src/consumer/scheduler.rs#L78-L82) | cron payload.action 分发点 | ✅ 扩展点 |
| [builtin.rs](../../src/pkg/tool_registry/builtin.rs#L31-L43) | 通用内置工具注册表 | ✅ 扩展点 |

新增文件（已落地）：

| 文件 | 角色 |
|------|------|
| [mark_artifact.rs](../../src/pkg/tool_registry/mark_artifact.rs) | ③ 声明式归档内置工具（Auto 模式，复制晋升；ArtifactRegistrar trait 注入） |
| [tool_log_retention.rs](../../src/pkg/tool_log_retention.rs) | ① 按天目录清理与占用统计（retention 读取 + Running 保护） |
| [tool_log_stats.rs](../../src/handlers/system/storage/tool_log_stats.rs) / [tool_log_cleanup.rs](../../src/handlers/system/storage/tool_log_cleanup.rs) | 前端存储监控 API（占用统计 / 手动清理，共用清理函数） |
| [artifact.rs](../../src/service/domain/project/artifact.rs) ProjectToolOutputRegistrar | ArtifactRegistrar 的 Domain 实现（复用 create_generated_artifact_from_file） |

配套改动（扩展点落地）：

| 文件 | 变更 |
|------|------|
| [config.rs](../../common/src/config.rs) ToolLogConfig | `[tool_log].retention_days` 配置（默认 30，0 = 不清理） |
| [ai_orz.toml](../../common/config/ai_orz.toml) | `[tool_log]` 配置段 |
| [scheduler.rs](../../src/consumer/scheduler.rs) | cron action `tool_log_cleanup` 分发 |
| [system/mod.rs](../../src/service/domain/system/mod.rs) | 第 3 条系统默认 cron 任务（每日 05:00）幂等注入 |
| [service/mod.rs](../../src/service/mod.rs) | service::init 注册 ProjectToolOutputRegistrar |
| [builtin.rs](../../src/pkg/tool_registry/builtin.rs) | mark_artifact 工厂注册（tag: artifact） |
| [system.rs](../../common/src/api/system.rs) | ToolLogStorage/Cleanup DTO |
| [router.rs](../../src/router.rs) | GET/POST `/api/v1/system/storage/tool-logs` 路由 |
| [health.rs](../../frontend/src/pages/system/health.rs) | 前端「工具日志存储」卡片（统计 + 按天明细 + 手动清理） |
| [system.rs](../../frontend/src/api/system.rs) | 前端 API client（get_tool_log_storage / cleanup_tool_logs） |

## 四、关键边界 / 行为红线

1. ② 超预算截断**必须**携带 `truncated: true` + `log_path` + `full_output_bytes` 回传，禁止静默丢弃。
2. ③ 产物升级是**复制晋升**：归档时把 ① 的日志文件一次拷贝入项目产物目录（复用 `create_generated_artifact_from_file`），① 与 ③ 生命周期解耦，TTL 清理不触碰产物副本。
3. `mark_artifact` 默认 ControlMode = Auto；归档产物必须带 `tool-output` tag + GeneratedContent 来源标记，用户可在产物列表识别与清理。
4. 天然产物型工具（browser 截图等）输出**直接落产物存储返回引用**，禁止 base64 内嵌返回（沿用 web_search_and_browser_tools_design 红线 11）。
5. 日志清理**必须**跳过进程注册中心中 Running 状态进程的 call_id 对应日志文件。
6. retention 配置 0 = 不清理；默认值对齐系统日志先例（30 天）。
7. 清理任务作为第 3 条系统默认 cron 任务注入（payload action = `tool_log_cleanup`），沿用 payload 字符串包含去重的幂等模式；用户可禁用/调整，系统不强制。
8. ① 的日志目录按天划分为 `tools/shell_exec/logs/{YYYYMMDD}/{call_id}.log`；清理单位是日期目录，非逐文件 mtime。
9. 手动清理（前端按钮）与自动清理（cron）走**同一清理函数**，共享 Running 保护逻辑，禁止两套实现。
10. 观测数据（①）与资产（③）分属不同生命周期域：① 可被清理策略删除，③ 只能被用户显式删除；归档为复制晋升（独立副本），清理逻辑天然不触碰产物文件。

## 五、扩展模式

### 场景 A：新增会产生输出的工具（最常见）

1. 三问过一遍：过程有观测价值？→ 框架级 ①（有 stdout/stderr 即默认 yes）；LLM 决策需要内容？→ 工具自定义摘要策略（②）；输出天然是交付物？→ 工具内直接调 ArtifactManage 落 ③。
2. 摘要策略在工具内实现（工具最懂什么重要），框架只保证超限截断 + 引用回传的协议不变。
3. 参考 shell_exec 的结果协议字段：`success / call_id / truncated / full_output_bytes / log_path / output`。

### 场景 B：新增天然产物型工具（如导出报告、生成文件）

1. 工具执行时直接写文件 + 调 `ArtifactManage.create_attachment_artifact` 登记引用。
2. ② 结果只回 artifact 引用（id + 路径），不回内容本体。
3. 参考 browser 截图实现。

### 场景 C：Agent 归档已有工具输出（运行后回捞）

1. Agent 从 ② 结果拿到 `call_id`，调用 `mark_artifact(call_id, project_id, task_id?, name, description)`。
2. mark_artifact 按 call_id 在 `tools/*/logs/*/{call_id}.log` 定位日志文件 → 复制晋升入项目产物目录（GeneratedContent 产物，带 tool-output 标记）→ 返回 artifact 引用。
3. 用户在前端产物列表治理（查看/删除）；① 原日志仍受 TTL 管理，与产物副本互不影响。

### 场景 D：调整日志保留策略

1. 持久配置：改 `ai_orz.toml` `[tool_log].retention_days`（0 = 不清理），重启生效。
2. 临时覆盖：前端存储卡片手动清理时输入保留天数（单次生效，不改持久配置）。
3. 下一次 cron 清理周期（每日 05:00）生效；手动清理按钮可立即触发同一清理函数。
