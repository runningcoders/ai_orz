# ai_orz 工具模块设计与开发总结

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：tool_design 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构
> - [runtime_design.md](./runtime_design.md) — Runtime 域内 Awakening 层 execute_auto/execute_manual 分发逻辑
> - [mcp_tool_design.md](./mcp_tool_design.md) — MCP 工具子系统的独立设计
> - 【② Plan 落地】[进程管理与shell_exec修复.md](../plan/进程管理与shell_exec修复.md) — shell_exec 进程管理命令黑白名单 + 进程双暴露
> - 【② Plan 落地】[前端工具与进程管理.md](../plan/前端工具与进程管理.md) — 前端工具管理三 Tab 视图（Builtin/HTTP/MCP）
> - 【③ Wiki 长文】[工具生态系统.md](docs/wiki/zh/content/功能模块/工具生态系统/工具生态系统.md) — 工具系统全景：注册→分组→执行→统计
> - 【③ Wiki 长文】[统一工具调用架构.md](docs/wiki/zh/content/项目概述/核心功能特性/统一工具调用架构/统一工具调用架构.md) — 三协议路由 + 工具包 tag 分组
> - 【③ Wiki 长文】[工具注册与发现.md](docs/wiki/zh/content/功能模块/工具生态系统/工具注册与发现.md) — CoreTool trait 契约 + 注册表加载
> - 【③ Wiki 长文】[工具注册表.md](docs/wiki/zh/content/基础设施/工具注册表/工具注册表.md) — 工具注册表全景 + 内置/MCP/HTTP 三板块入口
> - 【④ RAG 卡】[工具系统三层调用架构：CoreTool trait + Builtin/HTTP/MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) — §三层调用链 §tag 免绑定 §6 条红线
> - 【平行 RAG】[技能系统 Seed 预置导入与 Agent 入职绑定](docs/wiki/knowledge/zh/技能系统%20Seed%20预置导入与%20Agent%20入职绑定：5%20套%20TEMPLATE_*%20编译期嵌入%20+%20install_skill_pack%20幂等%20Tag%20分发%20+%20Prompt%20Token%20熔断/技能系统%20Seed%20预置导入与%20Agent%20入职绑定：5%20套%20TEMPLATE_*%20编译期嵌入%20+%20install_skill_pack%20幂等%20Tag%20分发%20+%20Prompt%20Token%20熔断.md) — 入职后通过 ToolProvider 安装 tool_packs tag
> ⭐ **落地索引（四类互引）**
> - 对应 Plan 2 份真实 + 占位 1：[进程管理与shell_exec修复.md](../plan/进程管理与shell_exec修复.md) / [前端工具与进程管理.md](../plan/前端工具与进程管理.md) / 占位：待 handler_tool_registration_macro 设计落地独立 plan
> - 对应 Wiki 长文 4 篇（工具生态系统/统一工具调用架构/工具注册与发现/工具注册表，见上 【③ Wiki 长文】）
> - 对应 RAG 卡 1 份 + 平行卡 1 份（见上 【④ RAG 卡】+【平行 RAG】）

---

## 开发时间线（2026-04-17 / 2026-07-12 更新）

### 目标
设计并实现工具模块基础架构（自建 CoreTool trait + OpenAiCompatibleCortexDao，已移除 rig 依赖），支持多种协议（builtin/http/mcp），符合项目现有代码规范。

### 2026-07-12 更新内容
- **工具包机制**：tag 分组工具、Agent 入职自动安装、免绑定三层校验
- **协作工具**：search_agents、send_message_to_agent、list_agents、get_agent（tag: collaboration）
- **神经工具免绑定**：带 "neural" tag 的工具无需绑定即可调用
- **internal 工具机制**：带 "internal" tag 的工具不可绑定给 Agent（加载时过滤），由 `ToolDal::execute_manual` 内部通过 `registry.create_tool` 创建实例并转发调用（`request_tool_call`=同步 / `send_tool_call_message`=异步）

---

## 工具调用架构演进（2026-08-05 更新）

### Rig 移除 + 命名清理

参考 commit `f02addd`（移除 rig 依赖）与本轮命名清理（2026-08-05）：

- **Cortex 扁平化**：`BrainDal.think()` → `CortexDaoRegistry.get(provider_type)` → `dao.think()`（2 层），替代旧的 Brain → Cortex 实体 → CortexTrait → 实现 → CortexDao 工厂（5 层）。
- **ToolCallDao 原语重命名**：`ToolCallDao::call_manual` → `ToolCallDao::execute`，消除"manual"语义重载（旧名误导：实际所有工具都走这里，不分 Auto/Manual）。
- **删除冗余 forwarder**：`ToolDal::call_manual` / `McpToolDal::call_manual` 纯转发方法删除，`call_tool` 直接调 `ToolCallDao::execute`，减少一层无意义间接。

### 三层职责

| 层级 | 职责 | 入口 |
|------|------|------|
| **业务入口（DAL）** | `ToolDal::call_tool` | Auto/Manual/普通工具统一入口，转发到 DAO |
| **协议路由（domain）** | `RuntimeDomainImpl::call_tool` | 按 `ToolProtocol` 分发：Mcp → `McpToolDal`；Builtin/Http → `ToolDal` |
| **执行原语（DAO）** | `ToolCallDao::execute` | 套 `LoggingDecorator` → 调 `CoreTool::call`，生成真实 `call_id` |

### 三条执行路径全部汇流到 `ToolCallDao::execute`

```
[1] awakening 循环 LLM 主动调 Auto 工具
    execute_auto → call_tool → ToolCallDao::execute

[2] awakening 循环 LLM 主动调 Manual 工具
    execute_manual → special_tool(request_tool_call / send_tool_call_message)
                  → call_manual_tool_for_agent → call_tool → ToolCallDao::execute

[3] 消息驱动（Consumer 处理 ToolCallRequest）
    call_manual_tool_for_agent → call_tool → ToolCallDao::execute
```

### 装饰器收敛 + Trace 完整性

- 装饰收敛到 `ToolCallDao::execute` 内部：每次调用 `dyn_clone::clone_box(&*tool.our_tool)` 后 `ToolCallLoggingDecorator::new(cloned)`，保证每次调用拿到独立的 `ToolCallEntry`。
- 成功时返回 `(Value, ToolCallEntry)`，`entry.call_id` 由 `LoggingDecorator` 生成真实 UUID，调用方应使用此 `call_id` 构造 `ToolExecutionResult`，不再伪造。
- 失败时 `entry` 被 consume 构造 `ToolCallTraceRef`，Error 携带 `trace_ref`，便于事后追溯。

---

## 最终架构设计

### 目录结构
```
ai_orz/
├── common/src/enums/
│   └── tool.rs                  # 枚举：ToolProtocol、ToolStatus
├── src/
│   ├── models/
│   │   └── tool.rs              # 持久化对象 ToolPo
│   ├── pkg/
│   │   └── tool_registry/       # 全局工具实例注册中心（独立解耦）
│   │       ├── mod.rs           # ToolRegistry 定义
│   │       ├── builtin.rs       # BuiltinTool trait
│   │       ├── http.rs          # HTTP 工具（占位）
│   │       └── mcp.rs           # MCP 工具运行时（规划中，详见 docs/archive/design-archive/mcp_tool_design.md）
│   └── service/
│       └── dao/
│           └── tool/            # Tool DAO 层
│               ├── mod.rs        # ToolDao trait 定义
│               ├── sqlite.rs    # SQLite 实现
│               └── sqlite_test.rs # 单元测试
└── migrations/
    └── 20260417000000_create_tools.sql # 数据库迁移
```

### 职责拆分
| 模块 | 职责 |
|------|------|
| `common/enums/tool.rs` | 定义 `ToolProtocol`（builtin/http/mcp）、`ToolStatus`（enabled/disabled）枚举，支持 SQLx 存储 |
| `models/tool.rs` | `ToolPo` 持久化对象，所有 ID 都是 `String`，对齐项目现有风格 |
| `pkg/tool_registry` | **全局工具实例注册中心**，独立于 DAO，职责单一：<br>- 按协议分类存储工具实例<br>- 提供注册和查询接口<br>- 内置工具实现 `CoreTool` trait（项目自建，带 `RequestContext`） |
| `service/dao/tool` | **工具元数据持久化**：<br>- CRUD 操作<br>- Agent 绑定工具的增删查改<br>- 不持有连接池，所有操作从 `RequestContext` 获取连接池，符合 DAO 规范 |

---

## 核心设计决策

### 1. ID 类型：`String` vs `Uuid`
- 最终选择：**`String`**
- 原因：项目现有所有模块都用 `String` 存储 ID，保持一致性；无需强制 Uuid，支持用户自定义标识符更灵活
- 实现：`ToolPo::new(id, ...)` 如果传入空字符串，内部自动生成 Uuid v7 字符串

### 2. 注册中心位置：DAO 层 vs 独立 pkg
- 最终选择：**独立 pkg/tool_registry**
- 原因：DAO 只负责持久化元数据，注册中心负责内存实例管理，职责分离解耦，符合项目 pkg 存放基础设施的约定

### 3. CoreTool trait 设计（自建，替代 Rig ToolDyn）
- 项目自建 `CoreTool` trait，所有工具统一实现此接口
- `CoreTool` 自带 `RequestContext` 参数，支持权限、日志、追踪
- trait 定义：`async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value, ToolError>` + `parameters_schema()` + `name()` + `description()`
- 继承 `DynClone + Send + Sync + Debug`，支持 `Box<dyn CoreTool + Send + Sync>` 动态分发
- 已移除 rig 依赖，不再使用 `ToolDyn`

### 4. 数据库设计
两张表：
- `tools`：工具元数据表
  - `id` TEXT PRIMARY KEY
  - `name` TEXT NOT NULL
  - `description` TEXT
  - `protocol` TEXT NOT NULL
  - `config` TEXT NOT NULL  (JSON 序列化)
  - `parameters_schema` TEXT (JSON 序列化)
  - `status` TEXT NOT NULL
  - `created_at` INTEGER NOT NULL
  - `updated_at` INTEGER NOT NULL
  - `created_by` TEXT
  - `updated_by` TEXT
