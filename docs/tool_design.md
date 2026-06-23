# ai_orz 工具模块设计与开发总结

## 开发时间线（2026-04-17）

### 目标
基于 Rig 框架，设计并实现工具模块基础架构，支持多种协议（builtin/http/mcp），符合项目现有代码规范。

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
│   │       └── mcp.rs           # MCP 工具（占位）
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
| `pkg/tool_registry` | **全局工具实例注册中心**，独立于 DAO，职责单一：<br>- 按协议分类存储工具实例<br>- 提供注册和查询接口<br>- 内置工具实现 `BuiltinTool` trait，继承 Rig 原生 `ToolDyn` |
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

### 3. Rig dyn 兼容方案
- Rig 原生 `Tool` trait 因为 async 方法自带 `Sized` 约束，不支持 dyn
- 解决方案：Rig 已经提供原生 dyn 兼容 trait `ToolDyn`，直接使用即可，无需自行封装
- 实现：`BuiltinTool` trait 继承 `ToolDyn + DynClone`，添加 `id()` 和 `description()` 两个元数据方法

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
| Rig `Tool` trait 不支持 `Box<dyn Tool>` | async 方法默认有 `Sized` 约束 | 使用 Rig 原生 `ToolDyn` trait，已经解决 dyn 兼容 |
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

```
test result: ok. 117 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;
```

---

---

## Agent 工具绑定架构（2026-04-18 更新）

### 目标
将已存储的工具绑定到 Agent，在创建 Cortex 时将工具实例传入 Rig Agent，支持 Agent 调用工具。严格遵循项目分层规范：`handler → domain → dal → dao`，禁止同层互调。

### 更新后的架构

#### 目录结构变化
```diff
 ai_orz/src/
 ├── models/
 │   └── tool.rs              # + Tool 复合实体 (ToolPo + Box<dyn ToolDyn + Send + Sync>)
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
   - 对每个 ToolPo，从 GLOBAL_TOOL_REGISTRY 查找已注册的 Box<dyn ToolDyn>
   - 拼装成 Tool { po: tool_po, tool: boxed_dyn }
   - 自动过滤未在注册中心找到的工具（已删除/未实现）
   ↓
5. AgentDal 用 Agent::from_po_with_tools(agent_po, tools) 返回完整 Agent
```

### 核心设计决策

| 问题 | 方案 | 原因 |
|------|------|------|
| **谁来拼装完整 Tool？** | ToolDao 负责 | DAO 只负责自己领域的对象拼装，符合单一职责 |
| **Tool 应该包含什么？** | `Tool { po: ToolPo, tool: Box<dyn ToolDyn + Send + Sync> }` | 分离元数据（PO）和运行实例（dyn），满足 Rig 需要直接获取 dyn 的要求 |
| **get_agent_with_tools 放哪层？** | AgentDal 层 | Dal 职责就是组合多个 DAO 构建完整业务实体，不违反分层规则 |
| **CortexDao 接收什么？** | `Vec<Tool>` 而非 `Vec<ToolPo>` | ToolDao 已经拼装好了，CortexDao 只需要提取 dyn 传给 Rig，职责清晰 |
| **工具存在哪里？** | Agent 实体持有 `Vec<Tool>` |领域概念：工具属于 Agent，Brain/Cortex 只在构建时使用不存储 |

### Rig 0.35 适配说明

rig-core 0.35 有重大不兼容变更：
- **之前**：可以增量 `agent.tool(...)` 添加工具
- **现在**：必须一次性 `agent.tools(tool_set)` 传入所有工具，ToolSet 需要从 `Vec<Box<dyn ToolDyn>>` 创建
- **适配方案**：从 `Vec<Tool>` 提取 `Box<dyn ToolDyn + Send + Sync>`，通过 `unsafe std::mem::transmute` 转换为 `Box<dyn ToolDyn>`
- **安全性**：所有注册工具都保证实现 `Send + Sync`，Cortex 本身需要 `Send + Sync`，因此 transmute 是安全的，代码已添加 `// SAFETY:` 注释说明

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
```
test result: ok. 119 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;
```

