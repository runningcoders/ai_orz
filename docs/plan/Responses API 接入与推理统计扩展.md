# Responses API 接入与推理统计扩展 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Cortex 层支持 OpenAI Responses API（`/v1/responses`）协议，同时补齐两项能力：① `ChatMessage::Reasoning` 变体承载并回传 reasoning item；② 把缓存命中 token 与思考 token 接入统计表。

**Architecture:** 四个正交改动面——① `ModelProviderConfig` 增 `api_flavor` 开关（协议维度与 `ProviderType` 解耦），`CortexDaoRegistry::get()` 改为按 provider 路由；② 新增 `ResponsesCortexDao` 实现（请求体 + SSE 语义事件解析独立成文，不动 `OpenAiCompatibleCortexDao`）；③ `ChatMessage` 增 `Reasoning` 变体，作用域限定单次 `run_think_loop`；④ `TokenUsage` 增 `cached_tokens` / `reasoning_tokens`，沿 ThinkRoundEvent → ModelCallEvent → DuckDB 落库。上层（think_loop / brain / tool_call / MCP）零改动。

**Tech Stack:** Rust + reqwest SSE + serde（wire DTO 文件私有）+ DuckDB（`src/pkg/stats/`）+ sqlx SQLite（不涉及）

---

## 背景（为什么做）

- 接入的新模型厂商只提供 Responses 形态接口，不支持 `/v1/chat/completions`。
- 现有 7 个 `ProviderType` 共用同一个 `OpenAiCompatibleCortexDao`，差异只有 `default_base_url` 与 DoubaoVision 的 embed 端点——**"多厂商"≠"多协议"**，本项目此前无任何第二协议适配（Anthropic/Gemini 零命中；Jev 走 cerebellum，不在 chat 链路）。
- 推理模型的 token 构成中，缓存命中与思考开销占比显著，当前统计表只有 `tokens_input` / `tokens_output` / `total_tokens`，无法观测这两项。

## 已对齐的设计决策（不可推翻）

1. **协议维度必须与 `ProviderType` 正交**，不加 `ProviderType::OpenAiResponses`。`ProviderType` 语义是"厂商/端点"（实际只决定 `default_base_url` 与 DoubaoVision embed 分支），塞进协议会导致 `Custom` 类型枚举爆炸，且 OpenAI 自家两种协议并存。**既有原则佐证**：`ModelProviderConfig.supports_vision` 的注释原文即写着"**与 provider_type 相互独立**"。
2. **`api_flavor` 照抄 `access_mode` 模式**：`Option<ChatApiFlavor>`，缺省 = `chat_completions`，存量 config JSON 缺字段自动兼容，**无 migration**。
3. **`CortexDaoRegistry::get(provider_type)` → `get(provider: &ModelProviderPo)`**（需读 config）。6 个调用点均已在作用域内持有 provider，改签名无痛。
4. **新增 `native/responses.rs`，不改造 `OpenAiCompatibleCortexDao`**。两者差异过大（请求体结构、SSE 事件模型、终态判定全不同），混在一个文件会让分支爆炸。
5. **`ChatMessage::Reasoning` 作用域限定单次 `run_think_loop`**（纯内存，不持久化）。调研确认：`messages` 是 `think_loop.rs:293` 的局部变量，全仓库**零处**序列化 messages 入库/入 JSONL；跨轮次历史来自短期记忆摘要与业务消息表的 `to_prompt()` 字符串。⇒ 单轮内工具往返回传纯内存即可满足，**不新建存储载体、不加 migration**。
6. **`to_summary_text` 的 Reasoning 臂绝不输出 `encrypted_content`**，只输出 id + 摘要计数。照抄 `UserMultimodal` 的 base64 排除先例（`cortex_types.rs:269-276`，配套守卫测试 `:314-331`）。理由：该字段经 `RoundDigest.transcript`（`think_loop.rs:622` 的 `round_start_idx` 切片会自动含新 push 的消息）→ `abort_summary` 可能到达用户可见文本与短期记忆。
7. **两条协议都解析 token 明细字段**。Responses 是 `input_tokens_details.cached_tokens` / `output_tokens_details.reasoning_tokens`，Chat Completions 对应 `prompt_tokens_details.cached_tokens` / `completion_tokens_details.reasoning_tokens`——形状一致、成本近似为零，存量 provider 立刻受益。
8. **DuckDB 加列走 `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`**，参照现有 `caller_organization_id` 先例（`model_call.rs:153` 等 5 处同模式）。**`migrations/` 零新增**——那 21 个 .sql 是 SQLite 专用，DuckDB 不走 sqlx migration，靠 `create_table()` 每次启动幂等执行。
9. **第一版关闭 `strict`**。Responses 的 function tool 扁平结构带 `strict` 字段，MCP 外部 schema 是不可信输入，开 `strict` 会被挑刺 400。
10. **显式下发 `"store": false`**。默认服务端会留存请求 payload，与项目脱敏体系的主路径/旁路分层原则冲突。