- `agent_tools`：Agent 绑定工具关系表
  - `agent_id` TEXT NOT NULL
  - `tool_id` TEXT NOT NULL
  - `created_at` INTEGER NOT NULL
  - `created_by` TEXT
  - PRIMARY KEY (agent_id, tool_id)

> 去掉外键约束，简化迁移和测试，符合项目约定。

### 5. 枚举存储兼容
- SQLx 默认使用枚举变体名，项目中枚举输出小写，因此添加 `#[sqlx(rename_all = "lowercase")]`
- 所有枚举都添加了 `sqlx::Type` derive，支持直接从数据库解码

---

## 开发过程中踩过的坑

| 问题 | 根因 | 解决方案 |
|------|------|----------|
| JSON 类型 SQLite 不支持 | 迁移文件最初写了 `JSON` 类型 | 改为 `TEXT` 类型，应用层处理 JSON 序列化 |
| UUID 解码错误 "expected 16 bytes, found 36" | SQLite 存储 UUID 为字符串，SQLx 需要特殊处理 | 直接改用 `String` 存储 id，去掉 Uuid 依赖 |
| 枚举解码错误：找不到 "builtin" | SQLx 默认期望 PascalCase `"Builtin"`，但实际存储小写 | 添加 `#[sqlx(rename_all = "lowercase")]` |
| Rig `Tool` trait 不支持 `Box<dyn Tool>` | async 方法默认有 `Sized` 约束 | 已移除 rig 依赖，改用自建 `CoreTool` trait（继承 `DynClone + Send + Sync`），原生支持 `Box<dyn CoreTool + Send + Sync>` |
| `cargo fix` 自动误改其他 DAO 测试导入 | 原来其他 DAO 没有在 `mod.rs` 重新导出 `dao()`，`cargo fix` 误以为调用错误 | 统一所有 DAO 导出规范：`mod.rs` 导出 `pub use sqlite::{init, dao};` |
| 值移动错误：`tool_id` 借用后 move | `add_tool_to_agent` 参数按值传 String | 改为 `&str` 借用，符合 Rust 风格，调用方不需要 clone |

---

## 单元测试

tool DAO 测试覆盖了所有核心功能：
1. `test_create_and_get_by_id` - 创建并按 ID 查询
2. `test_update_tool` - 更新工具信息
3. `test_get_by_name` - 按名称查询
4. `test_list_enabled` - 列出所有启用工具
5. `test_add_and_list_for_agent` - 添加工具到 Agent 并列出
6. `test_remove_from_agent` - 从 Agent 移除工具

**全部 6 个测试通过**

## 全项目测试结果

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

---

---

## Agent 工具绑定架构（2026-04-18 更新）

### 目标
将已存储的工具绑定到 Agent，Awakening 显式循环中将工具派生为 `ToolDescriptor` 传给 `BrainDal.think()`，支持 Agent 通过 function calling 调用工具。严格遵循项目分层规范：`handler → domain → dal → dao`，禁止同层互调。

### 更新后的架构

#### 目录结构变化
```diff
 ai_orz/src/
 ├── models/
 │   └── tool.rs              # + Tool 复合实体 (ToolPo + Box<dyn CoreTool + Send + Sync>)
 │   └── agent.rs             # + Agent 新增 tools: Vec<Tool> 字段
 ├── pkg/
 │   └── tool_registry/       # (已有) 全局工具实例注册中心
 └── service/
     └── dao/
     │   └── tool/
     │       ├── mod.rs       # + get_tool_full / list_tools_for_agent_full
     │       ├── sqlite.rs    # 实现完整工具拼装
     │       └── sqlite_test.rs # + 8 个单元测试覆盖新功能
     └── dal/
         └── agent/
             ├── mod.rs       # + get_agent_with_tools
             └── sqlite.rs   # 实现 Agent + 工具拼装
```

#### 完整职责链
```
1. Domain 层需要获取带完整工具的 Agent
   ↓
2. Domain 调用 AgentDal.get_agent_with_tools(ctx, agent_id)
   ↓
3. AgentDal 组合：
   - AgentDao.get_agent(ctx, agent_id) → 获取 AgentPo
   - ToolDao.list_tools_for_agent_full(ctx, agent_id) → 获取已拼装好的 Vec<Tool>
   ↓
4. ToolDao.list_tools_for_agent_full 内部：
   - 查询 DB 得到 Vec<ToolPo>（绑定到该 Agent 的所有启用工具）
   - 对每个 ToolPo，调用 registry.create_tool(po) 获取 Box<dyn CoreTool + Send + Sync>
   - 拼装成 Tool { po: tool_po, our_tool: boxed_dyn }
   - 自动过滤未在注册中心找到的工具（已删除/未实现）
   ↓
5. AgentDal 用 Agent::from_po_with_tools(agent_po, tools) 返回完整 Agent
```

### 核心设计决策

| 问题 | 方案 | 原因 |
|------|------|------|
| **谁来拼装完整 Tool？** | ToolDao 负责 | DAO 只负责自己领域的对象拼装，符合单一职责 |
| **Tool 应该包含什么？** | `Tool { po: ToolPo, our_tool: Box<dyn CoreTool + Send + Sync> }` | 分离元数据（PO）和运行实例（CoreTool dyn），Awakening 按 control_mode 调用 execute_auto/execute_manual |
| **get_agent_with_tools 放哪层？** | AgentDal 层 | Dal 职责就是组合多个 DAO 构建完整业务实体，不违反分层规则 |
| **think() 接收什么？** | `&[ToolDescriptor]`（从 Tool 派生） | Awakening 把 agent.tools 派生为 ToolDescriptor 传给 BrainDal.think()，工具执行由 execute_auto/execute_manual 负责 |
| **工具存在哪里？** | Agent 实体持有 `Vec<Tool>` |领域概念：工具属于 Agent，Brain 只持有 ModelProviderPo，think 时按需传入 ToolDescriptor |

### Rig 依赖移除说明

项目已完全移除 rig-core 依赖：
- **CortexDao 自建**：`OpenAiCompatibleCortexDao` 直接通过 reqwest HTTP 调用 OpenAI 兼容 API（POST /chat/completions）
- **不再有 ToolDyn / RigToolAdapter / unsafe transmute**：所有工具统一实现自建 `CoreTool` trait，原生支持 `Box<dyn CoreTool + Send + Sync>` 动态分发
- **Brain 扁平化**：Brain 直接持有 `Option<ModelProviderPo>`，BrainDal.think() 按 `provider_type` 在 `CortexDaoRegistry` 中选择 DAO 实现
- **工具调用三层架构**：上层 `execute_auto`/`execute_manual` → 中层 `call_tool` → 底层 `ToolCallDao.execute` + `decorate`

### 分层规范符合性检查

✅ **严格单向逐层调用**：`handler → domain → dal → dao`，无反向调用  
✅ **禁止同层互调**：dal 不调用 dal，dao 不调用 dao（本次 `AgentDal` 调用 `AgentDao + ToolDao`，是 dal 组合 dao，符合规则）  
✅ **职责分离清晰**：每个层只做自己该做的事，不越界  
✅ **DAO 只做单表/单领域操作**：`ToolDao` 只拼装 Tool 不碰 Agent，符合约定

### 单元测试更新

新增 8 个单元测试，覆盖新增功能：
1. `test_create_and_get_tool_full` - 创建工具并查询完整实体（验证注册中心过滤）
2. `test_get_tool_full_exists` - 查询已存在工具的完整实体（验证注册中心集成）
3. `test_add_tool_to_agent_and_list` - 绑定多个工具到 agent 并列出（验证关联查询）
4. `test_remove_tool_from_agent` - 解绑工具验证（验证解绑逻辑）
5. `test_list_enabled` - 列出启用的工具（验证状态过滤）
6. `test_get_by_name` - 按名称查询工具（验证唯一性查询）
7. `test_update_tool` - 更新工具信息（验证更新持久化）
8. `test_find_not_exists` - 查询不存在工具返回 None（边界测试）

### 测试结果
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

---

## 工具调用自动追踪（2026-04-20 ~ 2026-04-21 更新）

### 目标
为 Awakening 循环中调用的所有工具自动添加完整调用日志追踪，记录完整的输入输出、调用参数、错误信息，方便调试、审计和后续训练数据收集。保持非侵入式设计，装饰器逻辑收敛到 `ToolCallDao` 内部，不修改 `CoreTool` 接口，方便后续扩展。

### 架构设计

#### 目录结构
```diff
 ai_orz/src/
 ├── models/
 │   └── tool.rs              # (已有) Tool 复合实体
 ├── pkg/
 │   ├── tool_registry/       # (已有) 全局工具实例注册中心
+│   └── tool_tracing/        # 新增：工具调用日志追踪模块
+│       ├── entry.rs         # ToolCallEntry 定义 + ToolCallStatus 枚举
+│       ├── logger.rs        # ToolCallLogger 单例工厂 + JSONL 写入
+│       ├── tool_call_logger.rs # ToolCallLoggingDecorator - 装饰器包装 CoreTool
+│       ├── mod.rs           # 模块导出
+│       └── logger_test.rs   # 完整单元测试
 └── service/
     └── dao/tool_call/
         └── impl.rs          # ToolCallDao.execute + decorate（装饰器收敛）
```

#### 工作流程图
```
应用启动
  ↓
ToolCallLogger::init(base_data_path) → 全局单例初始化完成
  ↓
Awakening 显式循环 → 按 control_mode 分发：
  - Auto:  ToolDal.execute_auto → call_tool
  - Manual: ToolDal.execute_manual → registry.create_tool(internal tool) → 转发 → call_tool
  ↓
call_tool → ToolCallDao.execute(tool, args)
  ↓
内部 decorate(): ToolCallLoggingDecorator::new(dyn_clone::clone_box(&tool.our_tool))
  ↓
decorated.call_with_entry(ctx, args)
    → 调用原始 CoreTool::call(ctx, args) 得到结果
    → 自动构造 ToolCallEntry 包含完整上下文（call_id, tool_id, agent_id, task_id...）
    → ToolCallLogger::get().log_call() → 写入 daily JSONL 文件
    → 返回 (Value, ToolCallEntry) 给上层
```

### 存储结构

