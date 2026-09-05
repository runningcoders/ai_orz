---
kind: wiki_knowledge_card
name: A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 +
  A2aCallbackDao Push 推送
category: dao 外部调用 + domain 适配层
scope:
- src/service/dao/agent_runtime/**/*.rs
- src/service/dao/cortex/external.rs
- src/service/dao/a2a_callback/**/*.rs
- src/service/dal/agent_a2a.rs
- src/models/agent.rs
- src/service/dao/organization_link/http.rs
- src/service/dal/organization.rs（call_peer facade）
source_files:
- src/service/dao/agent_runtime/mod.rs:Ln-Lm（AgentRuntimeDao trait：单一 invoke(ctx,
  agent, prompt) -> Result<String> 抽象）
- src/service/dao/agent_runtime/a2a.rs:Ln-Lm
- src/service/dao/agent_runtime/codex.rs:Ln-Lm
- src/service/dao/cortex/external.rs:Ln-Lm（ExternalCortexDao：从 AgentPo external_config
  构造 runtime_dao；brain.think() 分发调用；统一包装 ThinkResult::Final）
- src/service/dal/agent_a2a.rs:Ln-Lm
- src/service/dao/a2a_callback/http.rs:Ln-Lm
- src/models/agent.rs:Ln-Lm
- docs/archive/design-archive/a2a_server_architecture_design.md
- ''
- docs/wiki/zh/content/功能模块/AI Agent 管理/AI Agent 管理.md
- docs/wiki/zh/content/数据模型/Agent 和技能模型/Agent 实体.md
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
- src/service/dal/organization.rs#L1-L515 (call_peer facade：先查 WS registry → publish federation.outbound，无活连接回退 HTTP)
- docs/plan/跨组织业务调用方案.md#L100-L200 (delegation e2e 时序 + receptionist 接待模型)
- 【关联卡】docs/wiki/knowledge/zh/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities.md

---

# A2A Client + 外部 Agent Runtime（出站调用 + 内部桥接）

## §1 整体方案
外部 Agent（A2A 远程 HTTP 调用型 + CLI 子进程型）作为**可被内部统一 think() 调用的一等公民**接入，不侵入现有 Brain/Cortex 主链路。四层清晰解耦，严格单向调用：

(a) **AgentRuntimeDao trait（外部执行统一抽象）**：`src/service/dao/agent_runtime/mod.rs` 定义通用接口 `async fn invoke(&self, ctx, agent: &AgentPo, prompt: &str) -> Result<String>`，两种后端实现：
   - **A2aRuntimeDao（HTTP JSON-RPC）**：构建 POST JSON-RPC 2.0 `tasks/send` 请求 → 等待远程 Agent 返回结果文本（短任务同步等待；长任务走 callback + 30s 轮询兜底，详见内部策略）；REQUEST_ID_COUNTER 原子单调递增生成 JSON-RPC id；extract_text_from_task_result 从 A2aTask 结果消息提取首个 Text part。
   - **CodexRuntimeDao（CLI 子进程）**：tokio::process::Command 启动子进程 → stdin 写 prompt → stdout 读执行结果文本 → 超时 timeout_secs 自动 kill。支持 work_dir、env 隔离（每个 Agent 独立 HOME / PATH）。支持 prompt_template：例如 `"<question>\n{prompt}\n</question>"`，{prompt} 占位被真实 prompt 替换后再写入 stdin。

(b) **ExternalCortexDao 桥接（统一 think() 接入）**：`src/service/dao/cortex/external.rs` 把 AgentRuntimeDao 包装成 `CortexDao` trait（与内部 native CortexDao 同接口）。关键适配：从 ChatMessage[] 历史中取最后一条 user 消息作为 prompt（外部 Agent 不支持多轮 tool call，只接受单轮 prompt），invoke 完成后把文本包装成 `ThinkResult::Final { content, usage: default() }`。**由 brain.think() 按 brain.kind=External 分发到此实现**，外部 Agent 与内部 Agent 在 Runtime 层调用方式完全一致。

(c) **A2aAgentDal（委托模式 + 不引入全局 builder）**：`src/service/dal/agent_a2a.rs` 专门管理 Remote 类型 Agent 的 CRUD。通过委托 `Arc<dyn AgentDal>` 复用全部默认管理操作（create/find_by_id/query/search 等），不重写任何默认方法。**不主动新建独立 PromptBuilder**（未重写 prompt_builder() 时走 AgentDal trait 默认方法返回 DefaultPromptBuilder），未来扩展 RemotePromptBuilder 时仅需在此文件重写对应方法。

