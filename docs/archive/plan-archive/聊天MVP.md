# 聊天MVP

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：聊天MVP 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 排查聊天页消息链路异常时，按"数据流图 + 验收清单"快速定位层级
> - 从短轮询升级为 SSE/WebSocket 时，回看"Handler API 契约 + 双向分页模式"不破坏
> 状态：**完成（2026-07-12）**
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范
> - [飞书P2P消息集成.md](./飞书P2P消息集成.md) — 其他消息渠道接入

---

## 一、MVP目标（为什么做）

ai_orz 系统仅有后端消息链路，无前端用户对话界面。本期上线对话 MVP：首页即对话页，支持前台 Agent 对话和项目内 PMO 对话。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 无用户对话入口 → Agent 能力无法直接触达 | 前端首页改为对话页（左右分栏布局），左侧项目列表 + 右侧对话区 |
| (b) 消息列表查询 API 缺失 → 前端无法拉历史 | 后端新增 `list_messages` API：支持 project_id/task_id 过滤 + before_timestamp 上拉翻页 + after_timestamp 下拉轮询（双向分页模式） |
| (c) 用户主动发消息 API 未对接现有 send_message_to_agent | 复用已有 neural_tool `send_message_to_agent`，Handler 层修正 from_id/from_role User 角色判断 |
| (d) 实时性：Agent 回复如何到达前端 | MVP 采用 **3 秒短轮询** + after_timestamp 增量拉取（最稳，无需升级基础设施），后续升级 SSE 复用同一 Response 结构 |
| (e) 多租户隔离：A 组织用户不能看 B 组织消息 | MessageQuery 新增 `organization_id` 过滤字段（从 RequestContext 注入），SQLite query builder AND 条件拼接 |

**收敛后效果**：用户登录 → 首页对话页 → 左侧选项目/选前台接待 Agent → 右侧立即对话（写新消息 + 3s 轮询自动刷新 + 上拉翻页看历史）全链路打通。

---

## 二、架构思路（怎么做的）

前后端分离 MVP，8 模块严格对齐：

```
前端（Dioxus 0.7 + WASM）
  ├─ 路由：/ → Chat 对话页；/login → Reception 接待页（原首页迁移）
  ├─ Layout/Navbar：新增「对话」导航入口
  ├─ Store/Auth：AuthState 新增 agent_id 字段（当前选中对话 Agent）
  ├─ API 层：
  │   ├─ list_messages(req) → ListMessagesResponse
  │   └─ send_user_message(req) → 复用现有发送接口
  ├─ Pages/Chat：
  │   ├─ 左右分栏 chat-layout（sidebar 项目列表 + messages 对话区 + input 输入框）
  │   ├─ 消息渲染：User/Agent 气泡区分（左右对齐 + 配色）
  │   ├─ 轮询：tokio::spawn 3s setInterval → after_timestamp = last_msg.created_at → 增量 append
  │   └─ 翻页：上拉触顶 → before_timestamp = earliest_msg.created_at → 历史消息 prepend
  └─ index.html：chat-* CSS class（chat-layout/chat-sidebar/chat-messages/message-bubble/chat-input）样式内联
      │
      │ API 调用
      ▼
后端（Axum + SQLite）
  ├─ DAO 层：MessageQuery 新增 organization_id 字段
  │   └─ SQLite query：新增 AND organization_id = ? 条件拼接
  ├─ Common API DTO（message 模块）：
  │   ├─ ListMessagesRequest { project_id/task_id/from_id/to_id/before_timestamp/after_timestamp/limit }
  │   ├─ MessageListItem（from_role/to_role/message_type/status + content + created_at）
  │   └─ ListMessagesResponse { messages: Vec<MessageListItem>, total: usize }
  ├─ Handler 层（list_messages 新建）：
  │   ├─ 路由：GET /api/v1/messages
  │   ├─ 校验：organization_id + user_id 不为空（请求上下文必选）
  │   ├─ 双向分页：
  │   │   ├─ 有 after_timestamp → order_by ASC（新消息追加，返回后时间过滤）
  │   │   └─ 其他 → order_by DESC（历史翻页，返回前时间过滤）
  │   ├─ 时间过滤在 Handler 层完成（DAO 层无 before/after 字段，limit+100 多拉避免漏）
  │   └─ 返回前端统一 ASC（从旧到新）
  ├─ Handler 层（send_message_to_agent 调整）：
  │   └─ 修正 from_id/from_role 判断逻辑，支持 User 角色作为发送方
  └─ Router：注册 list_messages 路由
```

**关键边界（行为红线，回归必保）**：
1. **双向分页语义（契约！）**：
   - `before_timestamp`：只返回 created_at < before 的消息 → DESC 查询，条数 = limit，前端 prepend（翻更早历史）
   - `after_timestamp`：只返回 created_at > after 的消息 → ASC 查询，条数不限，前端 append（轮询新消息）
   - 返回给前端的 `messages` 排序**永远 ASC（从旧到新）**，DESC 查询结果需反转
