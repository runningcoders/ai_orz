# 飞书P2P消息集成

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：飞书P2P消息集成 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 新增企业微信/Slack 等其他 P2P 渠道时，回看"分层架构红线 + 接入 4 步模板"
> - 排查飞书消息链路异常时，参考"数据流图 + 验收清单"快速定位入口
> 状态：**完成（2026-07-17）**
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范（consumer → domain → dal → dao 单向）
> - [lark-cli_集成二期.md](./lark-cli_集成二期.md) — 飞书 CLI 渠道集成
> - [身份凭证Domain统一CRUD重构.md](./身份凭证Domain统一CRUD重构.md) — 飞书凭证管理

---

## 一、集成目标（为什么做）

已有飞书渠道仅支持"系统→飞书"出站推送，缺失"飞书用户→Agent"入站接收能力，且架构无通用"外部消息适配层"。本期按 v4 AOP 中台方案落地：

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 入站能力缺失 → 飞书用户无法私信 Agent | LarkDao 新增 WebSocket 长连接：订阅 `im.message.receive_v1` P2P 事件，事件回调 → 适配层 → 内部消息入队 |
| (b) 渠道接入无统一抽象 → 每加渠道（微信/Slack）consumer 改分支 | `pkg/aop/message_adapter` 通用中台：`MessageInboundAdapter` trait + `MessageAdapterCallback` trait，consumer 只依赖中台 |
| (c) v3 架构分层违规 → consumer 直接依赖 DAL | Agent 路由上移到 consumer（通过 HrDomain 查询），LarkMessageChannelDal 仅做"事件→AdaptedMessage"纯转换，不依赖其他 DAL |
| (d) Agent 路由不灵活 → 仅支持渠道绑定特定 agent | 路由 3 级：① MessageChannel.agent_id（绑定）→ ② `feishu_reception` role Agent（前台接待）→ ③ 兜底任意 Onboarded Agent |
| (e) 用户映射无约束 → 可能产生垃圾数据 | 管理员预先绑定：创建 MessageChannel 配置 `lark_open_id` + `user_id` + `agent_id`，未绑定用户被拒绝 |

**收敛后效果**：飞书私信双向链路打通，后续接入新外部渠道只需实现 `MessageInboundAdapter` trait（consumer 零改动）。

---

## 二、架构思路（怎么做的）

v4 AOP 中台方案，5 层严格解耦（consumer → domain → dal → dao 单向）：

```
飞书用户
  │ ① 私信消息
  ▼
飞书开放平台（WebSocket 长连接 + 事件重投递）
  │ ② im.message.receive_v1 事件（二进制帧）
  ▼
DAO 层：LarkDao（飞书 SDK 全封装）
  ├─ LarkDaoHttpImpl: HTTP API（push / test_connection / tenant_access_token 缓存）
  ├─ LarkDaoWsImpl: WebSocket 长连接（连接管理 / 30s 心跳 / 指数退避重连 / 事件分发）
  └─ LarkEventHandler trait：事件回调接口
      │ ③ LarkMessageEvent（已解析）
      ▼
pkg 层：AOP 消息适配中台（通用抽象，零业务依赖）
  ├─ MessageInboundAdapter trait：渠道适配者实现此 trait
  ├─ MessageAdapterCallback trait：consumer 端实现（回调业务处理）
  └─ AdapterRegistry：按 ChannelType 注册/获取 adapter（Arc<dyn Any + Send + Sync>）
      │ ④ AdaptedMessage（owned，from_id/to_agent_id:Option/content/project_id...）
      ▼
Consumer 层：LarkEventDispatcher（编排层，不碰具体 DAL）
  ├─ 实现 MessageAdapterCallback trait
  ├─ adapt 回调：若 to_agent_id=None → 调 HrDomain 路由（feishu_reception role → 兜底）
  └─ 调 MessageDomain.delivery().send_to_agent(cmd)
      │ ⑤ 内部消息入队（完全复用现有链路）
      ▼
Domain → MessageConsumer → awaken() → Agent 处理 → send_message 工具 → deliver_message
      │ ⑥ 回复消息
      ▼
MessageChannelDal match ChannelType::Lark → lark_dao.push()
      │ ⑦ 调用飞书 /open-apis/im/v1/messages
      ▼
飞书用户收到回复
```

