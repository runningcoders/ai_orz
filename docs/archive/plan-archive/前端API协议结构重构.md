# 前端 API 协议结构体统一改造

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：前端API协议结构重构 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 状态：完成（2026-07-27 验收通过）
> 查阅场景：新增前端 API 方法时回看「改造原则 + 通用改造模式」两处即可，无需通读全文
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目架构规范 §4.11 前后端 API 协议规范
> - [api_protocol_convention.md](../design/api_protocol_convention.md) — DTO 契约规范（common 单一事实源）
> - [frontend_architecture.md](../design/frontend_architecture.md) — 前端架构设计

---

## 一、目标（为什么做）

`frontend/src/api/*.rs` 中 54 个方法采用"拆参数"签名（path/query/body 字段分散在方法参数列表），与后端 Handler 和 common 协议结构体不对称，存在以下问题：

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 方法签名与 common 协议结构体脱节 | 拆参数方法改为接受 `common::api::*Request` 结构体作为唯一入参 |
| (b) URL 拼接逻辑散落在调用方 | URL 拼接收敛到方法内部手工处理，新增 `build_pagination_url` / `build_query_string` helper |
| (c) StatsOptions 专用类型与 common 统计字段重复 | 废弃 StatsOptions，统计参数纳入对应 `GetXxxRequest`（common 已含 `with_stats` 等字段） |
| (d) hr.rs 和 finance.rs 中 `list_tools` 重复实现 | hr.rs::list_tools 改为重导出 finance::list_tools |

**收敛后效果**：前端 API 方法签名与后端 Handler 协议对称，`common::api::*` 结构体为前后端单一事实源，54 个拆参数方法改造完成后 StatsOptions 彻底废弃。

---

## 二、架构思路（怎么做的）

四层改造，知识逐层下沉：

```
调用方（pages/hooks，按需更新）
  │  构造 Request 结构体 → 调统一 API 方法
  ▼
frontend/src/api/*.rs（54 个方法签名收敛）
  │  拆参数方法：签名改为接受 common Request 结构体
  │  URL 拼接：方法内部手工分离 path/query/body
  │  body-only 方法：不动（已经是协议结构体）
  │  单字段方法（如 delete_agent(id)）：保持原始类型
  ▼
frontend/src/api/mod.rs（helper 下沉）
  ├─ build_pagination_url：分页参数 → query string
  └─ build_query_string：通用 (key, Option<String>) → query string
  └─ 清理：删除废弃 StatsOptions / build_url_with_stats
  ▲
  │  调用方按需填充 Request 结构体字段
common/src/api/*.rs（协议结构体已存在，含 #[param(source)] 标注）
```

**关键边界（行为红线，回归必保）**：
1. body-only 方法保持不动（已经是协议结构体入参）
2. 单字段方法（如 `delete_agent(id: &str)`）保持原始类型，不包一层 `DeleteXxxRequest`
3. 不引入新 macro，path/query/body 分配由方法内部手工代码完成
4. 改造原则：前后端 API 签名对称，调用者无需关心 URL 拼接，协议结构体为 single source of truth
5. 54 个拆参数方法全部覆盖：finance 14 + hr 18 + project 10 + system 6 + log_stats 2 + message 4

---

## 三、涉及文件清单（读代码直接跳）

