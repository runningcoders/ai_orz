# 微信 iLink 协议对齐重构方案

> 🎯 **定位**：以腾讯官方插件 `@tencent-weixin/openclaw-weixin` 的类型定义为协议 SSOT，逐字段核对现有 iLink 实现，给出分期重构方案（含待拍板决策）
> 状态：草稿（讨论中，未拍板；拍板后再转 Task 执行）
> 触发场景：修改 iLink 收发字段 / 排查"扫码成功却收不到消息" / 协议行为对不上官方时打开
>
> 关联文档：
> - [微信渠道接入设计](../design/wechat_channel_integration_design.md) — 分层落点与阶段划分（§5.1/§5.2 协议小节须按本文更正）
> - [CODE_STANDARDS.md](../CODE_STANDARDS.md) — 编码规范 SSOT
> - [微信 iLink 专属渠道](../wiki/zh/content/功能模块/消息系统/微信%20iLink%20专属渠道.md) — 运行态与观测四件套

---

## 一、结论先行

**我们对 iLink 协议的理解在多个关键字段上是错的**，其中三条足以单独导致「扫码绑定成功、连通性测试通过，却一条消息也收不到」。因此本次不是"增强"，而是**协议对齐重构**。

严重度分布：

| 级别 | 含义 | 条数 |
|------|------|------|
| **P0** | 直接决定收发能不能通 | 7 |
| **P1** | 能通但不会自愈 / 排查黑洞 | 5 |
| **P2** | 扫码登录状态机不完整 | 5 |
| **P3** | 能力面（媒体/引用/typing），本次不做，仅登记 | 5 |

一句话概括 P0 的三条致命项：

1. 文本字段名写成 `content`，官方是 **`text`** → `IlinkMessage::text()` 恒返回 `None`；
2. `message_type` / `message_state` 声明成 `String`，官方是**数字** → serde 反序列化整条失败，而失败被 `.ok()` 吞掉 → 收帧为 0（连日志都不打）；
3. `base_info` 与 `iLink-App-Id` / `iLink-App-ClientVersion` 请求头**一个都没发**。

> 这解释了上一轮为什么把症状误判为"消息根本没到 iLink"：真正的丢帧点在**解析层**，
> 发生在 publish 之前，比适配层的丢弃点更靠前，因此连后加的 `adapted to nothing` 日志都盖不住。

---

## 二、取证与依据

### 2.1 协议 SSOT：腾讯官方插件

`@tencent-weixin/openclaw-weixin` 是**腾讯官方**发布的微信渠道插件（`author: Tencent`），其 `src/api/types.ts` 是 proto（`GetUpdatesReq/Resp`、`WeixinMessage`、`SendMessageReq`）的 TypeScript 镜像，**等价于协议 spec**，可信度远高于此前依据的社区整理文章。

可复现获取（本机已验证）：

```bash
curl -sSL "https://registry.npmjs.org/@tencent-weixin/openclaw-weixin/-/openclaw-weixin-2.4.9.tgz" -o ocw.tgz
tar xzf ocw.tgz   # → package/src/**
```

已核对的权威文件（本次结论全部出自这几处）：

| 文件 | 承载的协议面 |
|------|-------------|
| `src/api/types.ts` | 全部 DTO 与枚举（`TextItem` / `MessageType` / `MessageState` / `MessageItemType` / `GetUpdatesResp` …）|
| `src/api/api.ts` | 传输层：请求头、`base_info` 注入点、三档超时、错误码判定、uint64 无损解析 |
| `src/monitor/monitor.ts` | 长轮询循环：失败退避、`-14` 暂停、超时协商、游标持久化 |
| `src/messaging/send.ts` | 出站请求体构造 |
| `src/messaging/inbound.ts` | 入站解析：文本提取、`context_token` 持久化 |
| `src/auth/login-qr.ts` | 扫码登录完整状态机（8 态）|
| `src/api/session-guard.ts` | `STALE_TOKEN_ERRCODE = -14` + 1 小时暂停 |
| `src/storage/sync-buf.ts` | 游标落盘（`{accountId}.sync.json`）|
| `package.json` | `version = "2.4.9"`、**`ilink_appid = "bot"`** |