**关键边界（行为红线，回归必保）**：
1. **分层约束（v3/v4 反复验证）**：consumer → domain → dal → dao **单向调用**。DAL 不得调 DAL；consumer 不得直接调 DAL（只能通过 AOP 中台 + Domain）
2. **LarkMessageChannelDal（DAL）纯转换**：只依赖 MessageChannelDao，不依赖 AgentDal/HrDomain。`AdaptedMessage.to_agent_id` 为 Option，渠道未绑定则返回 None
3. **Agent 路由上移到 Consumer**：LarkEventDispatcher 通过 HrDomain.agent_manage().query() 查询，roles 参数精确匹配 `feishu_reception` tag
4. **WebSocket 长连接**：断线必须指数退避重连；飞书事件有 `event_id` 去重机制，业务层无需幂等
5. **用户绑定前置**：`lark_open_id` 未在 MessageChannel 表找到时，记录警告并**不入队**，禁止自动创建 User（避免垃圾数据）

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按 AGENTS.md §3.2 目录结构索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **DAO 层（飞书 SDK 封装，新增为主）** | | |
| [src/service/dao/lark/mod.rs](../../src/service/dao/lark/mod.rs) | LarkDao trait 定义 | 扩展 trait：新增 start_event_listener / stop_event_listener；新增 LarkEventHandler trait；全局 OnceLock 初始化 |
| [src/service/dao/lark/error.rs](../../src/service/dao/lark/error.rs) | 飞书错误类型 | 新增 LarkError：API 错误码、网络错误、协议错误 |
| [src/service/dao/lark/token.rs](../../src/service/dao/lark/token.rs) | Token 缓存 | 新增 TokenCache：tenant_access_token 内存缓存 + 提前 5min 刷新 + Arc<RwLock> 并发安全 |
| [src/service/dao/lark/event.rs](../../src/service/dao/lark/event.rs) | 事件类型 | 新增 LarkMessageEvent 等结构体 + 事件解析（P2P 文本保留，群聊/非文本过滤丢弃） |
| [src/service/dao/lark/http.rs](../../src/service/dao/lark/http.rs) | HTTP API 实现 | 实现 push / test_connection / get_token / send_message（reqwest） |
| [src/service/dao/lark/ws.rs](../../src/service/dao/lark/ws.rs) | WebSocket 长连接 | 新增：连接管理 + 30s 心跳 + 指数退避重连 + 事件帧解析 + LarkEventHandler 回调 |
| [src/service/dao/mod.rs](../../src/service/dao/mod.rs) | DAO 初始化 | init_all() 中新增 `lark::init(&config.lark)` |
| **通用基础设施（AOP 中台，新增）** | | |
| [src/pkg/aop/message_adapter/mod.rs](../../src/pkg/aop/message_adapter/mod.rs) | 消息适配中台 | 新增：MessageInboundAdapter trait、MessageAdapterCallback trait、AdapterRegistry 注册中心 |
| [src/pkg/adapter/mod.rs](../../src/pkg/adapter/mod.rs) | 注册中心兼容 | 保留 AdaptedMessage 结构定义 + AdapterRegistry（兼容 v3 方案过渡期） |
| **模型层（字段扩展）** | | |
| [src/models/message_channel.rs](../../src/models/message_channel.rs) | 渠道模型 | ChannelConfig 新增 `lark_open_id`、`lark_user_name` 字段（用户级绑定） |
| **DAL 层（查询扩展）** | | |
| [src/service/dal/message_channel.rs](../../src/service/dal/message_channel.rs) | 渠道 DAL | 新增 find_by_lark_open_id(ctx, open_id) 方法（按 open_id 查绑定渠道） |
| [src/service/dal/agent/mod.rs](../../src/service/dal/agent/mod.rs) | Agent DAL | AgentQuery 新增 roles 参数，透传 DAO json_each 精确匹配 |
| [src/service/dao/agent/mod.rs](../../src/service/dao/agent/mod.rs) | Agent DAO trait | AgentQuery 新增 roles: Vec<String> 过滤字段 |
| [src/service/dao/agent/sqlite.rs](../../src/service/dao/agent/sqlite.rs) | Agent DAO SQLite | query 中 roles 用 `json_each(roles)` 精确匹配（参考 SkillQuery.tags 模式） |
| **Domain 层（能力扩展）** | | |
| [src/service/domain/hr/mod.rs](../../src/service/domain/hr/mod.rs) | HrDomain trait | 新增 find_reception_agent_id()：优先 feishu_reception role → 兜底 Onboarded |
| [src/service/domain/hr/agent.rs](../../src/service/domain/hr/agent.rs) | Agent Domain 实现 | 实现 find_reception_agent_id，调 agent_dal.query(roles=["feishu_reception"]) |
| [src/service/dal/lark.rs](../../src/service/dal/lark.rs) | LarkMessageChannelDal | 新增：adapt_lark(event) → Result<Option<AdaptedMessage>>，纯转换不路由 |
| [src/service/domain/finance/mod.rs](../../src/service/domain/finance/mod.rs) | FinanceDomain trait | MessageChannelManage 新增 find_lark_channel_by_open_id |
| [src/service/domain/finance/message_channel.rs](../../src/service/domain/finance/message_channel.rs) | MessageChannel Domain | 实现 find_lark_channel_by_open_id，透传 message_channel_dal |
| **Consumer 层（编排层，新增）** | | |
| [src/consumer/adapter/mod.rs](../../src/consumer/adapter/mod.rs) | 外部适配器入口 | 新增：init/shutdown，注册所有渠道 adapter；LarkEventDispatcher 启动 |
| [src/consumer/adapter/lark.rs](../../src/consumer/adapter/lark.rs) | 飞书事件分发器 | LarkEventDispatcher：实现 LarkEventHandler + MessageAdapterCallback；Agent 路由；调 send_to_agent |
| [src/consumer/mod.rs](../../src/consumer/mod.rs) | Consumer 入口 | consumer::init 注册并启动 adapter 子模块 |
| **配置层** | | |
| [common/src/config.rs](../../common/src/config.rs) | 配置定义 | 新增 LarkConfig 结构（enabled / app_id / app_secret / encrypt_key / verification_token） |
| **依赖新增** | | |
| Cargo.toml | 依赖声明 | 新增 tokio-tungstenite 0.24（native-tls 特性） |
| **零改动面（验证架构稳定性）** | | |
| MessageConsumer（awaken 链路）/ deliver_message（多渠道投递）/ 数据库 Schema（零 SQL 迁移）/ 前端现有 API 调用 | 对外契约不变 | 无修改；lark.enabled=false 时与原行为字节级一致 |