日志文件按工具+日期分文件存储，路径格式：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

每个 JSONL 行是一个完整的 `ToolCallEntry`：
```rust
pub struct ToolCallEntry {
    pub call_id: String,         // 唯一调用 ID
    pub tool_id: String,         // 工具 ID
    pub tool_name: String,       // 工具名称
    pub agent_id: Option<String>,// 关联 Agent ID
    pub task_id: Option<String>, // 关联任务 ID
    pub project_id: Option<String>, // 关联项目 ID
    pub started_at: u64,        // 开始时间毫秒时间戳
    pub finished_at: u64,        // 结束时间毫秒时间戳
    pub duration_ms: u64,        // 调用耗时毫秒
    pub input: serde_json::Value,// 输入参数
    pub output: Option<serde_json::Value>, // 输出结果
    pub error: Option<String>,   // 错误信息（如果失败）
    pub status: ToolCallStatus,  // 调用状态：Started/Completed/Failed
    pub metadata: serde_json::Value, // 扩展元数据
}
```
> 当前实现参考：[tool 相关模块](src/service/dal/tool.rs)

查询能力（Batch I 已完成）：
- `ToolCallLogger` 支持按 `call_id`、`tool_id`、`agent_id`、`project_id`、`task_id`、`status`、时间范围和 `limit` 扫描 tool-specific daily JSONL；
- 默认返回最新匹配记录（`limit = 1`），并设置最大 `limit` 防止无界 IO/内存放大；
- Runtime Domain 负责 scope 边界：查询必须从 `RequestContext` 派生可信 `agent_id/project_id/task_id`，request scope 只能收窄且必须与同字段 context scope 匹配；
- HTTP handler 返回 `ToolCallEntryDetail` 前统一脱敏 `input/output/error/metadata`，公共 API 不暴露 JSONL date、line number 或文件路径。

### 核心设计决策

| 问题 | 方案 | 原因 |
|------|------|------|
| **在哪里添加日志包装？** | `ToolCallDao.execute` 内部 | 装饰器收敛到 ToolCallDao，`call_tool` / `execute_auto` / `execute_manual` 都走此路径，单点记录 trace |
| **日志配置放在哪里？** | ToolCallLogger 从 config singleton 获取 | 配置已经是全局单例，不需要通过 DAO 传递参数，减少 API 污染 |
| **全局还是每个工具一个实例？** | 全局单例工厂 | base path 只需要初始化一次，每个调用按需获取 writer，没有重复创建开销 |
| **是否支持测试？** | 保留 `new()` 构造方法 | 测试可以创建本地实例用临时目录，不影响全局单例 |
| **什么时候写入日志？** | 调用完成后写入一次 | 只需要最终结果，不需要启动时写一条，简化设计；Started 状态保留给未来自调度工具 |
| **是否侵入原有代码？** | 装饰器模式 | 完全不修改 `CoreTool` 接口，符合开闭原则；`ToolCallDao.decorate()` 内部方法供未来叠加 StatsDecorator 等 middleware |

### 设计符合项目分层规范

✅ **严格单向逐层调用**：没有新增跨层调用  
✅ **职责单一清晰**：日志追踪是独立横切关注点，装饰器模式完美分离  
✅ **配置不依赖注入**：配置已经是全局单例，`ToolCallLogger` 直接获取符合约定  
✅ **单元测试完整覆盖**：5 个单元测试全部通过  

### 单元测试

新增 5 个单元测试覆盖核心功能：
1. `test_tool_call_logger_basic` - 基础日志读写测试
2. `test_tool_call_logger_multiple_calls` - 多次调用按行追加测试
3. `test_tool_call_logger_different_tools_separate_paths` - 不同工具分开目录存储测试
4. `test_tool_call_failed_entry` - 失败调用记录错误信息测试
5. `test_tool_call_with_context_ids` - 关联 Agent/Task/Project ID 测试

**全部 5 个测试通过**

---

## 后续待扩展

1. **统计模块驱动的外部唤醒轮次**：ToolCallResult 可触发 Agent 下一次唤醒；是否继续、轮次上限、暂停/继续和页面可见的任务/Agent 运行状态统一来自统计模块；普通工具审计详情继续通过已完成的强类型 `trace_ref = ToolCallTraceRef { tool_id, call_id }` 查询 ToolCallEntry
2. **ToolCallResult 产物化引用策略**：仅当结果需要用户下载或成为 Project Artifact 时接入 attachment / artifact；普通工具审计详情不复制到 attachment / artifact
3. **ToolEmbedding 语义自动选择**：基于 embedding 做工具相关性排序，减少上下文
4. **运行时动态加载工具增强**：在现有 Builtin/HTTP/MCP 基础上继续完善生命周期、缓存和健康检查

---

## 混合模式工具调用链路（2026-04-22 更新）

### 目标
实现**简单工具自动 + 关键工具收敛**的混合模式工具调用链路：
- `auto` 模式：简单工具走 Awakening 显式循环中的同步 function call 流程，开发高效
- `manual` 模式：关键工具走自建异步事件链路，支持权限控制、全链路审计、大结果附件存储

满足多 Agent 协作场景下对关键工具调用的可控性要求。已移除 rig 依赖，Auto/Manual 工具都通过自建 `CoreTool` + `OpenAiCompatibleCortexDao` 调用。

### 核心设计决策

| 设计点 | 方案 | 原因 |
|--------|------|------|
| **混合模式分类** | `control_mode: auto \| manual` | 不是按工具类型分，而是按控制要求分：简单工具 `auto`，需要审计/权限 `manual` |
| **工具调用存储** | 复用现有 `messages` 表 | 工具调用本身就是特殊消息，利用已有消息状态、附件存储、关联机制，不新建表 |
| **核心 trait** | 内部统一用 `CoreTool`（带 `RequestContext`） | 所有工具都需要访问上下文（DB、用户、权限、跟踪ID），统一接口方便装饰器 |
| **Auto 调用** | `execute_auto` → `call_tool` 直接执行 | Awakening 循环中模型发起 tool_call 时按 control_mode 分发，结果作为 ChatMessage::Tool 追加到对话历史 |
| **Manual 调用** | `execute_manual` 通过 internal 工具转发 | `request_tool_call`（同步）/`send_tool_call_message`（异步）由 registry.create_tool 创建实例，转发到 `call_tool` 或消息链路 |
| **日志装饰** | `ToolCallDao.execute` + `decorate` 内部收敛 | 装饰器逻辑收敛到 DAO 层，单点记录 ToolCallEntry trace，未来可叠加 StatsDecorator |
| **注册中心** | 存储工厂而非实例 | 每个工具实例从 `ToolPo` 创建，配置可动态从 DB 读取 |

### 消息类型扩展

在 `common/src/enums/message.rs` 的 `MessageType` 中新增：
```rust
pub enum MessageType {
    // ... existing variants
    ToolCallRequest,  // manual 模式：LLM 请求调用工具
    ToolCallResult,   // manual 模式：工具执行完成返回结果
}
```
> 当前实现参考：[tool 相关模块](src/service/dal/tool.rs)

### 核心结构

```rust
// src/models/tool.rs

/// CoreTool trait - 项目核心工具接口，所有工具都必须实现
/// 自带 RequestContext 上下文，支持权限、日志、追踪
#[async_trait]
pub trait CoreTool: DynClone + Send + Sync + Debug {
    /// 执行工具调用
    /// - ctx: RequestContext 包含用户、DB 连接、trace 等信息
    /// - args: JSON 参数（由 LLM 生成）
    /// - 返回: JSON 结果
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value, ToolError>;

    /// 工具参数 JSON Schema
    fn parameters_schema(&self) -> Value;

    /// 工具名称（用于 LLM function calling 识别）
    fn name(&self) -> &str;

    /// 工具描述（给 LLM 看）
    fn description(&self) -> &str;
}

/// 完整工具业务实体 - 包含持久化配置和可执行实例
pub struct Tool {
    pub po: ToolPo,              // 持久化配置（DB 读出）
    pub control_mode: ControlMode, // auto | manual（从 po.control_mode 派生）
    pub our_tool: Box<dyn CoreTool + Send + Sync>,         // 核心实现（未装饰）
}

// 注：已移除 rig_tool / RigToolAdapter / ToolDyn 适配
// Awakening 调用 BrainDal.think(ctx, brain, &messages, &tool_descriptors)，
// ToolDescriptor 通过 From<&Tool> 从业务 Tool 直接派生（name/description/parameters），
// 工具执行由 execute_auto / execute_manual 负责，不再需要 Rig 适配层。

/// 向后兼容类型别名
pub type FullTool = Tool;
```
> 当前实现：[tool_registry/mod.rs::ToolDescriptor 相关](src/pkg/tool_registry/mod.rs)

### 目录结构最终

```
ai_orz/
├── common/src/enums/
│   └── message.rs             # + MessageType: ToolCallRequest/ToolCallResult, + ControlMode
├── src/
│   ├── models/
│   │   ├── tool.rs            # CoreTool trait + Tool entity（无 RigToolAdapter）
│   │   └── cortex_types.rs    # ToolDescriptor + ChatMessage + ThinkResult（think 契约）
│   ├── pkg/
│   │   ├── tool_registry/
│   │   │   ├── mod.rs         # ToolRegistry - 存储工厂，create_tool() -> Box<dyn CoreTool>
│   │   │   ├── builtin.rs     # BuiltinToolFactory - 内建工具工厂 trait
│   │   │   ├── http.rs        # HTTP 工具实现
│   │   │   └── mcp.rs         # MCP 工具运行时
│   │   └── tool_tracing/
│   │       ├── mod.rs         # 导出
│   │       ├── entry.rs       # ToolCallEntry / ToolCallStatus - JSONL 日志结构
│   │       ├── logger.rs      # ToolCallLogger - 全局日志单例
│   │       └── tool_call_logger.rs # ToolCallLoggingDecorator - 包装 CoreTool 添加日志
│   └── service/
│       ├── dao/
│       │   ├── tool/
│       │   │   ├── mod.rs     # ToolDao trait
│       │   │   └── sqlite.rs  # SQLite 实现 - get_tool_full() 拼装
│       │   ├── tool_call/
│       │   │   ├── mod.rs     # ToolCallDao trait（含 decorate 方法）
│       │   │   └── impl.rs    # execute + decorate 装饰器收敛
│       │   └── cortex/
│       │       ├── mod.rs     # CortexDao trait + CortexDaoRegistry 分发
│       │       └── native/
│       │           ├── mod.rs # CortexDaoRegistry 单例
│       │           ├── openai.rs # OpenAiCompatibleCortexDao
│       │           └── http.rs   # OpenAI 兼容 API HTTP 调用
│       ├── dal/
│       │   ├── tool.rs        # ToolDal: execute_auto / execute_manual / call_tool
│       │   └── brain.rs       # BrainDal: think(ctx, brain, &messages, &tools)
│       └── domain/runtime/
│           └── awakening.rs   # Awakening 显式循环（按 control_mode 分发）
└── migrations/
    └── 20260417000000_create_tools.sql # 已包含 control_mode 字段
```