### 2.2 独立实现互证：Hermes

`NousResearch/hermes-agent` 的 `gateway/platforms/weixin.py`（72 KB）是 OpenClaw 之外的独立实现，关键字段与官方插件**完全一致**（`text`、数字 `message_type`、`get_updates_buf`、`base_info`），两个独立实现互证，排除"官方类型定义写得比实现宽"的误判可能。

取用方式（`raw.githubusercontent.com` 被限流，走 contents 接口）：

```bash
curl -sSL -H "Accept: application/vnd.github.raw" \
  "https://api.github.com/repos/NousResearch/hermes-agent/contents/gateway/platforms/weixin.py"
```

### 2.3 残留不确定点（需一次性实证）

我们手上只有**类型定义**，没有真实帧。`message_type` 线上究竟发数字还是字符串，理论上仍有"实现比类型窄"的可能。但**修法不依赖这个答案**：按本仓既有的宽容解析约定做成「数字 / 字符串双形态都收」，两种线上形态都不会挂。

为一次性消除悬念，阶段 A 附带一条**原始响应体留痕**（debug 级、截断、脱敏），首轮真机收帧即可确认。

---

## 三、差异全清单

### 3.1 P0 —— 决定收发能不能通

| # | 协议点 | 官方（2.4.9）| 我们 | 后果 |
|---|--------|-------------|------|------|
| 1 | 文本字段名 | `text_item.text` | `text_item.content` | 入站 `text()` 恒 `None`；出站正文发不出去 |
| 2 | `message_type` | 数字 `1=USER 2=BOT`（`0=NONE`）| `String`，比较 `"USER"` | serde 遇数字反序列化 String **整条失败** → 收帧为 0 |
| 3 | `message_state` | 数字 `2=FINISH`（`0=NEW 1=GENERATING`）| `String`，比较 `"FINISH"` | 同上 |
| 4 | `base_info` | **每个消息面 POST** 都带 `{channel_version, bot_agent}` | 完全没发 | 服务端可能据此判客户端兼容性 |
| 5 | `iLink-App-Id` 头 | `bot`（取自 `package.json.ilink_appid`）| 未发 | 客户端身份缺失 |
| 6 | `iLink-App-ClientVersion` 头 | `0x00MMNNPP` 十进制（2.4.9 → `132105`），**GET/POST 全带** | 未发（仅扫码状态错发 `1`）| 版本协商失效 |
| 7 | `X-WECHAT-UIN` 头 | `base64(随机 uint32 的十进制字符串)`，**仅 POST** | `base64(小端原始 4 字节)` | 头格式不符 |

### 3.2 P1 —— 能通但不会自愈 / 排查黑洞

| # | 协议点 | 官方 | 我们 | 后果 |
|---|--------|------|------|------|
| 8 | `getupdates` 响应错误码 | 校验 `ret`/`errcode`；`-14` 暂停 1h；3 连败退避 30s | **完全不校验**，一律当空轮次 | 会话过期后永不恢复，且日志与"没人发消息"同形 |
| 9 | `longpolling_timeout_ms` | 采纳服务端建议超时 | 忽略 | 服务端调整 hold 时长后我方口径漂移 |
| 10 | `notifyStart` / `notifyStop` | 渠道启停各调一次（`ilink/bot/msg/notifystart` `/notifystop`）| 未实现 | 服务端不知客户端在册 |
| 11 | 解析失败留痕 | 异常路径均有日志 | `.ok()` 吞 + 丢弃点 `log_debug!` | **排查黑洞**：症状与"上游没数据"完全同形 |
| 12 | 顶层 `message_id` | 优先读 `message_id`（uint64，字符串无损），缺省回落 item `msg_id` | 只读 `client_id` + 顶层 `msg_id` | 官方 schema 中 `msg_id` 在 **item** 上，我方顶层 `msg_id` 大概率永不命中 → 幂等键实际只有 `client_id` 生效 |