---

## 四、渠道接入速查表（新增微信/Slack 等 P2P 渠道参考）

### 4.1 新外部渠道接入 5 步模板

| 步骤 | 动作 | 代码入口参考 |
|-----|------|-------------|
| ① DAO 封装：外部 SDK | 封装 HTTP API + 长连接（如有），实现 `XxxDao` trait，事件回调统一用 trait 方式 | [lark/mod.rs](../../src/service/dao/lark/mod.rs)（trait 风格）、[ws.rs](../../src/service/dao/lark/ws.rs)（长连接管理） |
| ② DAL 适配：纯转换 | 新建 `XxxMessageChannelDal`，实现 `adapt_xxx(event) → Result<Option<AdaptedMessage>>`，仅做事件→内部消息转换，**不做任何业务路由** | [dal/lark.rs](../../src/service/dal/lark.rs)（adapt_lark 模式） |
| ③ 中台注册：实现 trait | 实现 `MessageInboundAdapter` trait，在 pkg/aop/message_adapter 注册中心按 `ChannelType::Xxx` 注册 | [message_adapter/mod.rs](../../src/pkg/aop/message_adapter/mod.rs)（trait 定义） |
| ④ Consumer 回调：业务编排 | 实现 `MessageAdapterCallback`，在回调中：填充 Agent（通过 Domain）→ 调 `MessageDomain.delivery().send_to_agent` | [adapter/lark.rs](../../src/consumer/adapter/lark.rs)（LarkEventDispatcher 模式） |
| ⑤ 配置 + 绑定：用户映射 | 扩展 ChannelConfig 加渠道特有字段（如 `wechat_openid`）；管理员通过现有 MessageChannel API 绑定 | [message_channel.rs ChannelConfig](../../src/models/message_channel.rs) |