(d) **A2aCallbackDao（Push 推送出站）**：作为 A2A Server 侧（对外接收入站任务、对外 Push 任务变更）的唯一出站 DAO，实现：Push 时根据 channel.scope_project 或 message.po.project_id 定位项目 → 拉取项目全部历史消息 → 按 message.from_role 映射 A2aMessage.role（user/agent/system）→ 组装完整 A2aTask JSON（state + messages[]）→ POST 到 webhook_url。test_connection 发送一条 state=ping 的轻量 payload 验证端点可达。与其他渠道 DAO 同模式（Lark/Webhook/Email DAO），**不跨层依赖**。

(e) **ExternalAgentConfig PO 模型（单一配置事实源）**：`src/models/agent.rs` 中 ExternalAgentConfig 枚举，变体与 AgentRuntimeDao 实现 1:1 对应：
   - `Cli { command, args, work_dir, env, timeout_secs, prompt_template }` → 由 ExternalCortexDao 映射到 CodexRuntimeDao
   - `Remote { endpoint, agent_name, auth_token, timeout_secs }` → 由 ExternalCortexDao 映射到 A2aRuntimeDao

新增外部 Agent 执行后端时「1 新增 ExternalAgentConfig 变体 → 2 新增 XXXRuntimeDao 实现 → 3 ExternalCortexDao.from_agent 加 match arm」3 步闭环，Domain/Handler 零改动。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [dao/agent_runtime/mod.rs](src/service/dao/agent_runtime/mod.rs) | 统一执行抽象 Trait | `trait AgentRuntimeDao: Send + Sync + DynClone`；单一入口 `invoke(ctx, agent, prompt) -> Result<String>` |
| [dao/agent_runtime/a2a.rs](src/service/dao/agent_runtime/a2a.rs) | HTTP A2A 远程实现 | A2aRuntimeConfig(endpoint/agent_name/auth_token/timeout)；execute_a2a_send；call_a2a_jsonrpc 通用 JSON-RPC HTTP；REQUEST_ID_COUNTER 单调 |
| [dao/agent_runtime/codex.rs](src/service/dao/agent_runtime/codex.rs) | CLI 子进程实现 | tokio::process::Command stdin/stdout 异步；work_dir/env/timeout 配置；prompt_template {prompt} 占位替换 |
| [dao/cortex/external.rs](src/service/dao/cortex/external.rs) | ExternalCortexDao 桥接 | from_agent(agent: &AgentPo) -> Option<Self>（按 ExternalAgentConfig 构造 runtime_dao）；think() 提取 last user prompt → invoke → ThinkResult::Final |
| [dal/agent_a2a.rs](src/service/dal/agent_a2a.rs) | A2aAgentDal 委托 | struct A2aAgentDal { base: Arc<dyn AgentDal> }；impl AgentDal 全方法委托 base；未重写 prompt_builder（Default）|
| [dao/a2a_callback/http.rs](src/service/dao/a2a_callback/http.rs) | A2aCallbackDao HTTP Push | push(ctx, message, channel, options)：查项目消息 → 映射 A2aMessages → POST webhook_url；OnceLock 单例 + factory methods |
| [models/agent.rs](src/models/agent.rs) | ExternalAgentConfig 枚举 | Cli / Remote 变体；与 runtime dao 1:1 对应 |
| 【① Design】a2a_server_architecture_design.md §二 ExternalCortexDao 桥接 | 为什么要桥接成 CortexDao（统一 think 链路、不侵入内部 brain）| docs/archive/design-archive/a2a_server_architecture_design.md |
| 【③ Wiki 长文 1】AI Agent 管理.md §外部 Agent | 外部 Agent 配置字段含义 + 创建流程 | docs/wiki/zh/content/功能模块/AI Agent 管理/AI Agent 管理.md |
| 【③ Wiki 长文 2】Agent 实体.md §ExternalAgentConfig | PO 模型字段说明 | docs/wiki/zh/content/数据模型/Agent 和技能模型/Agent 实体.md |
| 【平行卡 1】协议层 | DTO 类型定义 | docs/wiki/knowledge/zh/A2A%20协议层：AgentCard%20发现%20+%20JSON-RPC%202.0%20+%20A2aTask%20任务状态机%20+%20A2aMessage%20双向消息/A2A%20协议层：AgentCard%20发现%20+%20JSON-RPC%202.0%20+%20A2aTask%20任务状态机%20+%20A2aMessage%20双向消息.md |
| 【平行卡 2】Server Handler 层 | 入站 Handler | docs/wiki/knowledge/zh/A2A%20Server%20Handler%20层：JSON-RPC%20方法路由%20+%20公开无鉴权路由%20+%20notification_url%20回调渠道自动创建/A2A%20Server%20Handler%20层：JSON-RPC%20方法路由%20+%20公开无鉴权路由%20+%20notification_url%20回调渠道自动创建.md |