### 3.3 P2 —— 扫码登录状态机

| # | 协议点 | 官方 | 我们 | 后果 |
|---|--------|------|------|------|
| 13 | `get_bot_qrcode` | **POST** + body `{local_token_list}`（本地已有 bot token 最多 10 个）| GET 无 body | 无法重绑已有 bot；每次扫码都走新建 |
| 14 | 状态枚举 | **8 态**：`wait` / `scaned` / `confirmed` / `expired` / `scaned_but_redirect` / `need_verifycode` / `verify_code_blocked` / `binded_redirect` | 4 态，未知 → `Wait` | IDC 重定向场景**一直空转到超时**；配对码挑战无法处理 |
| 15 | `verify_code` 回传 | `&verify_code=` 附在状态查询上 | 无 | `need_verifycode` 无解 |
| 16 | `expired` 处理 | 自动换码，最多 3 次 | 直接透传给前端 | 交互缺口（见决策 D7）|
| 17 | 扫码请求头 | 通用头两件套 | 错发 `iLink-App-ClientVersion: 1` | 值错误 |

### 3.4 P3 —— 能力面（本次不做，登记为后续）

| # | 协议点 | 官方 | 我们 | 结论 |
|---|--------|------|------|------|
| 18 | 媒体消息 | 图片/语音/文件/视频 + CDN 上传下载 + AES-128-ECB | 仅文本 | 阶段一既定范围，不做 |
| 19 | 引用消息 | `ref_msg` + quote store + 局部引用 | 无 | 不做 |
| 20 | 输入态 | `sendtyping` + `getconfig`（拿 `typing_ticket`）| 无 | 不做 |
| 21 | 会话字段 | `group_id` / `session_id` / `run_id` / `seq` | 无 | 不做（官方文档已确认 iLink bot 收不到普通群事件）|
| 22 | 本地去重 | **插件本身不做去重**，交上层核心（`MessageSidFull`）| AOP `message_key` 幂等 + 游标确认 | **我方口径更优，无需对齐** |

> 第 22 条是本次的一个"意外收获"：官方把去重责任推给宿主，等于承认协议层无法保证不重。我方 `message_key` 幂等 + 「确认才推进游标」正是官方缺失的那一层，属**领先而非落后**，不要在重构中削弱它。

---

## 四、重构方案

### 阶段 A（P0：让链路真的能通）

**A1. 协议常量收敛为一处 SSOT**
新增常量：`MessageType`(0/1/2)、`MessageState`(0/1/2)、`MessageItemType`(0…12)、`ILINK_APP_ID = "bot"`、
`ILINK_APP_CLIENT_VERSION`（由版本号按 `0x00MMNNPP` 计算）、`base_info` 构造与请求头构造。
落点见决策 **D1**。

**A2. 文本字段双读**
`IlinkTextItem` 以 `text` 为主、`content` 作兼容别名字段读取；**出站只发 `text`**（以官方 spec 为准，不双发）。

**A3. `message_type` / `message_state` 数字为主、双形态兼容**
引入带自定义 `Deserialize` 的轻量新类型（数字主 + 字符串兼容），谓词 `is_user()` / `is_finished()` 改为按数字判定；出站恒定输出数字。

**A4. `base_info` 注入**
消息面所有 POST（`getupdates` / `sendmessage`）带上；**扫码两接口不带**（官方亦未带）。

**A5. 请求头对齐**
`auth_headers` 补齐 `iLink-App-Id` + `iLink-App-ClientVersion`；`X-WECHAT-UIN` 改为 `base64(十进制字符串)`；
扫码两接口补通用头两件套、去掉错误的 `iLink-App-ClientVersion: 1`。

**A6. 实证留痕（一次性）**
`parse_updates` 加 debug 级、截断脱敏的原始响应体日志（首轮真机确认线上形态后即可摘除或降噪）。

### 阶段 B（P1：自愈与可见性）

**B1.** `IlinkUpdates` 扩出 `ret` / `errcode` / `errmsg` / `longpolling_timeout_ms`，并做错误码分类
（`-14` StaleToken / `-2` RateLimit / 其他）。