---

## 工具调用自动追踪（2026-04-20 ~ 2026-04-21 更新）

### 目标
为 Rig Agent 调用的所有内置工具自动添加完整调用日志追踪，记录完整的输入输出、调用参数、错误信息，方便调试、审计和后续训练数据收集。保持非侵入式设计，不修改 Rig 原生接口，方便后续扩展。

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
+│       ├── decorator.rs     # LoggingToolDecorator - 装饰器包装 ToolDyn
+│       ├── mod.rs           # 模块导出
+│       └── logger_test.rs   # 完整单元测试
 └── service/
     └── dao/tool/
         └── sqlite.rs       # 更新：拼装 Tool 实体时自动添加装饰器包装
```

#### 工作流程图
```
应用启动
  ↓
ToolCallLogger::init(base_data_path) → 全局单例初始化完成
  ↓
ToolDao.get_tool_full / list_tools_for_agent_full
  ↓
找到 ToolPo + 从注册中心获取 Box<dyn ToolDyn>
  ↓
自动包装：LoggingToolDecorator::new(original_tool, tool_id, tool_name)
  ↓
返回拼装好的 Tool { po, tool: Box::new(decorator) }
  ↓
Cortex 提取 Box<dyn ToolDyn> 传给 Rig Agent
  ↓
Rig Agent 需要调用工具
  ↓
LoggingToolDecorator.call(...)
    → 调用原始 tool.call(...) 得到结果
    → 自动构造 ToolCallEntry 包含完整上下文
    → ToolCallLogger::get().log_call() → 写入 daily JSONL 文件
    → 返回结果给 Rig