按分层索引，每行带可点击路径链接：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **前端 API 层（签名收敛 + helper）** | | |
| [frontend/src/api/mod.rs](../../frontend/src/api/mod.rs) | API 工具层 | 新增 `build_pagination_url` / `build_query_string` helper；删除废弃 `StatsOptions` 和 `build_url_with_stats` |
| [frontend/src/api/finance.rs](../../frontend/src/api/finance.rs) | 财务域 API | 改造 14 个拆参数方法（模型提供商 5 + 工具管理 5 + 其他 4），签名改为接受 common 协议结构体 |
| [frontend/src/api/hr.rs](../../frontend/src/api/hr.rs) | HR 域 API | 改造 18 个拆参数方法（Agent 管理 5 + 包管理 6 + 技能与记忆 7）；`list_tools` 改为重导出 finance::list_tools |
| [frontend/src/api/project.rs](../../frontend/src/api/project.rs) | 项目域 API | 改造 10 个拆参数方法（项目管理 + 任务管理 + 产物管理） |
| [frontend/src/api/system.rs](../../frontend/src/api/system.rs) | 系统域 API | 改造 6 个拆参数方法（cron trigger + log query + AOP stats） |
| [frontend/src/api/log_stats.rs](../../frontend/src/api/log_stats.rs) | 日志统计 API | 改造 2 个拆参数方法（日志级别分布 + 时间序列） |
| [frontend/src/api/message.rs](../../frontend/src/api/message.rs) | 消息域 API | 改造 4 个拆参数方法（历史消息加载 + 轮询 + 搜索） |
| **调用方（同步更新）** | | |
| [frontend/src/pages/**](../../frontend/src/pages/) | 页面模块 | 按编译错误定位，所有调用点改为构造 Request 结构体 |
| [frontend/src/hooks/**](../../frontend/src/hooks/) | 自定义 Hooks | `use_workspace_data.rs` 等调用点同步更新 |
| **零改动面（验证架构稳定性）** | | |
| common/src/api/*.rs / 后端 Handler / 路由 / 后端业务逻辑 | 对外契约不变 | 协议结构体为单一事实源，零修改；后端测试断言原样通过 |

---

## 四、通用改造模式速查表（新增 API 方法时套用）

新增 API 方法按以下 4 种模式套用，改造入口统一在 `frontend/src/api/` 各域文件：

### 4.1 四种改造模式对照表

| 模式 | 场景 | 签名变化 | URL 拼接方式 | 参考入口 |
|------|------|---------|-------------|---------|
| 模式 A：path + body | 如 `update_agent` | `fn(id, req)` → `fn(req)` | `format!("/api/.../{}", req.id)` | [finance.rs::update_model_provider](../../frontend/src/api/finance.rs) |
| 模式 B：path + query（含 stats） | 如 `get_agent` | `fn(id, stats_options)` → `fn(req)` | 内部用 `build_query_string` 拼接 5-6 个 stats query 字段 | [hr.rs::get_agent](../../frontend/src/api/hr.rs) |
| 模式 C：path only + 空 body | 如 `install_tool_pack` | `fn(agent_id, tag)` → `fn(req)` | 多 path 字段从 req.agent_id / req.tag 读取 | [hr.rs::install_tool_pack](../../frontend/src/api/hr.rs) |
| 模式 D：query 分页 | 如 `list_agents` | `fn(limit, offset)` → `fn(req)` | 用 `build_pagination_url` 拼接 pagination | [hr.rs::list_agents](../../frontend/src/api/hr.rs) |

> 方法签名统一规则：body-only 和单字段方法保持不动，不强制结构体化。

---

## 五、验收清单（2026-07-27 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-27，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| 前端 wasm32 编译 | 零 error，零 warning |
| 前端全量测试 | 46 passed，100% 通过 |
| 改造方法统计 | 54 个拆参数方法 + 2 个 helper + 1 个重复实现合并 |
| StatsOptions 清理 | 从 mod.rs 删除，grep 扫描零残留调用 |
| 后端回归 | 零改动，所有测试维持 100% 通过 |

### 与计划的偏离（如有）
无重大偏离，54 个方法按计划全部覆盖，按文件分批提交验证。

---

## 七、后续扩展路径（新增 API 方法 4 步模板）

> **核心不变量**：common DTO 契约 / 后端 Handler / 路由机制不动。

1. **common 协议结构体**：[common/src/api/](../../common/src/api/) — 如对应域的 Request 结构体不存在，先在对应域文件新增，字段加 `#[param(source = "path"|"query"|"body")]` 标注
2. **前端 API 方法签名**：[frontend/src/api/finance.rs](../../frontend/src/api/finance.rs)（或 hr/project/system 对应文件）— 按 §四 速查表选择模式，签名改为接受 Request 结构体引用或值，URL 拼接在方法内部
3. **分页和 query helper**：[frontend/src/api/mod.rs](../../frontend/src/api/mod.rs) — 分页场景用 `build_pagination_url`，多字段 query 用 `build_query_string(&[(key, opt_val), ...])`
4. **调用方更新**：[frontend/src/pages/](../../frontend/src/pages/) 和 [frontend/src/hooks/](../../frontend/src/hooks/) — 按编译错误定位，构造 Request 结构体传入（可在调用文件顶部定义 helper fn 减少重复构造代码）

完成。