2. **多租户隔离（安全）**：所有 Handler 入口必须从 RequestContext 取 organization_id 并注入 MessageQuery，禁止仅按 project_id 过滤（跨租户 project_id 可能碰撞）
3. **短轮询间隔**：3 秒（MVP 平衡体验/负载），后续升级 SSE/WebSocket 时 API 契约不变（仅前端轮询函数替换为 event_source）
4. **用户消息角色**：from_role = User(0)，from_id = 当前登录用户 UID（禁止让前端传 from_role，后端从 ctx 取）
5. **非文本消息渲染**：MVP MessageListItem 的 content 字段直接展示原始内容（ToolCall/Result/TaskAssignment 类型未来做卡片化，本期文本兜底）

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按前后端 + 分层索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **后端 DAO 层** | | |
| [src/service/dao/message/mod.rs](../../src/service/dao/message/mod.rs) | Message DAO trait | MessageQuery 新增 `organization_id: Option<String>` 字段（用于多租户过滤） |
| [src/service/dao/message/sqlite.rs](../../src/service/dao/message/sqlite.rs) | Message DAO SQLite 实现 | query 方法在排序前拼接 `AND organization_id = ?` 条件 |
| **后端 Common API DTO（新增）** | | |
| [common/src/api/message.rs](../../common/src/api/message.rs) | 消息 API DTO（新建） | 新增 ListMessagesRequest / MessageListItem / ListMessagesResponse 三个结构；Query 参数 derive(Params + JsonSchema) |
| [common/src/api/mod.rs](../../common/src/api/mod.rs) | common API 模块入口 | 新增 `pub mod message;` 注册 |
| **后端 Handler 层** | | |
| [src/handlers/finance/message/list_messages.rs](../../src/handlers/finance/message/list_messages.rs) | list_messages handler（新建） | GET /api/v1/messages；上下文校验 + 双向分页 + 时间过滤 + 排序对齐 |
| [src/handlers/finance/message/send_message_to_agent.rs](../../src/handlers/finance/message/send_message_to_agent.rs) | 发送用户消息 handler | 修正 from_id/from_role User 角色判断逻辑（之前假设发送方都是 Agent） |
| [src/handlers/finance/message/mod.rs](../../src/handlers/finance/message/mod.rs) | message handler 模块 | 导出 list_messages 新子模块 |
| **后端 Router** | | |
| [src/router.rs](../../src/router.rs) | 路由注册 | 新增 GET `/api/v1/messages` 路由 → list_messages handler |
| **前端 Layout/Store** | | |
| [frontend/src/pages/mod.rs](../../frontend/src/pages/mod.rs) | 前端路由枚举 | `/` 路径改为 Route::Chat（对话页）；Reception 移到 `/login` |
| [frontend/src/main.rs](../../frontend/src/main.rs) | 前端渲染入口 | Route::Chat 渲染入口更新 |
| [frontend/src/layouts/navbar.rs](../../frontend/src/layouts/navbar.rs) | 导航栏 Layout | 新增「对话」导航入口（跳转 /） |
| [frontend/src/store/auth.rs](../../frontend/src/store/auth.rs) | Auth 状态管理 | AuthState 新增 `agent_id: Option<String>`（当前选中对话 Agent，选中项目时同步绑定） |
| **前端 API 层** | | |
| [frontend/src/api/message.rs](../../frontend/src/api/message.rs) | 前端消息 API Client | 新增 list_messages / send_user_message 函数（对应后端协议结构体入参） |
| **前端 Pages（对话页核心）** | | |
| [frontend/src/pages/message/chat.rs](../../frontend/src/pages/message/chat.rs) | 对话页 MVP（重写） | 左右分栏 chat-layout；左侧项目列表（点击切换 context）；右侧对话区（消息渲染 + 上拉翻页 + 3s 轮询）；底部输入框 + 发送按钮 |
| **前端样式** | | |
| [frontend/index.html](../../frontend/index.html) | 前端 HTML 入口 | `<style>` 内联 chat-* CSS 类：chat-layout 分栏、chat-sidebar 侧边栏样式、chat-messages 滚动区、message-bubble 左右气泡、chat-input 输入栏 |
| **零改动面（验证架构稳定性）** | | |
| 数据库 Schema / MessageDomain / MessageConsumer / awaken 链路 / Agent → 用户 deliver_message 投递 | 对外契约不变 | 无修改；用户消息入队 → Agent 唤醒 100% 复用现有链路 |

---

## 四、对话扩展速查表（消息能力升级路径参考）

### 4.1 消息查询 API 契约不变，前端仅替换刷新方式

