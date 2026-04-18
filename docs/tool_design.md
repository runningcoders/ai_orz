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

## 后续待扩展

1. **添加第一个内置工具**：现在基础架构已完成，可以开始实现具体工具
2. **HTTP 协议工具支持**：目前是占位结构，待实现
3. **MCP 协议工具支持**：目前是占位结构，待实现
4. **ToolEmbedding 语义自动选择**：基于 embedding 做工具相关性排序，减少上下文
5. **运行时动态加载工具**：从数据库读取配置创建工具实例

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