## 范围

- **本 plan**：协议接入 + Reasoning 回传 + 统计字段写入链路。
- **不在本 plan**：统计字段的前端展示（读侧 7+ 文件，含 `common/src/models/stats.rs` 的 `TimeSeriesPoint` / `TokenSumResult` 与 `charts/line_chart.rs` 的 `LineChartValueField` 枚举变体）。写入链路完成后 `StatAggregation::Sum("cached_tokens")` 已开箱可用（`build_aggregation_query` 是通用实现），展示属于独立改造。

---

## 一、协议接入

### 1.1 入站请求体映射

| 现状（chat） | Responses | 备注 |
|---|---|---|
| `messages: [{role, content}]` | `input: [item...]` | content 由字符串变数组 |
| `role:"user"` | `{type:"message", role:"user", content:[{type:"input_text", text}]}` | |
| `role:"assistant"` + `tool_calls` | 拆为 `{type:"message", role:"assistant", content:[{type:"output_text",text}]}` + N 个 `{type:"function_call", call_id, name, arguments}` | **一条变多条** |
| `role:"tool"` | `{type:"function_call_output", call_id, output}` | `tool_call_id` → `call_id` |
| `role:"system"` | 保留在 input 内（role 允许 `system`/`developer`） | **不用顶层 `instructions`**——系统消息可能多条（人设/技能/规则），合并会丢结构 |
| `tools:[{type:"function", function:{...}}]` | `[{type:"function", name, description, parameters, strict}]` | **扁平，去 `function` 嵌套**；`strict` 恒 false |
| `stream_options.include_usage` | 无 | usage 只在 `response.completed` |
| `max_tokens` | `max_output_tokens` | 当前未下发，无影响 |

新增顶层字段：`store: false`。

### 1.2 出站 SSE 事件（只列需处理的）

```
response.output_item.added                 建槽位（message / function_call / reasoning）
response.output_text.delta                 content 累加
response.function_call_arguments.delta     arguments 累加
response.function_call_arguments.done      arguments 定稿
response.reasoning_summary_text.delta      摘要累加（可选）
response.output_item.done                  item 定稿
response.completed                         终态 + usage
response.incomplete / response.failed / error   失败
```

末尾仍发 `data: [DONE]`，现有 `parse_sse_line`（`http.rs:782`）剥 `data:` 前缀部分可复用。事件名取自 `openai-python` 的 OpenAPI 生成类型，非凭记忆。

### 1.3 终态判定映射（**finish_reason 消失**）

`finish_think_result`（`http.rs:321`）现依赖 `finish_reason` 决定产出 `Final` 还是 `ToolCall`，Responses 无此字段：

| chat | Responses |
|---|---|
| `finish_reason == "stop"` | `status == "completed"` 且 output 无 function_call |
| `finish_reason == "tool_calls"` | output 数组中**存在** `type == "function_call"` 的 item |
| `finish_reason == "length"` | `status == "incomplete"` 且 `incomplete_details.reason == "max_output_tokens"` |
| `finish_reason == "content_filter"` | `incomplete_details.reason == "content_filter"` |

第二行是核心风险点：现在读标志位，以后要**扫描数组**。漏判即静默退化为 Final（不报错、工具不执行）。

`check_stream_end`（`http.rs:497`，`finish_reason == "length"` → 显式失败）同步改表，否则超长输出静默截断。

### 1.4 usage 字段映射

| chat | Responses | 内部字段 |
|---|---|---|
| `prompt_tokens` | `input_tokens` | `input_tokens` |
| `completion_tokens` | `output_tokens` | `output_tokens` |
| `total_tokens` | `total_tokens` | `total_tokens` |
| `prompt_tokens_details.cached_tokens` | `input_tokens_details.cached_tokens` | **新增** `cached_tokens` |
| `completion_tokens_details.reasoning_tokens` | `output_tokens_details.reasoning_tokens` | **新增** `reasoning_tokens` |