| 刷新方式 | 实现模式 | 对后端改动 | 前端入口参考 |
|---------|---------|-----------|-------------|
| MVP：短轮询 3s | setInterval → list_messages(after_timestamp = last.created_at) | 0 | chat.rs 轮询 loop |
| 升级 1：SSE Server-Sent Events | EventSource /api/v1/messages/stream?project_id=xxx，MessageEvent.data = ListMessagesResponse 单条增量 | 新增 GET `/messages/stream` handler（复用 MessageQuery + 调 SSE PushDao broadcast） | [SSEPushDao 参考 src/service/dao/message_push.rs](../../src/service/dao/message_push.rs) |
| 升级 2：WebSocket | ws://messages/ws 双向，前端订阅 project_id | 新增 WS 路由 + actor 管理订阅 | 参考飞书 WS 接入模式 [ws.rs](../../src/service/dao/lark/ws.rs) |

> **核心不变**：ListMessagesRequest / MessageListItem 契约不动，三种刷新方式共享同一 DTO。

### 4.2 非文本消息卡片化渲染升级模板

| MessageType（值） | MVP 兜底 | 升级卡片化 | 入口 |
|------------------|---------|-----------|------|
| Text (0) | content 原样渲染 | 保持 | chat.rs 消息渲染分支 |
| ToolCallRequest (5) | 文本展示 | 折叠卡片：工具名 + args JSON pretty + 「加载中」状态 | 同上 |
| ToolCallResult (6) | 文本展示 | 折叠卡片：工具结果 + 耗时 + 成功/失败颜色 | 同上 |
| TaskAssignment (9) | 文本展示 | 任务卡片：标题 + 状态 badge + 跳转链接 | 同上 |

代码入口：[chat.rs 渲染 messages 迭代](../../frontend/src/pages/message/chat.rs)（用 from_role + message_type 双分支匹配）

---

## 五、验收清单（2026-07-12 全部达成 ✅）

  - before_timestamp 场景：返回 created_at < before 的 limit 条，排序 ASC
  - after_timestamp 场景：返回 created_at > after 的全部新消息，排序 ASC

---

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

## 六、执行结果摘要（2026-07-12，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| 后端 DAO：organization_id 过滤（单元测试） | 跨组织数据无泄漏，PASS |
| 后端 Handler：list_messages 双向分页 | before / after 两种场景 + DESC→ASC 排序，4 测试 PASS |
| 后端完整链路（发送→入队→消费→回复→轮询拉取） | 端到端测试通过（手动模拟 + 集成） |
| 前端路由：/ → Chat，/login → Reception | 编译 + 手动跳转 PASS |
| 前端 UI：左右分栏 + 气泡渲染 | 手动验收 User/Agent 区分正确 |
| 前端轮询 + 翻页：3s 增量 / 上拉历史 | 手动操作 PASS（无重复、漏消息、滚动跳动） |
| 后端 lib 全量测试 | 全部 PASS（697+ 个） |
| Clippy（后端 + 前端 wasm32） | 双端零警告 |
| 前端 wasm32 编译（全量） | 0 errors |

### 与计划的 1 处偏离（实现优化，业务零影响）
原计划「DAO 层 Query 直接支持 before_timestamp/after_timestamp 字段 + SQL 级过滤」，实现时因 SQLite QueryBuilder 扩展字段成本高（影响所有 DAO 实现），改为 Handler 层 `limit+100` 多拉后内存时间过滤。MVP 消息量下性能足够，后续批量数据时再下沉到 SQL 层。

---

## 七、后续扩展路径（4 步模板，按优先级）

> **核心不变量**：ListMessagesRequest/Response 契约不破坏；短轮询→SSE→WS 升级时前后端 DTO 共享。

1. **SSE 实时推送（替代短轮询，优先级最高）**：
   - 后端新增 GET `/api/v1/messages/stream` handler，按 project_id/task_id 调 SsePushDao 订阅频道
   - 前端 chat.rs：`use_effect` 创建 EventSource，onmessage 解析 MessageEvent → 复用现有 append_message 逻辑
   - 参考：[message_push.rs SsePushDao](../../src/service/dao/message_push.rs)
2. **非文本消息卡片化**：
   - chat.rs 渲染分支按 `message_type` match：5/6 → 工具折叠卡片，9 → 任务卡片
   - 新增独立 `message_card.rs` 组件，接收 MessageListItem → Dioxus Node
   - 参考 §4.2 映射表
3. **MVP 功能增强（常规 IM 能力）**：
   - 回复引用（reply_to_id 字段已在 MessageListItem，前端显示引用气泡）
   - 消息状态（Pending/Processing/Processed/Failed）用气泡边框色或 spinner 表示（status 字段已有）
   - 搜索：复用 MessageQuery.keyword FTS5 全文检索，顶部输入框传 keyword
4. **移动端适配（窄屏布局）**：
   - chat-layout 在 <640px 改为上下结构（顶部顶栏切换项目 → 全屏对话区）
   - 输入框软键盘弹起时 sticky 定位（CSS 变量 + viewport 适配）
   - 参考 Tailwind DaisyUI 迁移方案 [Tailwind DaisyUI迁移.md](./Tailwind DaisyUI迁移.md) 响应式模式