```

### 存储结构

日志文件按工具+日期分文件存储，路径格式：
```
{base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl
```

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

### 核心设计决策

| 问题 | 方案 | 原因 |
|------|------|------|
| **在哪里添加日志包装？** | ToolDao 拼装时 | ToolDao 已经负责拼装完整 Tool 实体，在此添加装饰器最自然，上层不需要感知 |
| **日志配置放在哪里？** | ToolCallLogger 从 config singleton 获取 | 配置已经是全局单例，不需要通过 DAO 传递参数，减少 API 污染 |
| **全局还是每个工具一个实例？** | 全局单例工厂 | base path 只需要初始化一次，每个调用按需获取 writer，没有重复创建开销 |
| **是否支持测试？** | 保留 `new()` 构造方法 | 测试可以创建本地实例用临时目录，不影响全局单例 |
| **什么时候写入日志？** | 调用完成后写入一次 | 只需要最终结果，不需要启动时写一条，简化设计；Started 状态保留给未来自调度工具 |
| **是否侵入原有代码？** | 装饰器模式 | 完全不修改 Rig 原生 `ToolDyn` 接口，符合开闭原则 |

### 设计符合项目分层规范

✅ **严格单向逐层调用**：没有新增跨层调用  \n✅ **职责单一清晰**：日志追踪是独立横切关注点，装饰器模式完美分离  \n✅ **配置不依赖注入**：配置已经是全局单例，`ToolCallLogger` 直接获取符合约定  \n✅ **单元测试完整覆盖**：5 个单元测试全部通过  \n\n### 单元测试

新增 5 个单元测试覆盖核心功能：
1. `test_tool_call_logger_basic` - 基础日志读写测试
2. `test_tool_call_logger_multiple_calls` - 多次调用按行追加测试
3. `test_tool_call_logger_different_tools_separate_paths` - 不同工具分开目录存储测试
4. `test_tool_call_failed_entry` - 失败调用记录错误信息测试
5. `test_tool_call_with_context_ids` - 关联 Agent/Task/Project ID 测试

**全部 5 个测试通过**

---

## 后续待扩展

1. **添加第一个内置工具**：现在基础架构已完成，可以开始实现具体工具
2. **HTTP 协议工具支持**：目前是占位结构，待实现
3. **MCP 协议工具支持**：目前是占位结构，待实现
4. **ToolEmbedding 语义自动选择**：基于 embedding 做工具相关性排序，减少上下文
5. **运行时动态加载工具**：从数据库读取配置创建工具实例
6. **启动时自动同步所有内置工具到数据库**：数据库和代码保持一致，支持工具管理界面

---

## 混合模式工具调用链路（2026-04-22 更新）

### 目标
实现**简单工具自动 + 关键工具收敛**的混合模式工具调用链路：
- `auto` 模式：简单工具走 Rig 原生同步 function call 流程，开发高效
- `manual` 模式：关键工具走自建异步事件链路，支持权限控制、全链路审计、大结果附件存储

满足多 Agent 协作场景下对关键工具调用的可控性要求，同时兼容 Rig 原生能力。

### 核心设计决策

| 设计点 | 方案 | 原因 |
|--------|------|------|
| **混合模式分类** | `control_mode: auto \| manual` | 不是按工具类型分，而是按控制要求分：简单工具 `auto`，需要审计/权限 `manual` |
| **工具调用存储** | 复用现有 `messages` 表 | 工具调用本身就是特殊消息，利用已有消息状态、附件存储、关联机制，不新建表 |
| **核心 trait** | 内部统一用 `CoreTool`（带 `RequestContext`） | 所有工具都需要访问上下文（DB、用户、权限、跟踪ID），统一接口方便装饰器 |
| **Rig 兼容** | `RigToolAdapter` 适配器 | Rig 需要 `ToolDyn`，适配器持有 `RequestContext`，调用 `CoreTool.call()` |
| **日志装饰** | `LoggingDecorator` 独立 | 日志和工具执行是两个职责，装饰器模式非侵入式添加 |
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

    /// 工具名称（用于 Rig 注册和 LLM 识别）
    fn name(&self) -> &str;

    /// 工具描述（给 LLM 看）
    fn description(&self) -> &str;
}

/// 完整工具业务实体 - 包含持久化配置和可执行实例
pub struct Tool {
    pub po: ToolPo,              // 持久化配置（DB 读出）
    pub control_mode: ControlMode, // auto | manual
    pub rig_tool: Option<Box<dyn ToolDyn + Send + Sync>>, // Rig 需要的适配
    pub our_tool: Box<dyn CoreTool + Send + Sync>,         // 我们核心实现
}

/// Rig 适配器 - 将 CoreTool 转换为 Rig 需要的 ToolDyn
pub struct RigToolAdapter {
    ctx: RequestContext,
    inner: Box<dyn CoreTool>,
}

impl ToolDyn for RigToolAdapter {
    // 实现 name/description/parameters_schema 转发给 inner
    // call 方法从 self.ctx 获取 RequestContext，调用 inner.call()
}

/// 向后兼容类型别名
pub type FullTool = Tool;
```

### 目录结构最终

```
ai_orz/
├── common/src/enums/
│   └── message.rs             # + MessageType: ToolCallRequest/ToolCallResult, + ControlMode
├── src/
│   ├── models/
│   │   └── tool.rs            # CoreTool trait + Tool entity + RigToolAdapter
│   ├── pkg/
│   │   ├── tool_registry/
│   │   │   ├── mod.rs         # ToolRegistry - 存储工厂，create_tool() -> Box<dyn CoreTool>
│   │   │   ├── builtin.rs     # BuiltinToolFactory - 内建工具工厂 trait
│   │   │   ├── http.rs        # HTTP 工具（占位）
│   │   │   └── mcp.rs         # MCP 工具（占位）
│   │   └── tool_tracing/
│   │       ├── mod.rs         # 导出
│   │       ├── entry.rs       # ToolCallEntry / ToolCallStatus - JSONL 日志结构
│   │       ├── logger.rs      # ToolCallLogger - 全局日志单例
│   │       ├── tool_call_logger.rs # LoggingDecorator - 包装 CoreTool 添加日志
│   │       └── rig_tool_call_logger.rs # RigToolCallLoggingDecorator - 原始 auto 模式适配
│   └── service/
│       └── dao/tool/
│           ├── mod.rs         # ToolDao trait
│           └── sqlite.rs      # SQLite 实现 - get_tool_full() 按 control_mode 拼装
└── migrations/
    └── 20260417000000_create_tools.sql # 已包含 control_mode 字段
```

### 拼装流程（ToolDao.get_tool_full）

```
Input: ToolPo from DB
  ↓
1. 从 ToolRegistry 根据 protocol 获取工厂，create_tool(po) → Box<dyn CoreTool>
  ↓
2. 用 LoggingDecorator 包装 CoreTool（无论 auto/manual 都打日志）
  ↓
3. match control_mode:
    - Auto:
        * 创建 RigToolAdapter 持有 ctx + 已经日志包装的 CoreTool
        * 包装成 Box<dyn ToolDyn>
        * 返回 Tool { po, rig_tool: Some(...), our_tool: ... }
    - Manual:
        * 不需要 Rig 适配
        * 返回 Tool { po, rig_tool: None, our_tool: ... }
```

### 工作流程图

#### Auto 模式（Rig 原生）
```
User message → Agent → LLM → generates tool call → Rig calls ToolDyn
                                    ↓
                            RigToolAdapter → CoreTool.call(ctx, args)
                                    ↓
                            LoggingDecorator 记录日志 → 返回结果 → Rig → LLM → User
```

#### Manual 模式（自建链路）
```
1. LLM generates tool call request
   ↓
2. System 构造 ToolCallRequest 消息存入 messages 表
   - 消息类型 = ToolCallRequest
   - 状态 = Pending
   - content 存工具调用参数 JSON
   - 关联 agent_id / task_id / project_id
   ↓
3. 发布事件到 EventBus → Worker 消费
   ↓
4. Worker 根据 tool_id 拿到完整 Tool + CoreTool
   执行 CoreTool.call(ctx, args) → 得到结果
   ↓
5. 构造 ToolCallResult 消息存入 messages 表
   - 消息类型 = ToolCallResult
   - 状态 = Success / Failed
   - 大结果 → 存附件 file_meta，content 只存摘要
   ↓
6. 发布完成事件 → 唤醒 Agent → Agent 读取 ToolCallResult → 继续对话给用户
```

### 分层符合性检查

✅ **严格单向逐层调用**：`handler → domain → dal → dao`，无反向调用  
✅ **禁止同层互调**：dal 组合 dao，不跨 dal 调用  
✅ **复用现有基础设施**：消息表、事件总线、附件存储全部复用  
✅ **职责分离清晰**：注册中心、日志、Rig 适配分开，单一职责

### 测试结果

本次重构完成后：
```
test result: ok. 128 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;
```
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

- [x] 所有 trait 接口定义完成
- [x] 错误类型定义完成
- [x] 单例模式设计完成
- [x] ToolManagement 接口占位实现
- [x] ToolExecution 接口占位实现
- [ ] ToolManagement 具体逻辑实现（调用 ToolDal）
- [ ] ToolExecution 具体逻辑实现（调用 ToolCallDao）
- [ ] 单元测试编写

## 🔄 消息驱动工具调用链路（2026-05-11 更新）

### 核心理念对齐

**工具调用本身就是消息**。不依赖 LLM 原生 Function Calling，采用自定义消息格式实现，所有工具调用过程均可追溯、可审计、可回放。

### 完整链路设计

```
┌─────────────┐
│  用户发消息  │ UserMessage
└──────┬──────┘
       │
       ▼
┌──────────────────────────────────┐
│  Agent 思考循环 (Project Domain) │
│  - 组装上下文 + 历史消息          │
│  - LLM 推理判断需求              │
│  - 解析输出格式 → type=tool      │
└──────┬───────────────────────────┘
       │
       ▼  构造 ToolCallRequest 消息
┌──────────────────────────────────┐
│     Message Channel DAL          │
│     - 消息写入数据库              │
│     - to_role = System           │
│     - 触发消费者唤醒              │
└──────┬───────────────────────────┘
       │
       ▼
┌──────────────────────────────────┐
│  消费者框架 (message.rs)         │
│  match msg.to_role {             │
│    System => handle_system_message │
│  }                               │
└──────┬───────────────────────────┘
       │
       ▼
┌──────────────────────────────────┐
│  handle_system_message           │
│  match msg.msg_type {            │
│    ToolCallRequest => {          │
│      - 从 Tool Domain 查工具      │
│      - 校验 tool_name + args     │
│      - 调用 tool.call(ctx, args) │
│      - 捕获结果/错误              │
│    }                             │
│  }                               │
└──────┬───────────────────────────┘
       │
       ▼  构造 ToolCallResult 消息
┌──────────────────────────────────┐
│     Message Channel DAL          │
│     - 消息写入数据库              │
│     - to_role = Agent            │
│     - 触发消费者唤醒              │
└──────┬───────────────────────────┘
       │
       ▼
┌──────────────────────────────────┐
│  Agent 思考循环 (再次进入)        │
│  - 读取 ToolCallResult           │
│  - 结合结果继续 LLM 推理          │
│  - 决定：reply / 再 tool / confirm │
└──────────────────────────────────┘
```

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

```json
{
  "tool_name": "create_task",
  "tool_args": {
    "project_id": "proj_xxx",
    "title": "完成文档编写",
    "description": "编写架构设计文档",
    "priority": "high"
  },
  "thinking_depth": 3,
  "trace_id": "trace_abc123"
}
```

### ToolCallResult 消息格式

```json
{
  "tool_name": "create_task",
  "success": true,
  "result": {
    "task_id": "task_xyz",
    "status": "pending"
  },
  "error": null,
  "execution_ms": 234,
  "trace_id": "trace_abc123"
}
```

**失败场景：**
```json
{
  "tool_name": "create_task",
  "success": false,
  "result": null,
  "error": {
    "code": "PERMISSION_DENIED",
    "message": "Agent 没有在该 Project 下创建任务的权限"
  },
  "execution_ms": 15,
  "trace_id": "trace_abc123"
}
```

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

**工具注册表单例：**

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn ContextTool>>,
}

