---
kind: wiki_knowledge_card
name: InboundState 入站运行状态：动态游标 + 会话滚动刷新 + fail-open 游标损坏恢复
category: common models + dal message_channel + dao message_channel
scope:
  - "common/src/models/inbound_state.rs"
  - "src/service/dao/message_channel/sqlite.rs"（set_inbound_state / inbound_state 列相关 SQL）
  - "src/models/message_channel.rs"（MessageChannelPo.inbound_state 字段）
  - "migrations/20260906000001_add_inbound_state_to_message_channels.sql"
source_files:

  - common/src/models/inbound_state.rs#L15-L40（InboundState 主结构 + to_json/from_json fail-open）
  - common/src/models/inbound_state.rs#L59-L104（InboundCursor + CursorKind Opaque/Sequence/Timestamp/Offset 四种语义）
  - common/src/models/inbound_state.rs#L107-L193（InboundSessions：按 peer 组织 + context_token 滚动刷新 + 100 上限裁剪 retain_default）

  - src/models/message_channel.rs（MessageChannelPo.inbound_state 字段位置；add_inbound_state 迁移后新增）

  - migrations/20260906000001_add_inbound_state_to_message_channels.sql

  - src/service/dao/wechat/ilink.rs#L498-L504（from_json 解析 inbound_state：失败=无状态，fail-open）
  - src/service/dao/wechat/ilink.rs#L312-L344（InboundStateWriter 窄接口：save 整列覆盖写）
  - src/service/dao/wechat/ilink.rs#L451-L456（一次写回：有实际变化才落库，空轮询零写入）

  - docs/wiki/zh/content/功能模块/消息系统/微信%20iLink%20专属渠道.md

  - docs/wiki/knowledge/zh/消息渠道入站适配中台：MessageInboundAdapter%20trait%20+%20MessageAdapterRegistry%20全局注册%20+%20start_all%20stop_all%20生命周期/消息渠道入站适配中台：MessageInboundAdapter%20trait%20+%20MessageAdapterRegistry%20全局注册%20+%20start_all%20stop_all%20生命周期.md

  - docs/wiki/knowledge/zh/微信%20iLink%20专属渠道闭环：wechat_dal%20+%20ilink_dao%20+%20inbound_state%20+%20授权流程/微信%20iLink%20专属渠道闭环：wechat_dal%20+%20ilink_dao%20+%20inbound_state%20+%20授权流程.md

---

# InboundState 入站运行状态（动态游标 + 会话滚动刷新）

## §1 整体方案

消息渠道表 `message_channels` 新增 **TEXT 列 `inbound_state`**，用于持久化「渠道入站运行时动态信息」——与静态配置 `config_json` 物理隔离：**运行时循环只写本列，管理后台只写 config，互不覆盖**。

当前被 **微信 iLink 渠道**消费（get_updates_buf 不透明游标 + context_token 滚动刷新会话），未来企微、钉钉等其他增量拉取型渠道复用此模型，加同名同类型列即可。

**核心设计原则**：
- **协议解耦**：游标 kind 定义语义（Opaque/Sequence/Timestamp/Offset），不把协议特定字段硬编码到结构里
- **fail-open 游标损坏恢复**：`from_json` 解析失败返回 None → poll_loop 默认从空状态开始拉取 → 事件幂等键兜底（message_key）
- **动态会话按 peer 组织**：Vec 而非 HashMap（peer 数个位数量级，线性查找开销可忽略；Vec 顺序稳定，日志排障直观）
- **100 上限裁剪**：`SESSIONS_RETAIN_LIMIT = 100`，运行期超出时保留最旧 100 条

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| common/src/models/inbound_state.rs | 通用运行状态模型 | InboundState L16；InboundCursor L60 + CursorKind L44；InboundSessions L127 |
| src/models/message_channel.rs | 渠道 PO：inbound_state 字段 | MessageChannelPo.inbound_state（迁移后新增的 TEXT JSON 列） |
| migrations/20260906000001_add_inbound_state_to_message_channels.sql | DDL：inbound_state 列 | ALTER TABLE message_channels ADD COLUMN inbound_state TEXT |
| dao/wechat/ilink.rs#L312-L344 | InboundStateWriter 窄接口 | `async fn save(channel_id, state)` 整列覆盖写；失败仅 warn 不中断轮询 |
| dao/wechat/ilink.rs#L498-L504 | 启动时加载 | `channel.po.inbound_state.as_deref().and_then(InboundState::from_json).unwrap_or_default()` |
| dao/wechat/ilink.rs#L451-L456 | 一次写回 | 有实际变化（新游标或消息 > 0）才落库；空轮询零写入 |
| 【总卡】消息渠道入站适配中台 | 本卡是其 Level 4 细卡；总卡 §2 末列本卡路径 | 见本卡 source_files[] 尾总卡绝对路径 |
| 【平行卡】微信 iLink 专属渠道闭环 | 本卡是其运行时状态持久化基础卡 | 见本卡 source_files[] 尾微信 iLink 卡绝对路径 |
| 【① Wiki 长文】微信 iLink 专属渠道.md | inbound_state 在端到端链路中的位置 | docs/wiki/zh/content/功能模块/消息系统/微信%20iLink%20专属渠道.md |