---

## 二、`ChatMessage::Reasoning`

### 2.1 变体定义

```rust
/// 推理项（Responses API 的 reasoning output item）
///
/// 某些推理模型要求下一轮 input 原样回传上一轮的 reasoning item，否则 400。
/// 作用域限定单次 `run_think_loop`：messages 是该函数的局部变量，不做任何
/// 序列化持久化（全仓库零处落库/落 JSONL），故纯内存承载即可。
Reasoning {
    /// item id（形如 rs_xxx），回传时原样带上
    id: Option<String>,
    /// 摘要文本（可展示；不进加密串）
    summary: Vec<String>,
    /// provider 私有加密串，仅可在**同一 provider** 回传；
    /// 绝不进 to_summary_text / trace / 日志
    encrypted_content: Option<String>,
},
```

### 2.2 编译期穷举点（必改，3 处）

| 文件:行 | 函数 |
|---|---|
| `src/service/dao/cortex/native/http.rs:67` | `messages_to_json`（chat 协议出口）→ Reasoning 臂**跳过**（chat 协议无此概念，输出必 400）+ debug 日志 |
| `src/models/cortex_types.rs:231` | `to_summary_text` → 输出 `[reasoning] id=xxx (N summary, encrypted omitted)` |
| `src/service/domain/runtime/awakening_tests.rs:103` | `CapturingBrainDal::think` mock（该处注释已明示"新变体穷举波及臂，编译器强制"，是项目既有的加变体检查点） |

带通配符的 match（6 处：`native/mod.rs:47`、`think_loop.rs:303`、`brain.rs:164`、`external.rs:104`、`flat.rs:133`/`:145`）自动跳过 Reasoning，语义正确，**无需改动**。

### 2.3 产出与回传

`ThinkResult` 两个变体各增 `reasoning_items: Vec<ReasoningItem>` 字段 + `reasoning_items()` accessor——与现有 `usage` 字段同构（两变体各一份 + accessor）。

`think_loop` 在 push Assistant 之后追加 Reasoning，顺序为 **Reasoning → Assistant(含 tool_calls) → Tool 结果**，与 Responses 的 output 数组顺序一致。

### 2.4 不确定性（需实测）

`store: false` 时厂商是否仍返回可回传的 `encrypted_content` 不确定。第一版按"有则回传、无则只带 id/summary"实现；若厂商实际不要求回传，该链路为空转（预留给 OpenAI 官方 reasoning 模型），不算缺陷。

---

## 三、统计字段扩展

### 3.1 写入链路（必改 6 文件）

```
LLM usage (wire)
 → src/service/dao/cortex/native/http.rs:835  Usage struct 增子结构
 → http.rs:321  finish_think_result 映射
 → src/models/cortex_types.rs:65  TokenUsage 增 2 字段
 → think_loop.rs:466 / :641  with_model_usage 传参
 → src/models/events/think_round.rs:25  事件字段 + new() + with_model_usage()
 → src/consumer/think_round_stats_consumer.rs:55  映射
 → src/pkg/stats/model_call.rs  ModelCallEvent（7 个子点）
 → DuckDB model_call_events
```

`TokenUsage` 增 `cached_tokens: u64` / `reasoning_tokens: u64`（派生 `Default`，现有 12 处 `TokenUsage::default()` 零波及）。

`ModelCallEvent` 7 个子点：字段定义(`:34`) / `new()`(`:45`) / builder(`:98`) / DDL(`:128`) / **ALTER(`:151`)** / `insert_event`(`:167`) / `bulk_insert_events`(`:202`)。列类型用 `BIGINT`（对齐现有 `tokens_input BIGINT`）。

> 注意：事件字段名是 `tokens_input` / `tokens_output`，不是 prompt/completion。

### 3.2 脱敏

`common/src/redaction/rule.rs` 的 `KeyRule{name:"token"}` 的 exclude 名单**已同时包含 `"cached"`(:86) 与 `"reasoning"`(:87)**，且该规则采用 `ValueClass::StringOnly`（数值原样保留）。⇒ 新字段不会被误伤。