impl ToolRegistry {
    // 全局单例
    pub fn instance() -> &'static Self { ... }
    
    // 注册工具（Agent 入职时绑定）
    pub fn register(&self, tool: Box<dyn ContextTool>);
    
    // 查找工具
    pub fn get(&self, name: &str) -> Option<&dyn ContextTool>;
    
    // 列出 Agent 可用工具（用于 Prompt 组装）
    pub fn list_for_agent(&self, agent_id: &str) -> Vec<ToolInfo>;
}
```

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
| **Rig Auto** | 简单无状态工具（计算、格式化等） | rig-core 原生工具调用机制，快速开发 |
| **自建 Manual** | 组织能力工具（创建任务/项目、分配 Agent 等） | ✅ 消息驱动链路，可追溯、可审计、可控 |

**两者共存策略：**
- Rig Auto 工具直接在思考循环中同步调用，不经过消息队列
- 自建 Manual 工具走完整消息链路，所有操作留痕
- LLM 输出格式中通过 `tool_type: "rig" | "manual"` 区分

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

**现在：**
```rust
#[async_trait]
pub trait BuiltinToolFactory: Send + Sync + Debug {
    fn create_po(&self) -> ToolPo;
    async fn create(&self, po: ToolPo) -> Result<Box<dyn CoreTool>, AppError>;
}
```

#### 2. 新增 ToolPo::fill_defaults_for_builtin()
为 Builtin 工具自动填充默认值：
```rust
impl ToolPo {
    pub fn fill_defaults_for_builtin(mut self) -> Self {
        self.protocol = ToolProtocol::Builtin;
        self.control_mode = ControlMode::Auto;
        self.version = 1;
        self
    }
}
```

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

#### 4. 简化 sync_builtin_tools_to_db()
**之前：**
- 手动设置 protocol、control_mode、version
- 手动构造 ToolPo

**现在：**
```rust
let po = factory.create_po().fill_defaults_for_builtin();
// 直接 upsert 即可
```

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
```
test result: ok. 283 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
**全项目测试全部通过**。