> 代码入口（中台核心抽象）：[pkg/aop/message_adapter/mod.rs](../../src/pkg/aop/message_adapter/mod.rs)

### 4.2 飞书特定路由模式参考

| 路由优先级 | 规则 | 入口路径 |
|-----------|------|---------|
| 1（最高） | MessageChannel.agent_id 字段有值 → 直接绑定 | [lark.rs find_reception_agent_id](../../src/consumer/adapter/lark.rs) |
| 2 | 查询 AgentPo.role 含 `feishu_reception` 且状态 Onboarded → 取首个 | [hr/agent.rs find_reception_agent_id](../../src/service/domain/hr/agent.rs) |
| 3（兜底） | 查询任意状态 Onboarded Agent → 取首个 | [hr/agent.rs find_reception_agent_id](../../src/service/domain/hr/agent.rs) |
| 4（拒绝） | 以上均无 → 记录警告，消息不入队 | 同上，返回 Ok(None) |

---

## 五、验收清单（2026-07-17 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-17，子代理驱动）

| 阶段 | 验证结果 |
|------|---------|
| Phase 1 DAO：飞书 SDK（HTTP + WebSocket） | 13 个单元测试 PASS（token 缓存/刷新、消息发送、事件解析、重连） |
| Phase 2 DAL：find_by_lark_open_id / roles 过滤 | 5 个集成测试 PASS |
| Phase 3 Domain：find_reception_agent / find_lark_channel | 4 个集成测试 PASS |
| Phase 4 Consumer + AOP 中台：LarkEventDispatcher | 7 个集成测试 PASS（绑定用户/未绑定/agent_id 路由/tag 路由/无 Agent/群聊/非文本） |
| 后端 lib 全量测试 | 全部 PASS |
| Clippy（后端 + 前端 wasm32） | 双端零警告 |
| 手动端到端（飞书测试应用） | P2P 私信双向链路通过（发送/回复/断线重连） |

### 与计划的 2 处偏离（架构优化，业务零影响）
1. **v3 → v4 架构升级**：原 v3 方案 consumer 直接依赖 DAL，实现阶段评审发现仍违反分层约束，升级为 AOP 消息适配中台方案（consumer 只依赖中台，不碰具体 DAL）。文件数增加 2（pkg/aop/message_adapter/ 模块），分层约束严格满足
2. **ChannelConfig 字段扩展**：原计划仅加 lark_open_id，实现时同步加 lark_user_name（便于后台展示），未引入破坏性变更

---

## 七、后续扩展路径（4 步模板，按优先级）

> **核心不变量**：分层约束单向；外部渠道接入走 AOP 中台；用户必须预先绑定。

1. **飞书群聊消息（@机器人）接入**：
   - LarkDao event.rs：新增群聊事件解析分支（当前过滤丢弃）
   - entry：[lark/event.rs](../../src/service/dao/lark/event.rs)
   - Domain 路由：按 chat_id 绑定 project_id（需 MessageChannel 扩展 chat_id 字段）
2. **飞书富媒体/卡片消息支持**：
   - DAO：`im:resource` scope + upload/download 接口实现
   - entry：[lark/http.rs](../../src/service/dao/lark/http.rs)
   - 适配层：AdaptedMessage.content 富文本解析 / Interactive Card → 结构化工具调用
3. **企业微信 P2P 渠道接入**：
   - 完全复用 §4.1 新渠道接入 5 步模板
   - 参考 LarkDao 模式：[service/dao/lark/](../../src/service/dao/lark/)（HTTP API + 长连接管理）
   - ChannelConfig 扩展：新增 `wechat_openid` / `wechat_corp_id` 等字段
4. **Slack / Discord P2P 渠道接入**：
   - 同上 5 步模板，SDK 替换为 Slack/Discord Web API + Socket Mode
   - entry：[message_adapter/mod.rs](../../src/pkg/aop/message_adapter/mod.rs)（trait 实现入口）