### 拼装流程（ToolDao.get_tool_full）

```
Input: ToolPo from DB
  ↓
1. 从 ToolRegistry 根据 protocol 获取工厂，create_tool(po) → Box<dyn CoreTool + Send + Sync>
  ↓
2. 不在拼装阶段装饰（装饰收敛到 ToolCallDao.execute 内部）
  ↓
3. 返回 Tool { po, control_mode: po.control_mode, our_tool: boxed_dyn }
  ↓
（our_tool 保持未装饰，执行时由 ToolCallDao.execute 内部 decorate 临时装饰）
```

### 工作流程图

#### Auto 模式（Awakening 显式循环）
```
User message → Awakening 循环 → BrainDal.think(messages, tool_descriptors) → OpenAiCompatibleCortexDao → HTTP /chat/completions
                                          ↓
                                    ThinkResult::ToolCall?
                                          ↓
                                    execute_auto(tool, args) → call_tool
                                          ↓
                                    ToolCallDao.execute → decorate → CoreTool.call(ctx, args)
                                          ↓
                                    ToolCallLoggingDecorator 记录日志 → 返回 (Value, ToolCallEntry)
                                          ↓
                                    追加 ChatMessage::Tool 到 messages → 继续循环
```

#### Manual 模式（通过 internal 工具转发）
```
Awakening 循环 → BrainDal.think → ThinkResult::ToolCall?
   ↓
execute_manual(tool, args)
   ↓
根据 tool.po.config.dispatch_mode 选择 internal 工具：
   ├── sync（默认）: registry.create_tool("request_tool_call")
   │       ↓
   │   special_tool.call() → call_manual_tool_for_agent()
   │       ↓
   │   call_tool → ToolCallDao.execute → decorate → CoreTool.call
   │       ↓
   │   同轮返回 (Value, ToolCallEntry)，追加为 ChatMessage::Tool 继续循环
   │
   └── async: registry.create_tool("send_tool_call_message")
           ↓
       special_tool.call() → MessageDomain.delivery.send_tool_call_request()
           ↓
       消息入队，立即返回"已提交"
           ↓
       Consumer 收到 ToolCallRequest 消息（to_role=System）
           ↓
       tool_execution.call_manual_tool_for_agent() → call_tool
           ↓
       构造 ToolCallResult 消息存入 messages 表
           ↓
       触发下一次 awaken()，Agent 读取 ToolCallResult 继续
```

### 分层符合性检查

✅ **严格单向逐层调用**：`handler → domain → dal → dao`，无反向调用  
✅ **禁止同层互调**：dal 组合 dao，不跨 dal 调用  
✅ **复用现有基础设施**：消息表、事件总线、附件存储全部复用  
✅ **职责分离清晰**：注册中心、日志装饰（ToolCallDao 内部收敛）、internal 工具转发分开，单一职责

### 测试结果

本次重构完成后：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)
**全项目测试全部通过**，无破坏性变更。

---

## 提交记录

| 提交 hash | 说明 |
|----------|------|
| `77db3bb` | 完成基础架构搭建，编译零错误 |
| `db6ebe5` | 修复 trait 定义错误 |
| `f4cab62` | 第一次重构，统一注册中心 |
| `7199874` | 按协议分类型存储 |
| `b84f51e` | 简化重构，解决 dyn 兼容 |
| `d28af5a` | 修复导入错误 |
| `f8af4a7` | 基于 Rig 原生 ToolDyn 重构 |
| `5a90197` | 移动注册中心到 pkg，统一 pkg 初始化收口 |
| `0a08d61` | 修复 SQLite JSON 类型、UUID 解码、枚举解码问题，测试全过 |
| `eac393b` | 全链路改为 String ID，去掉 Uuid 强依赖，统一所有 DAO 导出 |
| `d29a8f1` | 完成 Agent 工具绑定架构，符合分层规范，测试全过 |
| `...` | ... |
| `6039c39` | 完成混合模式命名对齐：CoreTool trait + Tool 实体，完整重构，测试全过 |

---

## 2026-04-29 Tool Domain 层设计

### 新增目录结构

```
ai_orz/src/service/domain/tool/
├── mod.rs              # 模块定义、错误类型、单例
├── management.rs       # 工具管理子模块（CRUD、绑定解绑、启用禁用）
└── execution.rs        # 工具执行子模块（单次/批量执行）
```

### Tool Domain 层职责划分

#### 1. ToolManagement - 工具管理子模块

负责工具的全生命周期管理，作为上层调用的统一入口：

| 方法 | 职责 |
|------|------|
| `sync_builtin_tools()` | 同步所有内置工具定义到数据库 |
| `list_tools()` | 获取所有工具列表 |
| `list_agent_tools()` | 获取某个 Agent 绑定的所有工具 |
| `get_tool()` | 根据 ID 获取工具详细信息 |
| `enable_tool()` / `disable_tool()` | 启用/禁用工具 |
| `bind_to_agent()` / `unbind_from_agent()` | 工具与 Agent 绑定/解绑 |
| `get_agent_bound_tool_ids()` | 获取 Agent 绑定的工具 ID 列表 |

#### 2. ToolExecution - 工具执行子模块

负责 manual 模式下的工具调用执行，支持重试和批量执行：

| 方法 | 职责 |
|------|------|
| `call_tool()` | 执行单个工具，返回带追踪信息的结果 |
| `batch_call_tools()` | 批量执行多个工具（可并行） |

执行结果包含完整调用链路信息：
```rust
pub struct ToolExecutionResult {
    pub request_id: String,      // 调用请求ID，用于关联
    pub tool_id: String,         // 工具ID
    pub tool_name: String,       // 工具名称
    pub success: bool,           // 是否成功
    pub result: Option<String>,  // 结果JSON
    pub error: Option<String>,   // 错误信息
    pub duration_ms: u64,        // 耗时毫秒
    pub call_entry: ToolCallEntry, // 完整追踪条目
}
```
> 当前实现参考：[tool 相关模块](src/service/dal/tool.rs)

### 错误类型设计

```rust
pub enum ToolDomainError {
    ToolNotFound(String),        // 工具未找到
    ToolNotEnabled(String),      // 工具未启用
    ExecutionFailed(String),     // 执行失败
    ValidationFailed(String),    // 参数验证失败
    Internal(String),            // 内部错误
    Database(sqlx::Error),       // 数据库错误
}
```
> 当前实现参考：[tool 相关模块](src/service/dal/tool.rs)

### 分层调用关系

```
Handler 层
    ↓
ToolDomain
  ├─ ToolManagement → ToolDal → ToolDao
  └─ ToolExecution → ToolCallDao + ToolTracing
```

✅ 严格遵循分层规范，Domain 层编排 DAL，不直接操作 DAO

### 当前实现状态

实现进度已全部并入生产代码：
- 接口契约（所有 trait 定义 + 错误类型 + 单例模式）✅ 落地；见 §二 分发点速查表 ToolDomainImpl mod.rs
- ToolManagement 业务逻辑（调用 ToolDal）✅ 落地：`src/service/domain/tool/management.rs`
- ToolExecution 业务逻辑（调用 ToolCallDao）✅ 落地：`src/service/domain/tool/execution.rs`
- 单元测试覆盖率 ✅ DAO 层 98 个测试 100% 通过（`src/service/dao/tool/sqlite_test.rs`）+ Domain 8 个集成测试通过（`tests/integration/tool_crud_test.rs`）

## 🔄 消息驱动工具调用链路（2026-05-11 更新）

### 核心理念对齐

**工具调用本身就是消息**。不依赖 LLM 原生 Function Calling，采用自定义消息格式实现，所有工具调用过程均可追溯、可审计、可回放。

### 完整链路设计

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

### MessageType 枚举扩展（对齐 Project 设计）

```rust
pub enum MessageType {
    UserMessage = 0,      // 用户 → Agent
    AgentMessage = 1,     // Agent → 用户
    SystemMessage = 2,    // System → Agent
    ToolCallRequest = 3,  // Agent → System（工具调用请求）
    ToolCallResult = 4,   // System → Agent（工具执行结果）
    ConfirmRequest = 5,   // Agent → User（确认请求）
    ConfirmResponse = 6,  // User → Agent（确认回复）
}
```

### ToolCallRequest 消息格式

**消息体 JSON 结构（存储在 message.content 字段）：**

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

### ToolCallResult 消息格式

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

**失败场景：**
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

### 工具注册表设计

**ContextTool 统一接口：**

```rust
#[async_trait]
pub trait ContextTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;  // JSON Schema
    
    async fn call(
        &self, 
        ctx: RequestContext, 
        args: serde_json::Value
    ) -> Result<serde_json::Value, ToolError>;
}
```
> 当前实现参考：[tool 相关模块](src/service/dal/tool.rs)

**工具注册表单例：**

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

### 分层职责对齐（严格遵守）

| 层级 | 职责 | 模块 |
|------|------|------|
| **Handler** | HTTP 接口：工具列表查询、手动触发测试 | `handlers/tools/` |
| **Domain** | 工具注册、权限校验、工具执行编排 | `service/domain/tool/` |
| **DAL** | 工具元数据 CRUD、绑定关系管理 | `service/dal/tool/` |
| **DAO** | 工具表 SQL 操作、PO 转换 | `service/dao/tool/` |
| **Pkg** | 工具注册表、ContextTool Trait、工具实现 | `pkg/tool_registry/` |
| **Consumer** | System 消息消费、工具执行入口 | `consumer/message.rs` |