---

## Tool 管理面 Handler（2026-06-04 更新）

### 目标

在既有 Finance Domain / Tool DAL 能力之上补齐用户管理面的 Tool CRUD、状态更新和 Agent 绑定接口。Handler 与用户 Action 一一对应，不引入通用 CRUD Handler 抽象；复用通过 `common/src/api/tool.rs` DTO、`ToolQuery` 查询参数和 Finance Domain 能力完成。

### 路由

所有 Tool 管理面路由统一挂在 Finance 管理域下：

```http
POST   /api/v1/finance/tools
GET    /api/v1/finance/tools
GET    /api/v1/finance/tools/{id}
PUT    /api/v1/finance/tools/{id}
PUT    /api/v1/finance/tools/{id}/status
DELETE /api/v1/finance/tools/{id}
POST   /api/v1/finance/tools/{id}/agent-bind
DELETE /api/v1/finance/tools/{id}/agent-bind
```

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

```text
create_tool.rs
list_tools.rs
get_tool.rs
update_tool.rs
update_tool_status.rs
delete_tool.rs
bind_tool_to_agent.rs
unbind_tool_from_agent.rs
response.rs
```

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

```http
PUT /api/v1/finance/tools/{id}/status
```

请求体：

```json
{
  "status": "Enabled"
}
```

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

