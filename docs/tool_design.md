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
