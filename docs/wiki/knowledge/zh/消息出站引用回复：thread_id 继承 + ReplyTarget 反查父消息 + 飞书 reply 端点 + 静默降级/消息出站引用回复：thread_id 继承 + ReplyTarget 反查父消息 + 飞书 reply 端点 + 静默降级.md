---
kind: RAG 原子知识卡
name: 消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级
category: 业务模块 / 消息系统
scope:
- src/service/domain/message/delivery*.rs
- src/service/dal/message_channel*.rs
- src/service/dao/lark/http.rs
- src/service/dao/lark/mod.rs
source_files:
- src/service/domain/message/delivery.rs#L539-L568
- src/service/domain/message/delivery.rs#L208-L293
- src/service/domain/message/delivery.rs#L322-L355
- src/service/dal/message_channel.rs#L664-L693
- src/service/dal/message_channel.rs#L473-L498
- src/service/dao/lark/mod.rs#L96-L140
- src/service/dao/lark/http.rs#L158-L215
- src/service/dao/lark/http.rs#L315-L360
- docs/plan/飞书出站引用回复方案.md
- docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息处理核心.md
- docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md
- docs/wiki/knowledge/zh/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md
---

## §1 概述

Agent 回复用户消息时，若该用户绑定飞书渠道，出站投递走**飞书引用回复端点**（`POST /open-apis/im/v1/messages/:message_id/reply`），使回复以「引用」形式挂在原消息下；父消息属于话题（`omt_` 前缀）时以话题形式回复（`reply_in_thread=true`）。非飞书渠道、无父消息、前缀不符、键为空等场景**静默降级**为普通发送端点，绝不影响投递成功率。

- **三段协作**：Domain（`delivery.rs`）按出站消息 `reply_to_id` 反查父消息一次，产出渠道无关的 `ReplyTarget{external_key, thread_id}`；DAL（`message_channel.rs`）的 Lark 分支把它解析为飞书原生 `LarkReply{message_id, in_thread}`；DAO（`dao/lark/http.rs`）按 `reply` 有无在「回复端点」/「发送端点」间二选一。
- **创建与推送两段异步**：`send_to_user`/`send_to_agent` 只落库，`MessageConsumer` 重新 fetch 消息后走 `Domain::deliver_message`，父消息上下文无法在内存跨阶段传递，因此推送时**现场主键点查父消息一次**（成本极低）。
- **thread_id 继承**：出站消息复用同一次父消息查询顺带继承 `thread_id` 并落库（`send_to_agent` 链式回复回退继承），供后续话题内回复继续走话题端点。

## §2 关键文件与职责表

| 角色 | 文件 | 关键锚点 |
|------|------|---------|
| Domain 出站投递反查父消息 | src/service/domain/message/delivery.rs | L539-L568 `deliver_message`：按 `reply_to_id` 主键点查父消息一次，取非空 `external_key` + `thread_id` 构造 `ReplyTarget`；查不到 / 无外部键 → `None`，透传给渠道 DAL |
| Domain 出站 thread_id 继承 | src/service/domain/message/delivery.rs | L208-L293 `send_to_agent`（同一次父消息查询顺带继承 `thread_id`，链式回复回退）；L322-L355 `send_to_user`（`po.thread_id = inherited_thread_id` 落库） |
| DAL 渠道无关回复目标 + 解析纯函数 | src/service/dal/message_channel.rs | L664-L693 `ReplyTarget{external_key, thread_id}` + `resolve_lark_reply`（`strip_prefix("lark:")` 取 `om_xxx`，空键/非 lark 前缀 → None；`thread_id` 存在 → `in_thread=true`） |
| DAL 出站编排器 Lark 分支 | src/service/dal/message_channel.rs | L473-L498 `push_to_channel`：`deliver_message`（L133-L138 trait / L273-L278 impl，第 4 参 `Option<ReplyTarget>`）透传 → Lark 分支 `resolve_lark_reply` → `lark_dao.push(reply)` |
| DAO 飞书回复参数 | src/service/dao/lark/mod.rs | L96-L140 `LarkReply{message_id, in_thread}` + `LarkDao::push` 第 5 参 `reply: Option<&LarkReply>` |
| DAO 飞书回复端点与分支 | src/service/dao/lark/http.rs | L158-L215 `reply_text_message`（`PATH_REPLY_MESSAGE` 常量 L31）；L315-L360 `push` 按 `reply` 在回复端点 / 发送端点二选一，日志新增 `reply_to=` |
| §2 Wiki 长文（消息处理核心） | docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息处理核心.md | Domain delivery 链路：发送—投递—SSE |
| §2 Wiki 长文（消息渠道适配器） | docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md | DAL 出站编排 + 飞书出站推送 |
| 主卡（消息交互与SSE推送） | docs/wiki/knowledge/zh/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md | Level 2 主卡：MessageDomain 双能力 + 多渠道出站总览 |