**B2.** 会话暂停：`-14` → 该 channel 暂停请求 1 小时；暂停期轮询休眠、出站快速失败并给出可读原因；
`state` 暴露 `paused` + `paused_until_ms` 进 `wechat_poll` 指标（监控面板加一态）。

**B3.** 超时协商：客户端超时 = `longpolling_timeout_ms` + 余量（默认 `35s + 10s = 45s`，与现值一致）；
客户端超时从 `warn` 降级为计数 + `debug`（官方视其为**正常控制流**）。

**B4.** `notifyStart` / `notifyStop`：循环启停各调一次，失败仅告警。

**B5.** 解析留痕：`parse_updates` 不再 `.ok()` 静默——单条失败 `warn` 并带消息键摘要；
`service/dal/wechat/impl.rs` 两个丢弃点 `log_debug!` → `log_info!`。

**B6.** `message_key()` 补顶层 `message_id`（字符串优先），回落 `client_id` / item `msg_id`。

### 阶段 C（P2：扫码状态机）

**C1.** `get_bot_qrcode` 改 POST + `local_token_list`（该用户已有 `WechatIlink` 凭据的 `bot_token`，最多 10 个）。
**C2.** `IlinkQrStatusKind` 扩到 8 态；状态 DTO 增 `verify_code` 入参与 `redirect_host` 出参。
**C3.** `scaned_but_redirect` → 切 `https://{redirect_host}` 继续轮询。
**C4.** `binded_redirect` → 视为成功（幂等，不新建凭据）。
**C5.** `need_verifycode` → 前端补配对码输入框；`verify_code_blocked` → 明确提示。
**C6.** `expired` 处理见决策 **D7**。

> 阶段 C 的 C1 + C4 是一组：没有 C1 的 `local_token_list`，服务端无从知道你已绑过这个 bot，
> 也就永远不会回 `binded_redirect`，C4 便是死代码。

---

## 五、待拍板决策

> 这是本轮讨论的核心。每项给了推荐值，但需要你确认后我再动手。

| # | 决策 | 选项 | 我的建议 |
|---|------|------|---------|
| **D1** | 协议常量落点 | ① `src/models/events/wechat.rs`（与 DTO 同文件）② `src/pkg/wechat_ilink.rs`（协议基建）③ `common` | **②**。配置面与消息面两个客户端都要用；`models` 已依赖 `pkg::aop`（`impl crate::pkg::aop::Event`），依赖方向无新增；`common` 是前后端共享层，此处无前端诉求 |
| **D2** | `channel_version` / `bot_agent` 填什么 | ① 镜像官方值（`2.4.9` / `OpenClaw`）② 我方标识（`ai_orz/<ver>`）| **②**。官方注释明确 `bot_agent`"仅用于观测，不参与鉴权与路由"、缺省即 `OpenClaw`；镜像官方属无必要冒名。`channel_version` 单独注释其语义（我方渠道实现版本）|
| **D3** | 客户端超时口径 | ① 保持固定 45s（> 服务端 hold，超时=异常）② 对齐官方固定 35s（超时=常态）③ 服务端建议值 + 10s 余量 | **③**。既采纳 `longpolling_timeout_ms`，又保住"客户端超时仍是异常信号"的监控语义 |
| **D4** | `-14` 暂停状态存哪 | ① 进程内（per-channel）② 落 `inbound_state` | **①**。暂停是自愈手段，重启后重试即恢复；落库会多出一处"必须两端闭合"的持久化运行态 |
| **D5** | 出站是否也拦暂停 | ① 拦（官方 `assertSessionActive` 口径）② 不拦 | **①**。省一次必然失败的请求，并给用户可读原因而非超时 |
| **D6** | `need_verifycode` 是否做 UI | ① 做配对码输入框 ② 只提示"请重新生成二维码" | **①**。这是风控/IDC 场景的唯一出路，不做等于该场景卡死 |
| **D7** | `expired` 自动换码 | ① 服务端有状态会话 + 自动换码（官方做法）② 保持无状态，前端重取（现状）③ 后端返回"已过期"，前端自动重取一次（前端重试策略）| **③**。① 要给 handler 引入扫码会话状态，成本与收益不匹配（YAGNI）；② 已有交互，③ 只是加一层前端自动重试 |
| **D8** | 顶层 `message_id` 是否进 `messages.external_key` | ① 只用于 `message_key` ② 一并打通 `external_key`（微信链路不再是 `None`）| **①**。`external_key` 语义变更会波及落库口径与飞书/邮件一致性，属另一件事 |