### 与现有混合模式的关系

| 模式 | 适用场景 | 实现方式 |
|------|----------|----------|
| **ControlMode::Auto** | 简单无状态工具（计算、格式化等） | `execute_auto` → `call_tool`，Awakening 显式循环中同步执行，结果作为 ChatMessage::Tool 追加到对话历史 |
| **ControlMode::Manual（sync）** | 轻量但有审计需求的工具 | `execute_manual` → `request_tool_call` internal 工具 → `call_tool`，同轮返回结果 |
| **ControlMode::Manual（async）** | 组织能力工具（创建任务/项目、分配 Agent 等） | `execute_manual` → `send_tool_call_message` internal 工具 → 消息驱动链路，可追溯、可审计、可控 |

**共存策略：**
- Auto 工具在 Awakening 显式循环中同步调用，不经过消息队列
- Manual 工具按 `dispatch_mode` 选择同步/异步：sync 直接调 `call_tool`，async 走完整消息链路
- LLM 通过 `ToolDescriptor` 看到所有可用工具，按 `control_mode` 在 Awakening 层分发执行

### 关键设计决策记录

| 决策 | 理由 | 影响 |
|------|------|------|
| 工具调用复用消息表 | 统一存储，天然支持追溯和回放 | 无需新增 tool_calls 表 |
| 工具执行放在 System 消费者 | 单一职责，Agent 只做决策不做执行 | 解耦决策与执行 |
| JSON 格式存储在 content | 灵活扩展，无需修改表结构 | 向后兼容 |
| 两种模式共存 | 平衡开发速度与可控性 | 渐进式迁移 |

---

## 内置工具机制简化与保护（2026-05-13 更新）

### 目标
简化 BuiltinToolFactory trait，移除冗余方法，同时为 Builtin 类型工具添加保护机制，防止用户通过 API 修改或删除内置工具。

### 核心变更