## §3 架构与设计约定

本卡为主卡《消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类》之下的 **Level 2 子卡**：主卡负责用户↔Agent 消息交互、SSE 推送与 5 类渠道出站的总览与总红线，本卡聚焦「出站引用回复」这一具体出站语义（飞书回复端点 / 话题内回复 / 静默降级）及其跨 Domain→DAL→DAO 的实现路径。

```
出站消息（Agent → 用户）
  ↓ Domain::deliver_message
  reply_to_id? ──否──▶ reply_target = None
       │是
       └─ message_dal.find_by_id(parent_id)（主键点查一次）
             ├─ 父消息 external_key 非空 ──▶ ReplyTarget{external_key, thread_id}
             └─ 无父消息 / external_key 空 ──▶ None
  ↓ MessageChannelDal::deliver_message(reply_target)
  push_to_channel(仅 Lark 分支使用 reply_target)
       └─ resolve_lark_reply(reply_target)
             ├─ Some(reply) ──▶ lark_dao.push(reply) ──▶ reply_text_message(reply_in_thread)
             └─ None ──────────▶ lark_dao.push(None)  ──▶ send_text_message（静默降级）
```

约定：
- **回复目标 = 父消息的 `messages.external_key`**（形如 `lark:om_xxx`，出站推送成功后由 DAL 回写）；`thread_id`（`omt_` 前缀）来自父消息话题键。
- **渠道无关解析与渠道相关传输分离**：Domain 只产出渠道无关的 `ReplyTarget`，飞书专属的「剥离前缀 / 话题判定」落在 DAL 的 `resolve_lark_reply` 纯函数。
- **DAO 只接收飞书原生参数**：`LarkReply{message_id, in_thread}` 可直接用于回复端点；`reply=None` 时走普通发送，行为与旧版等价。
- **thread_id 落库便于延续话题**：出站回复继承父消息 `thread_id`，使下一轮在话题内的回复仍走话题端点。

## §4 硬约束与回归红线

1. **父消息上下文必须推送时现场反查一次**：创建（`send_*`）与推送（`deliver_message`）是两段异步，父消息上下文无法在内存跨阶段传递；仅按 `reply_to_id` 主键点查一次，禁止多次查库或缓存在消息 PO 上。
2. **回复目标即父消息 `external_key`，不改其写入口径**：`external_key` 仍由 DAL 推送成功后回写（`lark:{om_xxx}`）；本能力只读不改，任何改写写入口径的改动视为回归。
3. **话题语义必须显式传 `reply_in_thread`**：父消息 `thread_id` 存在（`omt_` 前缀）即置 `in_thread=true`；出站消息继承的 `thread_id` 必须落 `messages.thread_id`，使话题链延续。
4. **静默降级是硬约定**：无目标 / 前缀非 `lark:` / 键为空（`lark:`）/ 父消息无 `external_key` → `resolve_lark_reply` 返回 `None` → 走普通发送端点，绝不允许因此报错或中断投递。
5. **`resolve_lark_reply` 必须是纯函数且拒绝空 id**：校验 `strip_prefix("lark:")` 后非空才产出 `LarkReply`（避免空路径参数），纯函数以便单测（`message_channel.rs` 内联 5 用例）。
6. **仅 Lark 渠道实现引用回复**：微信 / Slack / Email / Webhook / A2A 分支必须忽略 `reply_target`（不引用），保持现状；新增渠道分支同样不得擅自使用回复语义。
7. **仅文本消息**：媒体 / 卡片 / 富文本的引用回复不在范围，`reply_text_message` 只发 `msg_type="text"`。
8. **DAO `push` 新增第 5 参不得破坏旧行为**：`reply=None` 时与普通发送端点完全等价；调用点必须显式传参（编译期保证），测试调用点补 `None`。