- `cargo check` 通过；
- `cargo test` 通过；
- Tool DAO / DAL / Domain 覆盖了删除、内置工具保护、状态迁移和列表查询相关测试。

---

## HTTP Tool Runtime 设计补充（2026-06-22 更新）

### 核心结论

HTTP 工具不设计为一个固定暴露给 Agent 的裸 `http_get` / `http_post` 内置工具，而设计为一套**通用 HTTP Tool Runtime**：

```text
HTTP Runtime 是代码内置能力；
HTTP Tool 是数据库驱动的用户/系统注册工具。
```

用户通过管理页面创建具体 HTTP 工具，写入标准 `tools` 表记录：

```rust
ToolPo {
    protocol: ToolProtocol::Http,
    control_mode: ControlMode::Manual,
    parameters_schema,
    config: HttpToolConfig JSON,
    ...
}
```

运行时根据 `ToolProtocol::Http` 动态构建 `HttpCoreTool` 并执行。

### ToolProtocol 与 ControlMode 正交

`ToolProtocol` 表达工具来源/协议，不决定调用方式：

| 字段值 | 含义 |
|---|---|
| `Builtin` | 代码内置工具，由内置工厂创建 |
| `Http` | HTTP 协议工具，由 `ToolPo.config` 驱动 |
| `Mcp` | MCP 协议工具，后续扩展 |

`ControlMode` 表达谁来调用：

| 字段值 | 含义 |
|---|---|
| `Auto` | 进入 Rig tools，由 Rig 原生 tool calling 自动调用 |
| `Manual` | 不进入 Rig，走自建 `ToolCallRequest` / `ToolCallResult` 消息链路 |

因此：

```text
Builtin 不等于 Auto；
Http 不等于 Manual；
是否进入 Rig 只看 ControlMode。
```

`wrap_for_rig` 的唯一过滤条件是：

```rust
if tool.po.control_mode != ControlMode::Auto {
    continue;
}
```

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

```rust
let registry = get_registry();
let tool = registry.create_tool(po.clone());
```

`ToolRegistry.create_tool()` 根据 `ToolProtocol` 分发：

```rust
match po.protocol {
    ToolProtocol::Builtin => builtin_factory.create(po),
    ToolProtocol::Http => http::create_tool(po),
    ToolProtocol::Mcp => None, // 后续扩展
}
```

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
ToolProtocol::Http → HttpCoreTool
  ↓
ToolCallDao.call_manual()
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

详细方案见：`docs/builtins_http_tool_design.md`。

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
| `bc41fd8` | 修复 HR 测试缺失 DAO 初始化问题 |
| `05ef2f0` | 简化内置工具机制并添加 Builtin 保护 |

---
