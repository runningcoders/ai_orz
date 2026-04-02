# AGENTS.md - AI Agent 开发规则

> 本文档约定 AI Agent 开发 `ai_orz` 项目时必须遵守的规则，保证代码质量和架构一致性。

---

## 📋 核心架构规则（必须遵守）

### 1. 分层架构（严格遵守，不能乱）

项目严格遵循四层架构：

```
handlers → domain → dal → dao → models
```

| 层级 | 位置 | 职责 | 不允许做 |
|------|------|------|----------|
| **handlers** | `src/handlers/` | HTTP 接入，解析请求，调用 domain，返回响应 | 不能包含业务逻辑 |
| **domain** | `src/service/domain/` | 核心业务逻辑，领域规则 | 不能直接操作数据库 |
| **dal** | `src/service/dal/` | 具体业务编排，组合多个 DAO 完成业务 | 不能包含领域规则 |
| **dao** | `src/service/dao/` | 数据访问层，数据库增删改查 | 不能包含业务逻辑 |
| **models** | `src/models/` | 实体定义，数据库 PO 映射 | 不能包含业务逻辑 |

### 2. 编码规范（必须遵守）

#### ✅ 必须遵守：

- **所有 DAO 遵循单例模式**：使用 `OnceLock<Arc<dyn Trait>>`，对外暴露 `dao()` 函数获取单例
- **所有 DAO delete 接口接收完整 PO 对象**：`fn delete(&self, ctx: RequestContext, po: &EntityPo) -> Result<(), AppError>`，保持接口风格统一
- **状态枚举存放在 `pkg::constants::status`**：命名格式 `EntityPoStatus`，比如 `AgentPoStatus`、`ModelProviderPoStatus`
- **所有枚举存放在 `pkg::constants`**：provider type 等都放这里
- **Brain 实体不直接提供 prompt 方法**：所有 LLM 调用都走 `BrainDao`，`Brain` 只存储 `Cortex` 实例
- **文件层级避免深度过大**：尽量扁平，不要嵌套太多层
- **外部第三方调用都收敛到 DAO 层**：所有 API 调用都在 DAO 层，上层不直接调用

#### ❌ 禁止：

- 不要跨层级调用
- 不要在 handler 写业务逻辑
- 不要把业务规则写到 dao 层
- 不要硬编码配置，所有配置走环境变量/配置文件

### 3. 数据库命名规范

- 表名：下划线命名法，复数形式：`agents`、`model_providers`、`organizations`
- 字段名：下划线命名法
- 必须包含：`id`, `status`, `created_by`, `modified_by`, `created_at`, `updated_at`
  - `status`: `i32`，`0` 表示删除
  - `created_at/updated_at`: 秒级时间戳 `i64`

---

## 🎯 Frontend 规则（Dioxus 前端）

### 1. 目录结构

```
frontend/src/
├── main.rs          # 入口，全局状态管理（当前页面）
├── api/             # API 调用封装
└── components/      # 可复用组件 / 页面组件
```

### 2. 页面切换规则

当前项目保持简洁，**不使用完整 Dioxus 路由**，用简单枚举 + 信号管理当前页面：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Reception,
    EmployeeManagement,
    AgentManagement,
}

let current_page = use_signal(|| Page::Reception);
```

遵循这个方式，不要引入完整路由库。

### 3. 开发规则

- 员工管理页面暂时只需要基础页面结构，不需要完整业务逻辑
- Agent id 由后端生成，前端不需要用户填写 id
- 先用示例数据模拟展示 UI，后续再对接真实 API

---

## 🔧 开发流程

### 1. 新增功能流程

1. **先改 models** → 添加/修改实体定义
2. **再改 dao** → 实现数据访问
3. **再改 dal/domain** → 实现业务逻辑
4. **最后改 handler** → 暴露 HTTP API
5. **最后改前端** → 对接 API 展示 UI

### 2. 提交规则

- 每次完成一个功能就提交一次，不要攒一大堆一起提交
- 提交信息格式：`type: description`，比如 `feat: add model provider crud api`
- 所有修改必须编译通过才能提交，不允许提交编译失败的代码

### 3. 依赖管理

- 不要随便删除现有依赖
- 添加依赖需要确认确实需要，不要添加没用的依赖
- 保留 `js-sys` 和 `web-sys` 依赖，不要删除它们

---

## ❌ 禁止做

1. **不要修改**这些引导文件：`AGENTS.md`、`ARCHITECTURE.md`、`NAMING_CONVENTION.md` 除非你要修正规则
2. **不要修改**只读引导文件：`MEMORY.md`、`SOUL.md`、`TOOLS.md`、`AGENTS.md` 这些在 workspace root 的文件
3. **不要删除**提交记录，不要强制 push
4. **不要硬编码**端口、API 地址、数据库路径，这些都要支持环境变量
5. **不要忽略编译错误**，所有错误必须修复才能提交

---

## ✅ 完成标记

当你完成一个功能，记得：

1. 编译验证：`cargo check` 没有错误
2. 提交代码：`git add . && git commit -m "..." && git push`
3. 告诉人类完成了什么

---

## 📝 记住

**架构一致性比速度重要**，慢点没关系，错了更耽误时间。严格遵守这些规则，代码才好维护！