---

## 六、涉及文件清单

| 文件 | 角色 | 变更 |
|------|------|------|
| `src/pkg/wechat_ilink.rs` | 配置面协议客户端（pkg）| 协议常量 + 头/`base_info` 构造（D1）；扫码改 POST + `local_token_list` + 8 态 + `verify_code` |
| `src/models/events/wechat.rs` | 消息面 DTO + AOP 事件（models）| `text`/`content` 双读；`message_type`/`state` 双形态；顶层 `message_id`；谓词改数字判定 |
| `src/service/dao/wechat/ilink.rs` | 消息面客户端 + 长轮询循环（DAO）| 出站体对齐；错误码/超时协商/暂停/`notifyStart`；解析留痕；`message_key` |
| `src/service/dao/wechat/http.rs` | DAO 门面 | 暂停期拦截出站 `push`（D5）|
| `src/service/dal/wechat/impl.rs` | 入站适配 | 丢弃点 `log_debug!` → `log_info!` |
| `common/src/api/`（微信相关 DTO）| 接口契约 | 扫码状态扩展 + `verify_code` 入参 + `redirect_host`；`wechat_poll` 增 `paused` 态 |
| `src/handlers/finance/wechat_integration/login_status.rs` | handler | `verify_code` 透传 + 新状态文案 |
| `src/service/domain/finance/identity_credential.rs` | 凭据编排 | `local_token_list` 构造；`binded_redirect` 幂等；轮换口径复核 |
| `frontend/src/pages/finance/identity_wechat.rs` | 前端扫码弹窗 | 配对码输入 / 重定向提示 / 过期重取（D6/D7）|
| `docs/design/wechat_channel_integration_design.md` | 设计 SSOT | §5.1/§5.2 字段表按官方 spec 更正，并登记协议来源为官方插件 |
| `docs/wiki/zh/content/功能模块/消息系统/微信 iLink 专属渠道.md` | Wiki | 同步协议口径与故障排查 |
| `docs/wiki/knowledge/zh/微信 iLink 专属渠道闭环…` | RAG 卡 | 同步 |

### 零改动面（回归必保）

- **AOP 事件链路**：`topic` / `order_key` / 消费契约 / `notify_producer` 一律不动；
- **入站游标 P2 机制**：`CursorStore` + `on_consumed` 确认推进**是领先设计**（见差异 #22），不削弱；
- **监控五层聚合**：DAO → DAL → Domain → handler → 前端链路不变，仅新增"暂停"态；
- **飞书 / 邮件渠道**、**消息投递与身份分层**、**Agent 路由档位链**：零改动。

---

## 七、验收清单

- [ ] `cargo test --lib` 全绿，且新增：数字/字符串双形态解析、`text`/`content` 双读、出站体快照（含 `base_info` 与数字 `message_type`）
- [ ] `make ci`（fmt-check + clippy + clippy-fe + docs-lint）全绿
- [ ] **真机端到端**：微信里向 bot 发一条文本 → 日志出现 `ilink inbound batch count=1` → `messages` 表落库 → bot 回复成功送达微信
- [ ] **重启续拉**：重启进程，首轮日志 `resume_cursor=` 非空（游标回灌生效）
- [ ] **监控判活**：健康页「微信长轮询」面板 `rounds` / `last_poll_at_ms` 持续推进
- [ ] **错误码可见**：服务端返回 `ret != 0` 时日志明确区分错误码，而非静默空轮次

---

## 八、计划偏离登记

_（执行阶段回填）_