## §3 架构约定

1. **AgentRuntimeDao 永远接收 `&AgentPo`（不是单独 agent_id/config）**：trait 设计上直接给完整 AgentPo，实现可按需读取 agent.id/agent.name/po.external_config 任何字段，无需未来为了传更多参数频繁改 trait 签名。与「策略引擎 DTO 传完整实体」模式一致。
2. **外部 Agent 调用超时必须有配置化上限**：Remote/Cli 两种 runtime config 均显式含 timeout_secs 字段，A2aRuntimeDao 构建 reqwest Client 时 `.timeout(Duration::from_secs(timeout_secs))`，CodexRuntimeDao 用 tokio timeout 包装 stdout 读取。**禁止无限等待**（默认 300s，配置最小 5s / 最大 3600s）。
3. **ExternalCortexDao 的「单轮 prompt 提取策略」不可静默丢失多轮上下文**：think(messages) 时从 messages 中 rev 找到最后一条 role=user 的 content 作为 invoke 的 prompt。如果 messages 为空或无 user 消息 → 直接返回空字符串（不 panic）。调用方上层应确保至少传入一条有效 user 消息。
4. **CLI 子进程 HOME / env 严格隔离**：不同 Agent 的 CodexRuntime 必须各自独立 HOME、独立 token 配置（如 gh auth status 互不影响）。prompt_template 提供 {prompt} 占位替换，实现层用 `format!` 简单替换，不引入模板引擎依赖。
5. **A2aCallback Push 失败时不重试无限循环**：HTTP 推 webhook_url 失败（非 2xx/超时）→ 只写 log_warn! 告警 + 计入渠道健康度统计；**不做同步重试（阻塞消息投递链路）**，未来由独立统计/重跑消费者异步兜底。消息本身永不丢失（已经存 DB），只要最终健康检查修复渠道配置即可恢复推送。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 Domain 层直接 new A2aRuntimeDao / CodexRuntimeDao**：所有 runtime_dao 的构造必须通过 `ExternalCortexDao.from_agent(&agent_po)` 统一入口。直接 new 会导致鉴权、超时、日志埋点等切面逻辑丢失（from_agent 内部统一处理）。
2. ❌ **禁止把明文 auth_token 写入日志/AOP 事件/HTTP 响应**：A2aRuntimeConfig.auth_token 任何 log_info!/debug 打印时必须脱敏（"****" + 首尾各 4 字）。A2aCallbackDao 推送的 A2aTask JSON 中同样**永不包含我方外部 Agent 的 auth_token**（即使对方是可信合作伙伴）。
3. ❌ **禁止新增外部 Agent 类型时硬编码 if kind == Remote { a2a } 这类散落在业务代码各处的判断**：所有类型差异点必须通过 ExternalAgentConfig 变体 match 分发（集中在 ExternalCortexDao.from_agent 和 future PromptBuilder 重写 2 处文件内）；不得在 Handler/DAL/Domain 各层到处新增独立分支。
4. ✅ **新增执行后端 3 步强绑定**：(1) ExternalAgentConfig 枚举加变体 → (2) 对应 XXXRuntimeDao 文件（实现 AgentRuntimeDao trait）→ (3) ExternalCortexDao.from_agent 加 match arm。3 步缺一不可；**3 步全完成前禁止创建 HR Agent 创建页表单字段**（防止用户在前端创建出后端未实现的类型直接报错）。
5. ✅ **A2aCallbackDao HTTP POST 必须设置超时（默认 10s）**：防止外部 webhook_url 响应极慢阻塞我方 AOP 消息投递消费者；reqwest Client builder 必须显式 `.timeout(Duration::from_secs(10))`，不使用默认无超时。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 3 篇 wiki 长文（Agent 管理 / Agent 实体 / Runtime 编排）+ 1 Design（a2a_server_architecture）+ Plan 占位 + 2 平行卡（协议层/Server Handler 层）；对应 Wiki 长文 cite 段回链本卡 + Design + 平行卡。