**需补 1 行**：`rule.rs:264-280` 的护栏测试 `llm_usage_fields_are_not_redacted` 白名单已有 `cached_tokens`(:276)，**缺 `reasoning_tokens`**——纯回归价值，不加不会出错，但对齐既有清单。

### 3.3 现成护栏

- 无测试断言 DuckDB 列数或 TokenUsage 字段数 ⇒ 加字段不会打破存量测试。
- `stats_duckdb_test.rs:62-63` 的求和断言建议扩写覆盖新字段。

---

## 文件结构（全部改动点）

| 文件 | 改动 |
|---|---|
| `common/src/enums/provider.rs` | 新增 `ChatApiFlavor` 枚举 |
| `src/models/model_provider.rs` | `ModelProviderConfig` 增 `api_flavor` + `api_flavor_or_default()` |
| `src/service/dao/cortex/native/mod.rs` | Registry 增 `responses` 字段；`get()` 改签名 |
| `src/service/dao/cortex/native/responses.rs` | **新增** `ResponsesCortexDao` |
| `src/service/dao/cortex/native/http.rs` | 新增 responses 请求体 / input 转换 / SSE 解析 / 终态映射；`Usage` 增明细子结构 |
| `src/service/dal/brain.rs` | `get()` 调用点 :250 / :292 |
| `src/service/dao/cortex/mod.rs` | `get()` 调用点 :33 / :43 / :68 / :78 |
| `src/models/cortex_types.rs` | `ChatMessage::Reasoning` 变体 + 构造器 + `to_summary_text` 臂；`TokenUsage` 增 2 字段；`ThinkResult` 增 `reasoning_items` |
| `src/models/events/think_round.rs` | 增 2 字段 + `new()` + `with_model_usage()` |
| `src/service/domain/runtime/think_loop.rs` | 2 处 `with_model_usage` 传参；push Reasoning |
| `src/consumer/think_round_stats_consumer.rs` | 映射 2 个新字段 |
| `src/pkg/stats/model_call.rs` | 结构/new/builder/DDL/ALTER/insert/bulk_insert |
| `common/src/redaction/rule.rs` | 护栏白名单补 `reasoning_tokens` |
| `src/service/domain/runtime/awakening_tests.rs` | mock 穷举 match 增臂 |
| `http/tests.rs` / `cortex_types.rs` / `stats_duckdb_test.rs` | 配套用例 |

---

## 任务分期

### T1 底座（零行为变化，纯编译准备）
- [ ] `common/src/enums/provider.rs` 增 `ChatApiFlavor` 枚举
- [ ] `ModelProviderConfig` 增 `api_flavor` + `api_flavor_or_default()`
- [ ] `CortexDaoRegistry` 增 `responses` 字段、`get()` 改签名为 `get(&ModelProviderPo)`
- [ ] 6 个调用点跟进（`brain.rs:250/:292`、`cortex/mod.rs:33/:43/:68/:78`）
- [ ] `cargo check` 全绿，行为不变

### T2 统计字段扩展（可独立交付，存量 provider 立即受益）
- [ ] `http.rs` 的 `Usage` 增 `prompt_tokens_details` / `completion_tokens_details`
- [ ] `TokenUsage` 增 `cached_tokens` / `reasoning_tokens`，`finish_think_result` 映射
- [ ] `ThinkRoundEvent` 增字段 + `new()` + `with_model_usage()` 签名
- [ ] `think_loop.rs` 两处传参
- [ ] `think_round_stats_consumer.rs` 映射
- [ ] `ModelCallEvent` 7 个子点（含 ALTER）
- [ ] `rule.rs` 护栏白名单补 `reasoning_tokens`
- [ ] `stats_duckdb_test.rs` 扩写求和断言

### T3 `ChatMessage::Reasoning`
- [ ] 变体定义 + 构造器
- [ ] 3 个穷举点补臂（chat 协议的 `messages_to_json` 跳过 + 日志）
- [ ] `ThinkResult` 增 `reasoning_items` + accessor
- [ ] 守卫测试：`role == "reasoning"`；`encrypted_content` 不进 `to_summary_text`

### T4 Responses 请求体与非流式
- [ ] `responses.rs` 骨架 + `/responses` 端点
- [ ] `messages_to_input`（含 Reasoning 臂）
- [ ] tools 扁平化（`strict: false`）、`store: false`
- [ ] 非流式响应解析 + 终态映射