#### 1. 简化 BuiltinToolFactory trait
**之前：**
```rust
#[async_trait]
pub trait BuiltinToolFactory: Send + Sync + Debug {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn create(&self, po: ToolPo) -> Result<Box<dyn CoreTool>, AppError>;
}
```
> 当前实现：[tool_registry/builtin.rs::BuiltinToolFactory](src/pkg/tool_registry/builtin.rs#L19-L30)

**现在：**
```rust
#[async_trait]
pub trait BuiltinToolFactory: Send + Sync + Debug {
    fn create_po(&self) -> ToolPo;
    async fn create(&self, po: ToolPo) -> Result<Box<dyn CoreTool>, AppError>;
}
```
> 当前实现：[tool_registry/builtin.rs::BuiltinToolFactory](src/pkg/tool_registry/builtin.rs#L19-L30)

#### 2. 新增 ToolPo::fill_defaults_for_builtin()
为 Builtin 工具自动填充默认值：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

#### 3. 新增 Builtin 工具保护
在 ToolDao 层添加保护：
- `update_tool()`：检测 `ToolProtocol::Builtin`，返回错误
- `delete_tool()`：检测 `ToolProtocol::Builtin`，返回错误

**新增 DAO trait 方法：**
```rust
#[async_trait]
pub trait ToolDao: Send + Sync {
    // ... existing methods
    async fn delete_tool(&self, ctx: RequestContext, tool_id: &str) -> Result<(), AppError>;
}
```
> 当前实现参考：[tool 相关模块](src/service/dal/tool.rs)

#### 4. 简化 sync_builtin_tools_to_db()
**之前：**
- 手动设置 protocol、control_mode、version
- 手动构造 ToolPo

**现在：**
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

### 设计决策

| 决策 | 理由 |
|------|------|
| trait 只保留 create_po() 和 create() | 元数据方法冗余，所有信息都可以放在 create_po() 返回的 ToolPo 里 |
| ToolPo 负责填充默认值 | PO 自身知道默认值应该是什么，集中管理 |
| 保护放在 DAO 层 | 最底层，确保任何上层调用（DAL/Domain/Handler）都无法绕过保护 |
| 更新和删除都受保护 | 约定优于配置，内置工具应由代码维护，用户应扩展自己的工具 |

### 测试更新
- `test_update_tool`：改为使用 `ToolProtocol::Http`
- `test_delete_builtin_tool_protected`：新增，验证 Builtin 工具无法删除
- `test_update_builtin_tool_protected`：新增，验证 Builtin 工具无法更新

### 测试结果
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)
**全项目测试全部通过**。

---

## Tool 管理面 Handler（2026-06-04 更新）

### 目标

在既有 Finance Domain / Tool DAL 能力之上补齐用户管理面的 Tool CRUD、状态更新和 Agent 绑定接口。Handler 与用户 Action 一一对应，不引入通用 CRUD Handler 抽象；复用通过 `common/src/api/tool.rs` DTO、`ToolQuery` 查询参数和 Finance Domain 能力完成。

### 路由

所有 Tool 管理面路由统一挂在 Finance 管理域下：

> 相关实现细节见：[handlers/hr/tool + http_tool 模块](src/handlers/hr/tool/)

列表查询通过 query 参数表达筛选：

| 参数 | 说明 |
|------|------|
| `keyword` | 关键词过滤，复用 `ToolQuery.keyword` |
| `enabled_only` | 只返回启用工具 |
| `agent_id` | 查询指定 Agent 已绑定工具 |
| `limit` | 限制返回数量 |

### DTO 与敏感字段策略

新增 `common/src/api/tool.rs`，前后端共享 Tool 管理面 DTO：

- `CreateToolRequest`
- `ToolListQuery`
- `UpdateToolRequest`
- `UpdateToolStatusRequest`
- `BindToolToAgentRequest`
- `UnbindToolFromAgentRequest`
- `ToolListItem`
- `ToolDetail`

Tool 协议配置 `config` 可能包含 header、token、connection string 等敏感信息：

- 写入接口允许接收 `config`；
- 列表响应不返回 `config` 原文，仅返回 `has_config: bool` 表达是否存在配置；
- 详情响应可以返回脱敏后的 `config`：HTTP `headers` / `query` / `body` 中的值默认全部替换为 `[REDACTED]`（保留字段结构，不保留原值），URL 中的 userinfo 移除，URL query 的所有值统一替换为 `[REDACTED]`；
- `parameters_schema` 可以返回，因为它描述参数结构而不是运行密钥。

### Handler 职责边界

`src/handlers/finance/tool/` 按 action 拆文件：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

Handler 只做请求级编排：

1. 解析 Path / Query / Json DTO；
2. 从 `RequestContext` 补全当前用户；
3. 将 DTO 组装为 Entity 或 `ToolQuery`；
4. 调用 `domain().tool_provider_manage()`；
5. 将 `Tool` 转换为脱敏 Response DTO。

不做：

- 不直接调用 DAL / DAO；
- 不在 Handler 间互调；
- 不承载复杂状态规则；
- 不抽象通用 Handler 框架。

### 状态更新与内置工具保护

状态变更统一使用：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

请求体：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

实现规则：

- 不新增 `/enable`、`/disable` 路由；
- `enable_tool` / `disable_tool` 薄方法已从 Domain 移除；
- Handler 读取实体后调用 `Tool::transition_status`，再通过 `update_tool` 写回；
- `Builtin` Tool 由系统同步维护，管理面禁止创建、修改、删除内置工具。

### 管理面 Tool 实体拼装

运行面完整 Tool 需要 `ToolPo + CoreTool`；但 Http / Mcp 等管理面工具可能还没有运行时 `CoreTool` 实例。为避免管理面列表/详情被运行时注册中心阻塞，DAL 的 `query` 对非 Builtin 工具支持返回 `Tool::from_po_for_management(po)`：

- Builtin：仍要求注册中心存在运行实例，避免运行面工具不可用；
- 非 Builtin：允许作为管理面实体返回，便于 CRUD、状态、绑定管理；
- 运行面执行仍应使用完整可调用 Tool，不因管理面 fallback 改变执行语义。

### 验证

- 编译检查通过；
- 测试通过；
- Tool DAO / DAL / Domain 覆盖了删除、内置工具保护、状态迁移和列表查询相关测试。

---

## MCP Tool Runtime 规划补充（2026-06-23 更新）

详细设计见 `docs/archive/design-archive/mcp_tool_design.md`。核心结论：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

### 代码结构决策

- 新增 `mcp_servers` 表与 `McpServerDao`，`McpServerDao` 只负责持久化 CRUD；
- `ToolDal` 保持通用基础 DAL，不承载 MCP/HTTP 等协议专属膨胀逻辑；
- 第一版新增 `McpToolDal`，负责 MCP tools 同步、按 server 管理、读取 server config、组装带 MCP 依赖的可调用 `Tool` 实体；
- MCP client/session 生命周期不单独暴露成上层可见的 `McpClientDao`，而是内聚到 `McpToolCallDaoImpl`；
- `McpToolCallDaoImpl` 作为 `ToolCallDao` 的 MCP 协议增强实现，组合基础 `ToolCallDao`，大部分方法转发，主要扩展 `assemble_mcp_core_tool(po, server)`；
- MCP Tool 仍注册到 `tool_registry`，但不强行复用单一 `create_tool(po)`；第一版优先提供专用 `create_mcp_tool(po, deps)`，后续参数增多再演进为 builder 模式，由 `McpToolDal` 准备并传入 deps。

### MCP 调用链路

```text
ToolCallRequest(tool_id, args)
  ↓
Runtime/Finance Domain 识别 protocol=Mcp
  ↓
McpToolDal.call_by_tool_id(ctx, tool_id, args)
  ↓
ToolDao -> ToolPo(config={server_id, tool_name})
  ↓
McpServerDao -> McpServerPo / McpServerConfig
  ↓
McpToolCallDao.assemble_mcp_core_tool(po, server)
  ↓
registry::mcp::create_mcp_tool(po, deps) 或 McpToolBuilder -> McpCoreTool
  ↓
ToolCallDao.execute(ctx, tool, args)
  ↓
McpCoreTool.call -> McpClientRuntime -> rmcp tools/call
  ↓
ToolCallResult 写回消息链路
```

### 分层边界

```text
handler → domain → dal → dao
```

- Handler：只对应管理面 action；
- Domain：权限、安全策略、同步工具、跨 DAL 编排；
- DAL：通用 `ToolDal` 保持基础能力，MCP 专属逻辑进入 `McpToolDal`；
- DAO：`McpServerDao` 只做持久化；`McpToolCallDaoImpl` 作为 `ToolCallDao` 的协议增强实现，内聚 MCP client/session 生命周期；
- pkg：提供 rmcp transport、SDK-neutral 类型、脱敏、timeout 等基础实现，不直接承载业务编排。


---

## HTTP Tool Runtime 设计补充（2026-06-22 更新）

### 核心结论

HTTP 工具不设计为一个固定暴露给 Agent 的裸 `http_get` / `http_post` 内置工具，而设计为一套**通用 HTTP Tool Runtime**：

> 相关实现细节见：[handlers/hr/tool + http_tool 模块](src/handlers/hr/tool/)

用户通过管理页面创建具体 HTTP 工具，写入标准 `tools` 表记录：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

运行时根据 `ToolProtocol::Http` 动态构建 `HttpCoreTool` 并执行。

### ToolProtocol 与 ControlMode 正交

`ToolProtocol` 表达工具来源/协议，不决定调用方式：

| 字段值 | 含义 |
|---|---|
| `Builtin` | 代码内置工具，由内置工厂创建 |
| `Http` | HTTP 协议工具，由 `ToolPo.config` 驱动 |
| `Mcp` | MCP 协议工具；MCP Server 独立建模，具体工具通过 `server_id + tool_name` 绑定 |

`ControlMode` 表达谁来调用：

| 字段值 | 含义 |
|---|---|
| `Auto` | Awakening 显式循环中由 `execute_auto` 直接执行，结果作为 `ChatMessage::Tool` 追加到对话历史 |
| `Manual` | 通过 `execute_manual` 转发到 internal 工具（`request_tool_call` 同步 / `send_tool_call_message` 异步） |

因此：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

Awakening 循环中按 `control_mode` 分发的逻辑是：

> 相关实现细节见：[runtime/awakening.rs + tool_execution.rs](src/service/domain/runtime/awakening.rs)

### 代码组织

`HttpCoreTool` 直接放在工具中心，统一工具构建逻辑：

```text
src/pkg/tool_registry/http.rs
```

该模块负责：

- 定义 `HttpToolConfig`；
- 定义 `HttpCoreTool`；
- 为每次调用创建带 timeout、redirect policy、DNS pinning 的 `reqwest::Client`；
- 根据 `ToolPo.config` 创建 HTTP 类型 `CoreTool`；
- 执行模板渲染、安全校验、HTTP 请求、响应裁剪和脱敏。

`ToolCallDao` 不直接知道 HTTP 请求细节，只通过现有统一入口获取工具实例：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

`ToolRegistry.create_tool()` 根据 `ToolProtocol` 分发：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

HTTP Tool 是数据库注册、配置驱动的协议工具，因此不使用 `HashMap<tool_id, factory>`。`ToolRegistry` 持有一个协议级 `HttpToolFactory`，默认实现为 `DefaultHttpToolFactory`，内部再构造 `HttpCoreTool`。这样 registry 不直接依赖包级别 `http::create_tool(po)` 函数，后续可以通过依赖注入替换 HTTP runtime、client、resolver 或测试替身。

### HTTP Tool 执行链路

```text
用户页面创建 HTTP Tool
  ↓
Finance Tool 管理面
  ↓
ToolDal.create_tool()
  ↓
ToolDao 写入 tools 表
  ↓
Agent Prompt 展示该 Manual Tool
  ↓
LLM 输出 ToolCallRequest(tool_id, args)
  ↓
Message Consumer 识别 ToolCallRequest
  ↓
ToolDal.call_tool_by_id()
  ↓
ToolCallDao.assemble_core_tool()
  ↓
ToolRegistry.create_tool(po)
  ↓
ToolProtocol::Http → HttpToolFactory.create(po) → HttpCoreTool
  ↓
ToolCallDao.execute()
  ↓
HttpCoreTool.call(ctx, args)
  ↓
ToolCallResult 写回消息链路
```

### 设计约束

- Agent 不直接获得裸 `http_get(url, headers)` 能力；
- URL、method、headers、query、body 模板由 `HttpToolConfig` 固定；
- Agent 只能填写 `parameters_schema` 定义的业务参数；
- HTTP Tool 第一版默认 `ControlMode::Manual`；
- SSRF 防护、timeout、response size limit、redirect 策略、敏感 header 脱敏必须内置到 HTTP Runtime；
- 本地/私网 HTTP Tool 采用默认拒绝 + 显式授权：`blocked_domains` 优先拒绝，只有配置 `allow_local_network=true` 才允许访问 localhost/私网/link-local 目标；运行时还会在发请求前解析域名，任一解析 IP 命中本地/私网/metadata/保留网段等非公网风险地址时默认拒绝，并将校验后的地址 pin 到本次请求、禁用代理，避免校验与实际连接之间发生 DNS rebinding；域名匹配前会统一去尾点，避免 `example.com.` 绕过白/黑名单；
- HTTP Runtime 会在请求前做轻量参数 schema 校验（required、基础类型、enum、additionalProperties=false）并拒绝未解析或暂未支持的 `{{...}}` 模板占位符；
- HTTP Runtime 默认不跟随重定向（`redirect::Policy::none()`），避免初始 URL 合法但 3xx 跳转到 localhost/私网/metadata 风险地址；3xx 响应按普通响应进入 `allowed_status_codes` 校验；
- 管理面继续遵循 `config` 脱敏策略：写入可接收，列表不返回原文仅返回 `has_config`，详情可返回脱敏后的 `config`；HTTP 详情 config 对 `headers` / `query` / `body` 值默认全量脱敏，仅保留字段结构，URL userinfo 移除且 URL query 所有值脱敏；create/update 在持久化前校验 HTTP config 并强制第一版 Manual-only，固定目标若命中 localhost/私网/特殊地址、`blocked_domains`，或不满足 `allowed_domains`，会在写入前拒绝；运行时对外错误不包含渲染后的 URL、header、query/body 值或密钥；HTTP Tool 调用追踪日志中 input/output/error 均以 `[REDACTED]` 记录。

详细方案见：`docs/archive/design-archive/builtins_http_tool_design.md`。

---

## 提交记录

| 提交 hash | 说明 |
|----------|------|
| `77db3bb` | 完成基础架构搭建，编译零错误 |
| `db6ebe5` | 修复 trait 定义错误 |
| `f4cab62` | 第一次重构，统一注册中心 |
| `7199874` | 按协议分类型存储 |
| `b84f51e` | 简化重构，解决 dyn 兼容 |
| `d28af5a` | 修复导入错误 |
| `f8af4a7` | 基于 Rig 原生 ToolDyn 重构 |
| `5a90197` | 移动注册中心到 pkg，统一 pkg 初始化收口 |
| `0a08d61` | 修复 SQLite JSON 类型、UUID 解码、枚举解码问题，测试全过 |
| `eac393b` | 全链路改为 String ID，去掉 Uuid 强依赖，统一所有 DAO 导出 |
| `d29a8f1` | 完成 Agent 工具绑定架构，符合分层规范，测试全过 |
| `...` | ... |
  1246|| `6039c39` | 完成混合模式命名对齐：CoreTool trait + Tool 实体，完整重构，测试全过 |
  1247|| `bc41fd8` | 修复 HR 测试缺失 DAO 初始化问题 |
  1248|| `05ef2f0` | 简化内置工具机制并添加 Builtin 保护 |
  1249|| `[NEW]` | 新增三个通用内置工具 `http_fetch`/`fs_read`/`fs_write`，完整安全防护，测试全过 |
  1250|---

## 通用内置工具（2026-06-28 新增）

### 目标
提供三个开箱即用的通用内置工具，满足 Agent 自主处理文件和网络请求的需求：
- `http_fetch` - 获取 HTTPS 网页/API 内容
- `fs_read` - 读取本地文件（支持范围读取/grep搜索）
- `fs_write` - 修改本地文件（支持多种编辑模式，原子操作）

所有工具都启用 `ControlMode::Auto`，在 Awakening 显式循环中由 `execute_auto` 同步调用。

### 安全设计

**HTTP 安全（SSRF 防护）**：
- 强制仅允许 HTTPS 方案，拒绝 HTTP
- 默认拒绝本地网络/私有 IP 访问（需要显式配置 `allow_local_network=true` 开启）
- 支持域名白名单/黑名单
- DNS 解析结果进行二次校验，DNS pinning 防止劫持
- 默认响应大小限制 1MB，硬限制 10MB，防止 OOM

**文件系统安全（沙箱隔离）**：
- 默认仅允许访问 `base_data_path` 范围内的文件
- 支持通过 `additional_allowed_paths` 配置额外允许路径
- 敏感文件名直接拒绝（`.env`, `.pem`, `.key`, `.p12`, `id_rsa`, `password`, `secret` 等）
- 拒绝符号链接，防止 `..` 路径穿越攻击
- 路径规范化解析，杜绝绕过检测

### 功能说明

#### 1. `http_fetch` (ID: `http_fetch`, name: `fetch_url`)

**参数**：
> 相关实现细节见：[handlers/hr/tool + http_tool 模块](src/handlers/hr/tool/)

**返回**：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

**测试覆盖**：4 个单元测试全部通过。

---

#### 2. `fs_read` (ID: `fs_read`, name: `read_file`)

**参数**：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

**配置** (`ToolPo.config`)：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

**返回**：
- 正常读取：返回带行号的内容 + 元信息
- grep 模式：返回匹配列表 + 上下文
- 需要确认：`{success: false, require_confirmation: true, message: "..."}`

---

#### 3. `fs_write` (ID: `fs_write`, name: `write_file`)

**参数**：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

**支持的模式**：
| 模式 | 说明 |
|------|------|
| `overwrite` | 完全覆盖文件（不存在则创建） |
| `append` | 追加内容到文件末尾 |
| `insert_after` | 在指定行后插入新内容 |
| `delete_range` | 删除 `[start_line, end_line]` 范围内的行 |
| `replace_range` | 将 `[start_line, end_line]` 替换为新内容 |

**原子性保证**：
- 先读取现有文件到内存，在内存完成修改，最后一次性写入磁盘
- 要么完全成功，要么文件不改变，避免部分写入损坏文件

**配置**：同 `fs_read`，支持 `additional_allowed_paths` 扩展允许路径。

### 目录结构

```diff
ai_orz/src/pkg/tool_registry/
+├── tool_security.rs       # 通用安全工具函数（SSRF + 文件沙箱）
+├── http_fetch.rs         # http_fetch 工具实现 + 单元测试
+├── fs_read.rs            # fs_read 工具实现
+├── fs_write.rs           # fs_write 工具实现 + 单元测试
+├── fs_tests.rs           # 文件安全模块单元测试
```

### 测试结果

所有相关测试全部通过：
> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

包括：
- http_fetch 安全测试（拒绝 HTTP/localhost/私有IP）
- 文件安全测试（敏感文件检测、路径验证）
- fs_write 参数校验单元测试

### 自定义扩展路径

管理员可以在数据库中修改 `fs_read` / `fs_write` 工具的 `config` JSON，添加 `additional_allowed_paths` 来允许访问项目外的指定路径：

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

所有安全检查（敏感文件过滤、符号链接检查）仍然生效。

---

## `shell_exec` 异步 Shell 执行工具（设计中）

### 目标
提供 Agent 异步执行 Shell 命令的能力，支持短命令同步等待和长命令后台运行，完整的输出日志存储和进程追踪，满足项目构建、脚本执行、系统任务等需求。

### 核心设计决策

#### 执行模型
- **协议/模式**：`protocol = Builtin`，`control_mode = Manual`，`dispatch_mode = async`
- 不走 `execute_auto` 同步调用，通过 `execute_manual` → `send_tool_call_message` internal 工具走自建 `ToolCallRequest` / `ToolCallResult` 消息链路异步执行
- 消费者负责实际执行，完成后唤醒 Agent

#### 存储结构（完全遵循现有约定）

| 内容 | 路径 | 获取方法 |
|------|------|----------|
| 调用追踪元信息 | `{base_data_path}/tools/shell_exec/call_trace/{YYYYMMDD}.jsonl` | `config.tool_call_trace_dir("shell_exec")` |
| 命令输出日志 | `{base_data_path}/tools/shell_exec/logs/{call_id}.log` | `config.tool_logs_dir("shell_exec")` |

元信息（`pid`, `background`, `exit_code`, `started_at`, `finished_at`）存储在 `ToolCallEntry.metadata`，不单独重复存储。

#### 配置结构（`ToolPo.config`）

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

- `additional_allowed_paths`：额外允许的工作目录，默认只允许 `base_data_path` 内
- `allowed_env`：允许从进程环境继承的环境变量白名单
- `default_timeout_ms`：默认超时（5 分钟），Agent 单次调用可覆盖
- `default_max_output_size_bytes`：默认最大输出（10MB），超过截断

#### 调用参数（Agent 发起请求）

> 相关实现细节见：[tool 模块](src/pkg/tool_registry/)

#### 安全设计

1. **工作目录限制**：
   - 工作目录必须在 `base_data_path` 或 `additional_allowed_paths` 范围内
   - 复用 `tool_security.rs` 的路径校验逻辑（敏感文件检测、符号链接拒绝）

2. **环境变量过滤**：
   - 只继承白名单 `allowed_env` 中的环境变量
   - Agent 传入的额外 `env` 全部允许
   - 自动过滤 `HOME`, `USER`, `SSH_*`, `GITHUB_*` 等敏感变量

3. **不做命令语法分析**：假设调用由 Agent 发起，Agent 已做决策，安全由目录沙箱保证

#### 执行流程

```
Agent 发起 ToolCallRequest(shell_exec)
  ↓
ToolCallLogger 记录 ToolCallEntry (status=Started) → call_trace JSONL
  ↓
Message Consumer 处理
  ↓
1. 校验工作目录是否允许（复用 tool_security）
2. 组装环境变量（过滤继承 + 追加 Agent 传入）
3. 启动 shell 子进程，拿到 pid
4. 更新 ToolCallEntry.metadata = { pid, background, started_at, log_path }
5. 异步读取 stdout + stderr → 持续追加到 .../logs/{call_id}.log
  ↓
┌─────────────────────────┐
│ 短命令 (background=false)│
└─────────────────────────┘
  ↓
  等待子进程退出
  ↓
  获取 exit_code + finished_at
  ↓
  更新 ToolCallEntry.metadata + status=Completed
  ↓
  创建附件（日志文件）
  ↓
  ToolCallResult 返回摘要：
  {
    "success": true,
    "exit_code": 0,
    "duration_ms": 1234,
    "log_size_bytes": 12450,
    "log_lines": 256,
    "attachment_id": "...",
    "truncated": false,
    "call_id": "..."
  }
  ↓
  唤醒 Agent

┌─────────────────────────┐
│ 长命令 (background=true) │
└─────────────────────────┘
  ↓
  启动后立即返回，不等待退出
  ↓
  更新 ToolCallEntry status=Running
  ↓
  ToolCallResult 返回：
  {
    "success": true,
    "background": true,
    "pid": 12345,
    "started_at": 1234567890,
    "log_attachment_id": "...",
    "call_id": "..."
  }
  ↓
  唤醒 Agent（Agent 可后续读取日志）
  ↓
  子进程输出继续追加到日志文件
```

#### 长进程状态查询

Agent 可通过读取 `ToolCallEntry` 获取 `metadata.pid`，然后查询进程是否运行：

- **Linux/macOS**：检查 `/proc/{pid}` 是否存在
- 状态变化后更新 `ToolCallEntry.metadata.exit_code` + `finished_at`

后续可扩展 `shell_status` / `shell_kill` 辅助工具方便 Agent 操作。

#### 目录结构

```diff
ai_orz/common/src/config.rs
+├── tool_logs_dir() method - 获取工具日志目录

ai_orz/src/pkg/tool_registry/
+├── shell_exec.rs      # shell_exec 工具实现
```

复用现有：
- `tool_security.rs` → 路径安全校验
- `ToolCallLogger` → 调用追踪
- `attachment` → 附件关联

---

## 统一后台进程管理与 shell_exec 超时移交（2026-08-09 更新）

### 同步/异步定调

- **同步是默认**：工具调用默认同步等待结果返回给 Agent，这是最自然的交互模型
- `dispatch_mode=async` 消息链路仅留给显式配置的重型工具，不是常规路径
- **Agent 保留调用级决策空间**：通过 `shell_exec` 的 `background` 参数自主决定是否后台执行（轮询式异步：先拿 pid，再用 `shell_status` 轮询）；配置级 `dispatch_mode` 与调用级 `background` 不冲突，前者是工具元数据，后者是单次调用意图

### call_id 单一事实源与全链路关联

现状问题：`call_id` 在 `ToolCallDao::execute` 内部生成，`CoreTool::call(ctx, args)` 拿不到它；shell_exec 日志只能用请求级 `log_id` 命名（同一请求多次调用混在一个日志文件）。

方案：call_id 升级为业务可指定的幂等键，单点收口在执行层：

1. `RequestContext` 新增可选字段 `tool_call_id`（builder + getter，默认 None 不影响现有构造）
2. `ToolCallDao::execute` 取 id 顺序：`ctx.tool_call_id()` 有值（业务指定）→ 直接复用；无值 → 生成新 UUID v7 并通过 `ctx.to_builder().tool_call_id(call_id).build()` 注入后再调 `CoreTool::call`
3. 消费端规则：所有需要关联 id 的工具一律优先取 `ctx.tool_call_id()`；仅当 ctx 未注入（测试直接调 CoreTool）才回退 `ctx.log_id`

关联链路：`ToolCallEntry.call_id`（JSONL）↔ 日志文件名 `{call_id}.log` ↔ `ProcessEntry.call_id + pid` ↔ 工具返回 JSON 的 call_id/pid，任一端均可反查其余。

### 幂等防重

`ToolCallDao::execute` 入口处仅当 call_id 为**业务指定**时（自动生成不查，新 UUID 永不命中，避免每次调用多一次 JSONL 扫描）：

- 调 `ToolCallLogger::read_call_by_id` 查历史（限定 tool 目录扫描）
- 命中且 `status=Completed` → 直接返回历史 output 与历史 entry（entry.metadata 标 `deduplicated=true`），不重复执行
- 命中且 `status=Failed` → 允许重试，正常执行（失败不该永久钉死）
- 未命中 → 正常执行

### pkg/process 进程注册中心（纯基础设施）

`src/pkg/process/mod.rs`：

- `ProcessEntry { pid, tool_id, call_id, agent_id, project_id, task_id, command, working_dir, log_path, background, started_at, status(Running/Exited), exit_code, finished_at }`
- `ProcessRegistry` 全局单例（once_cell + `Mutex<HashMap<u32, ProcessEntry>>`，pid 为键）：`register / get / list / mark_exited / refresh / remove` + `tail_log(path, n)`
- 进程原语（`#[cfg(unix)]` libc；非 unix 桩）：`is_alive(pid)` = kill(pid, 0)；`terminate(pid)` = SIGKILL
- 内存版，服务重启条目丢失可接受（审计线索保留在 ToolCallEntry JSONL）；pid 复用风险由 entry.started_at 供人工甄别

### SystemDomain ProcessManager（领域层）

`src/service/domain/system/process.rs`：

- trait `ProcessManager`：`get_process / list_processes / kill_process / process_status`（同步方法，注册中心为内存结构）
- **Agent scope 规则**：`ctx.agent_id()` 为 Some 时必须与 entry.agent_id 匹配（Agent 只能管理自己启动的进程，不匹配返回 `PermissionDenied`）；ctx 无 agent_id（人类用户/管理面）放行；list 同样按 agent 过滤
- kill 走 `terminate` 原语后 `mark_exited`；status 先 `refresh` 探活再返回 entry + 日志尾部（默认 20 行，上限 500）

### shell_exec 统一日志流式模型 + 超时 detach

- **统一执行模型**：sync 与 background 都从 spawn 起把 stdout/stderr 重定向到日志文件 `{call_id}.log`（取代原 sync 管道捕获的双套逻辑）；sync 等待结束后从日志文件读取输出做摘要（受 `max_output_size_bytes` 截断），全量留盘
- **超时语义改为 detach**：超时不再 kill，返回 `{ status: "timeout", call_id, pid, log_path, message }`，进程继续运行，Agent 可用 shell_status 查询或 shell_kill 终止；新增可选参数 `timeout_action: "detach" | "kill"`（默认 detach，保留显式 kill 能力）
- **进程注册**：spawn 成功后（sync/background 均注册）写入 ProcessRegistry，携带 ctx 的 agent/project/task/call_id；退出时 `mark_exited(exit_code)`
- 返回 JSON 统一携带 `call_id` 与 `pid`；tags 增加 `"shell"` 供分组绑定

### shell_status / shell_kill 双露工具

- `#[register_handler_tool]` + `#[generate_http_handler]` 宏双露（HTTP + LLM 工具，复用 `request_tool_call` 既有模式）：
  - `shell_status(pid, tail_lines?)` → `{ pid, alive, exit_code, started_at, command, log_path, call_id, log_tail }`
  - `shell_kill(pid)` → `{ pid, killed }`
- HTTP 路由：`GET /api/v1/system/processes/{pid}`、`POST /api/v1/system/processes/{pid}/kill`
- 三个 shell 工具（shell_exec/shell_status/shell_kill）以 tag `"shell"` 分组绑定

### 测试更新

新增 18 个测试：pkg/process 注册中心 6 个（含真实 spawn 探活/终止）、ProcessManager scope 校验 5 个（agent 不匹配拒绝/匹配放行/用户 ctx 放行/list 过滤/真实 kill）、shell_exec 真实子进程 4 个（超时 detach 存活/超时 kill 终止/background 注册/sync 完成读日志）、call_id 全链路关联与幂等防重 3 个（Completed 返回历史/Failed 允许重试）。

---

## 前端体验闭环：HTTP 工具表单 + 进程管理页面 + 工具调用 Tab（2026-08-09 更新）

### shell_list 双露工具（后端）

- 进程列表双露为 `shell_list` LLM 工具 + HTTP 接口，与 shell_status/shell_kill 凑齐三件套（tag `"shell"`）：`#[register_handler_tool]` + `#[generate_http_handler]`，内部调 `ProcessManager::list_processes(ctx)`（复用 Agent scope 过滤）并逐条 `registry().refresh(pid)` 探活后转 `ProcessInfo`
- DTO：`ListProcessesRequest`（空参，scope 由 RequestContext 决定）/ `ProcessInfo { pid, call_id, tool_id, agent_id, command, working_dir, background, started_at, alive, exit_code, log_path }` / `ListProcessesResponse`
- HTTP 路由：`GET /api/v1/system/processes`（排在 `/processes/{pid}` 前避免路由遮蔽）

### HTTP 工具创建表单（前端）

- 金融工具页（tools.rs）页头「+ 创建 HTTP 工具」按钮 → Modal 表单，字段对齐 `CreateToolRequest` + `HttpToolConfig`：name/description/tags + method（仅 GET/POST 下拉）/url 模板/headers/query/body JSON 文本域/timeout_ms/response_max_bytes/allowed_status_codes/response_json_pointer/allowed_domains/blocked_domains/allow_local_network + parameters_schema
- 提交调现有 `create_tool` API，protocol 固定 `Http`；校验逻辑抽为纯函数（`build_create_request` 等，必填/方法白名单/JSON 解析），行内错误提示；只建创建入口，不含编辑已有工具

### 后台进程管理页面（前端）

- `api/system.rs`：`list_processes` / `get_process_status(pid, tail_lines)` / `kill_process(pid)`
- 共享组件 `ProcessDetailContent { pid }`：懒加载 shell_status，展示全字段 + log_tail + 手动刷新 + 带确认的终止按钮；**详情不建独立路由页**，以 Modal 形态在列表页与聊天侧栏两处复用
- 列表页 `/system/processes`：状态徽标（Running 绿/Exited 灰）+ 后台标记 + 退出码 + call_id 截断展示；自动刷新复选框（默认关，开启后 5s 轮询）+ 手动刷新；行内终止与详情弹窗

### ChatSidePanel 工具调用 Tab（前端）

- 两种对话模式均新增「工具」Tab（项目模式：总览/任务/产物/Agent/工具；默认模式：Agent/我/工具）：项目对话按 project_id 查，默认对话按前台 agent_id 查，limit 30
- 数据：并行 `query_tool_call_entries` + `list_processes`，前端按 `entry.call_id == process.call_id` join 出关联进程（**仅 Running 显示** PID 徽标）；点击 PID 徽标弹 Modal 复用 `ProcessDetailContent`
- 行内容：工具名 + 状态徽标（执行中/已完成/失败）+ 耗时 + 启动时间；行展开看 input/output 摘要（JSON 截断 300 字符）与错误信息
- 刷新链路：挂入侧栏既有 `refresh_tick` SSE 防抖（2s）+ 手动刷新计数叠加下发；进程终止等变更后局部重拉 join 数据
- 数据依赖 JSONL 扫描查询，limit 30 控制开销；进程列表为内存态，服务重启后为空属预期

### 测试更新

新增 17 个测试：shell_list handler 2 个（用户 ctx 全量/agent ctx 仅见自己）、HTTP 工具表单校验 6 个（必填/方法白名单/JSON 解析/数字解析/逗号列表）、进程页面纯函数 4 个（命令截断 3 + 状态徽标 1）、工具调用 Tab 纯函数 5 个（状态徽标/耗时格式/JSON 截断/call_id join，含边界）。

---

## 通用内置工具补全 tag 分组（2026-08-09 更新）

为通用内置工具在**代码层**补齐 tag，使其能通过工具包机制（`install_tag` / `installed_tags`）按组安装；tag 定义视为稳定约定，后续一般不再修改。

### tag 分组

| 工具 | tag | 说明 |
|------|-----|------|
| `http_fetch` | `http` | 通用 HTTPS 抓取（SSRF 防护、拒本地网络） |
| `fs_read` / `fs_write` | `fs` | 本地文件读写（base_data_path 沙箱 + `additional_allowed_paths` 扩展） |
| `shell_exec` | `shell` | 原有 tag 不变，与双露的 shell_status/shell_kill/shell_list 同组 |

### 存量记录的升级策略：所有权分界刷新（无需版本字段）

内置工具的 DB 记录按字段划分所有权，`sync_builtin_tools_to_db` 对存量记录执行分字段刷新：

| 所有权 | 字段 | sync 行为 |
|--------|------|-----------|
| **代码所有权** | name / description / control_mode / parameters_schema / tags | 以代码定义为准无条件刷新（代码即最新版，无需版本比较） |
| **运维所有权** | config（如 fs 的 `additional_allowed_paths`）/ status | 永不覆盖，保留现场设置 |

配套约束：
- 字段无变化时不写库，避免启动时空刷 UPDATE
- 仅对 `protocol = Builtin` 的记录刷新，不碰用户自建工具
- API 层面内置工具的 update/delete 本就受保护拦截，代码所有权字段不存在用户修改源，因此无条件覆盖安全
- 这样版本升级后，老环境启动一次即自动获得最新的描述/schema/tags；新环境首次同步同路

### 触发点：启动链路 + initialize_system 兜底

- **启动链路（每次启动）**：`service::init_base_data()` → `domain::init_all_base_data()` → `finance::init_base_data()` 调 `sync_builtin_tools`，保证版本升级后内置工具定义自动对齐代码；失败仅 warn 不阻塞启动
- **initialize_system（首次初始化）**：同一 sync 再跑一次，幂等无冲突，作为兜底
- 与 system domain 的 cron triggers 注入并列，同属两阶段初始化的异步第二阶段

### 绑定语义（不变）

补 tag 只解决「可按组安装」的能力，**不改变默认可见性**：Agent 仍需绑定工具或在入职/配置时 `install_tag("http"/"fs"/"shell")` 才能调用；`neural` 免绑定通道不受影响。

### 测试更新

新增 2 个测试：`generic_builtin_tools_carry_expected_tags`（registry 层断言四个通用工具 tag 定义）、`test_sync_builtin_tools_refreshes_code_owned_fields`（DAL 层断言代码字段被刷新且 config/status 保留）。

---

## 五、扩展模式

### 场景 1：新增一种 ToolProtocol（如 WebSocket SSE 工具）

如果未来需要新增流式长连接类工具协议（SSE/WebSocket），可参考现有的 Builtin/Http/Mcp 分支：

1. 在 [enums/tool.rs::ToolProtocol](common/src/enums/tool.rs#L14-L30) 新增变体，同步补充 `ToolPo.protocol` 存储层的枚举映射
2. 在 [pkg/tool_registry/mod.rs::ToolRegistry](src/pkg/tool_registry/mod.rs#L49-L80) 新增 typed storage 字段，并在 `create_tool` / `call_tool` 分发处加分支
3. 在 [dal/tool.rs::ToolDal](src/service/dal/tool.rs#L78-L150) 的协议路由逻辑中，仿照 McpToolDal 委托路径，新增对应 DAL 或复用现有 ToolDal + 新增 handler 工具

### 场景 2：为通用工具新增一个 tag 分组（如 "db" 数据库操作工具组）

若未来需要新增 `db_query` / `db_migrate` 等通用内置工具并按组绑定：

1. 参考四个通用工具的 tag 注册模式，在 [pkg/tool_registry/builtin.rs](src/pkg/tool_registry/builtin.rs) 的对应 `#[register_handler_tool]` 宏上加 `tags = "db"`
2. 复用现有的 `sync_builtin_tools_refreshes_code_owned_fields` 逻辑（代码所有权字段无条件刷新），启动时 DB 记录的 tags 自动同步
3. Agent 入职或配置页调用 `install_tag("db")` 即可整组安装，无需逐个绑定，免绑定校验三层逻辑在 [runtime/tool_execution.rs](src/service/domain/runtime/tool_execution.rs) 的绑定检查处已支持 tag 交集判定

---