## §3 架构约定

1. **InboundCursor.kind 必须与协议语义匹配**：iLink 的 get_updates_buf → Opaque（只能原样回传，不可比较，不可回退）；企微的序号游标 → Sequence（可比较，回退安全）。写代码时必须显式指定 kind。
2. **InboundSessions.upsert 的 None 字段语义**：`context_token: None` 表示"保留原值，不覆盖"；只有 `Some(value)` 才覆盖。这保证"入站消息只更新 context_token，但不丢失 last_message_id"。
3. **InboundStateWriter 窄接口隔离 DAO 依赖**：DAO 不依赖 MessageChannelDao 完整类型（DAO 禁止依赖其他 DAO），init 时注入 `Arc<dyn InboundStateWriter>`。生产实现委托 MessageChannelDao::set_inbound_state；测试注入内存实现。
4. **整列覆盖写而非部分更新**：`InboundStateWriter.save` 用整列覆盖写（`UPDATE ... SET inbound_state = ? WHERE id = ?`）。不做 JSON 部分更新——运行时循环自己管理 state 的完整内容，落库时一次性写。
5. **一次写回而非每条消息都写**：poll_loop 里每轮拉取结束（有新游标或有消息）才 save 一次。空轮询零写入——iLink hold 35s → 每 45s 一次空轮询 → 如果空轮询也写 → 每 45s 一次 DB 写 → 高并发多渠道场景下 IO 压力不必要。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止直接拼字符串游标**：游标内容 `InboundCursor.value` 是 `String` 字段，但外部消费者（如出站推送）**必须先检查 cursor.kind**，Opaque 类型禁止比较大小、禁止生成"比当前大 N"的游标。只有 Sequence/Timestamp/Offset 语义才允许计算相对偏移。
2. ❌ **禁止 inbound_state JSON 损坏时 panic 或阻断轮询**：`from_json` 返回 None → 等价"无状态，从头开始"（fail-open）。运行态丢失最多损失"上次游标的重复消息"——iLink 的 get_updates_buf 丢了 = 回退到第一条消息，靠 message_key 幂等键兜底。
3. ✅ **InboundSessions.retain_default 必须在每次写回前调用**：`SESSIONS_RETAIN_LIMIT = 100` 硬编码上限，运行期超出时丢弃最旧的。禁止注释掉 retain_default 调用来"解决"会话多问题——正确做法是排查是否某 peer 每帧都被当作新 peer（peer_id 格式问题）。
4. ✅ **InboundStateWriter.save 失败必须 warn 不中断**：轮询循环里 `if let Err(e) = writer.save(...) { log_warn! }`。DB 短暂不可用 → 轮询继续 → 状态可能落后一次 → DB 恢复后下次写回补上。禁止把 DB 错误冒泡到轮询循环外层导致整个通道挂掉。
5. ✅ **整列覆盖写必须带 WHERE id = ? 精确命中**：`UPDATE message_channels SET inbound_state = ? WHERE id = ?`。禁止 `UPDATE ... SET inbound_state = ? WHERE channel_type = 'wechat'` 批量覆盖——多 bot 渠道同时轮询时会互相覆盖。

---

# 本卡 Level 4 声明（AGENTS §2.1.3.4 总卡-细卡）

- scope[本卡] ⊂ scope[消息渠道入站适配中台卡] 的消息持久化/运行状态管理子域 → 总卡描述 MessageInboundAdapter trait + Registry 生命周期，本卡描述各渠道复用的通用运行状态持久化模型
- 关联声明位置：
  - 总卡 §2 末列本卡相对路径（需在总卡 Step 4 更新时追加）
  - 本卡 source_files[] 追加总卡相对路径 ✓
  - 双方 Wiki 长文 cite 段互链（Step 4 + Step 6 完成）