### T5 Responses SSE 流式
- [ ] 语义事件解析（8 类事件）
- [ ] 聚合状态机（按 item id 而非 choices index）
- [ ] 终态判定改表 + `check_stream_end` 改表
- [ ] usage 从 `response.completed` 取

### T6 think_loop 回传
- [ ] 按 Reasoning → Assistant → Tool 顺序 push
- [ ] 校验顺序与 Responses output 一致

### T7 测试与联调
- [ ] `http/tests.rs` 对照新增用例（现有 34 个）
- [ ] 真实厂商联调，重点验证事件齐全性与 reasoning 回传必要性

---

## 风险与验证

| 风险 | 判据 | 防线 |
|---|---|---|
| **静默退化 Final** | 工具不执行但不报错 | `finish_reason → status` 映射表 + 单测断言"有 function_call 必产出 ToolCall" |
| **静默截断** | 长输出被切且无 error | `check_stream_end` 同步改表 + 断言 `incomplete` 必报错 |
| **encrypted_content 泄漏** | 加密串进用户可见文本/记忆 | 照抄 base64 先例 + 守卫测试断言 `!summary.contains(...)` |
| **reasoning 发给不支持的 provider** | 400 | chat 协议 `messages_to_json` 跳过 + 日志；作用域限定单轮（同 provider） |
| **MCP schema 被 strict 挑刺** | 工具注册 400 | 第一版 `strict: false` |
| **DuckDB 旧库缺列** | 查询报未知列 | `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` 幂等补列 |

**验证命令**：`make ci`（lint + test）；前端无关，无需 `dx check`。

---

## 附录 A：火山方舟（Doubao / Agent Plan）适配

厂商已确认为火山方舟。以下结论来自官方文档核对（2026-10-21），不是推测。

### A.1 支持性结论：支持

| 证据 | 来源 |
|---|---|
| Agent Plan 的接入协议选项直接列出 `openai-responses`，且与 `openai-completions` **共用同一 Base URL** `https://ark.cn-beijing.volces.com/api/plan/v3` | Agent Plan 三方工具接入文档 |
| 模型详情页「API 支持范围」列出 `Responses /v3/responses` | 方舟控制台 · 模型详情 |
| 能力矩阵中**显式缓存 / MCP / 知识库 / 联网搜索**四项标注均为 `Responses API` | 方舟控制台 · 模型能力 |

><details open><summary>结论</summary>
>
>Responses 在火山不是"兼容层"，而是**能力主线**：MCP、知识库、联网搜索、上下文缓存只有 Responses 提供。
>继续用 chat 等于主动放弃这四项能力。这是切换的核心理由，强于"协议新旧"本身。
>
></details>

### A.2 ⚠️ Base URL 陷阱（优先级最高，与协议无关）

Agent Plan 与后付费 API 调用的 Base URL **不同**，专属 API Key 也不通用：

| 场景 | OpenAI 兼容 Base URL |
|---|---|
| **Agent Plan**（订阅制） | `https://ark.cn-beijing.volces.com/api/plan/v3` |
| 方舟 API 调用（后付费） | `https://ark.cn-beijing.volces.com/api/v3` |

而 `http.rs::default_base_url` 对 `ProviderType::Doubao | DoubaoVision` 返回的是 **`https://ark.cn-beijing.volces.com/api/v3`（后付费）**。

⇒ 若 provider 未显式覆盖 `base_url`，即使持有 Agent Plan 专属 Key，打出去的也是后付费通道，
额度不走套餐。**上线前必须逐个核对运行库 provider 的 `base_url` 实际值。**

URL 拼接为 `{base_url}/responses`，故两种 Base URL 分别得到 `/api/plan/v3/responses` 与 `/api/v3/responses`，与文档一致。

### A.3 wire 核对：与 OpenAI 一致的部分（实现无需改）

- `usage`：`input_tokens` / `output_tokens` / `total_tokens` + `input_tokens_details.cached_tokens` + `output_tokens_details.reasoning_tokens` —— **命名与 OpenAI 逐字一致**，附录 §三 的两个新增统计字段在火山上直接有数据。
- `tools`：**扁平** `{type:"function", name, description, parameters}`，无 `function` 嵌套。
- output item：`reasoning`（`id`=`rs_xxx`、`summary`=`[{type:"summary_text", text}]`、`status`）、`message`（`id`=`msg_xxx`、`content`=`[{type:"output_text", text}]`）、`function_call`（`id`=`fc_xxx` + `call_id`=`call_xxx`、`name`、`arguments`）。
  - 回传以 **`call_id`** 为键（非 `id`），与 OpenAI 一致，实现已正确取 `call_id`。
- 终态：`status` = `completed` / `incomplete` / `failed`，`incomplete_details` 同构。
- SSE 事件名与 OpenAI 一致（`response.created` / `output_text.delta` / `function_call_arguments.delta` / `completed` / `failed` / `incomplete`…）。
  - 方舟额外发 `response.reasoning_summary_part.added|done`、`response.reasoning_summary_text.delta` 三个事件，**实现忽略即可**（本实现只读终态 envelope，不逐 delta 累加）。

### A.4 方舟特有字段（暂不启用，YAGNI）

`caching`（上下文缓存）、`context_management`、`thinking`、`max_tool_calls`、`expire_at`，
以及 header `X-Fornax-Trace: true`（仅 Responses 支持，可在推理接入点看 Trace）。

> 注意：`caching` 与 `instructions`、非 Function 类 `tools` **互斥**；`context_management` 依赖 `store=true`。
> 二者与当前 `store:false` 决策冲突，若将来要启用需重新评估脱敏边界。

### A.5 联调必验清单（按风险排序）

| # | 验证项 | 不经核实的后果 |
|---|---|---|
| 1 | 运行库 provider 的 `base_url` 是否为 `/api/plan/v3` | 额度不走 Agent Plan，静默烧后付费 |
| 2 | `store:false` 是否被接受、是否影响 reasoning 回传 | 多轮 400（若方舟强制要求 encrypted_content） |
| 3 | 接受无状态 `input` 数组（不用 `previous_response_id`） | 需引入会话态，成本翻倍 |
| 4 | 工具名 LCD 约束（已知方舟对 `function.name` 有 ASCII 限制） | 该 Agent 每轮全挂，且报错指不出具体工具 |
| 5 | 多轮 function_call 往返（`function_call` → `function_call_output`） | 工具链路静默断 |
| 6 | `cached_tokens` / `reasoning_tokens` 实际是否上报 | 统计列恒为 0，误判为未生效 |

> 第 4 项已有防线：出站统一走 `to_wire_tool_name` 现算 wire name（`^[a-zA-Z0-9_-]{1,128}$`），
> 有护栏测试 `builtin_tool_wire_names_are_lcd_safe_and_unique`。本附录不新增要求。

### A.6 默认协议决策（2026-10-21 已拍板）

**拍板结论：`api_flavor` 缺省即 `Responses`，不做按 provider_type 的特判；
特殊情况由用户显式配置 `"chat_completions"` 回退。**

| 位置 | 缺省语义 |
|---|---|
| `ChatApiFlavor::default()` | `Responses`（`#[default]` 已迁移） |
| `ModelProviderConfig::api_flavor_or_default()`（字段 `None`） | `Responses` |
| `provider_api_flavor()` 且 **config 解析失败** | `ChatCompletions`（唯一例外，见下） |

**为什么敢把缺省改成主线协议**：

- Responses 是能力主线——方舟的 MCP / 知识库 / 联网搜索 / 上下文缓存只有它提供，继续用 chat 等于放弃这四项；
- 覆盖面已足够：方舟、OpenAI 官方、Ollama（v0.13.3+ 起提供**无状态** `/v1/responses`）均支持；
- 用户全部 provider 都是火山 Agent Plan，同一厂商内不存在"部分支持"的分裂情况。

**唯一例外：config 解析失败仍兜底 `ChatCompletions`。**
两条路径刻意不同——解析失败意味着记录已损坏、无从得知用户意图，此时选覆盖面最广的端点
（`/chat/completions` 是 OpenAI 兼容网关的事实标准）更可能让推理继续跑通；
且该分支有 `log_warn!` 告警，属显式降级路径而非静默兜底。

**已知需要用户显式回退的场景**：自建网关 / 老旧 OpenAI 兼容层 / Ollama < v0.13.3 ——
这些不具备 `/responses` 端点，需在 provider 配置里显式写 `api_flavor: "chat_completions"`。

**存量影响**：所有未显式配置 `api_flavor` 的 provider 会在本次升级后改走 Responses。
这是**有意为之**（无需 migration、缺省即切换），但意味着上线后需盯一轮推理成功率。
