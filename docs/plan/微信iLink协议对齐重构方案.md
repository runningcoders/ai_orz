# 微信 iLink 协议对齐重构方案

> 🎯 **定位**：以腾讯官方插件 `@tencent-weixin/openclaw-weixin` 的类型定义为协议 SSOT，逐字段核对现有 iLink 实现，给出分期重构方案（含待拍板决策）
> 状态：已拍板（D1–D8 定稿，见 §5；按阶段 A → B → C 执行）
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
| 12 | 顶层 `message_id` | 顶层 `message_id` 为服务端权威消息 ID（uint64，字符串无损），`client_id` 为对端客户端生成；两者的 **`msg_id` 在 `MessageItem` 上**（item 级）| 顶层 `msg_id: Option<Value>` + `client_id` | **字段错位**：官方顶层没有 `msg_id`，我方顶层 `msg_id` 大概率永不命中 → 幂等键实际只有 `client_id` 生效；且微信链路 `external_key` 恒 `None`（见 §3.5）|

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

### 3.5 协议面之外的两处互通缺口

这两条不属于"字段写错"，但同样影响可用性与可运维性，且都有官方参照。

**（1）二维码的备用授权链接从未暴露给用户 —— 产品缺口**

`get_bot_qrcode` 返回的 `qrcode_img_content` **本身就是一条 URL**（实测形如 `https://liteapp.weixin.qq.com/q/...`），
它既可以被编码成二维码让人扫，也可以**直接在手机微信里打开完成授权**。官方 `displayQRCode` 就带这条兜底：

```text
若二维码未能显示或无法使用，你可以访问以下链接以继续：
{qrcodeUrl}
```

我方前端只把它编码成二维码图像（`qr_img_src`），**链接本身没有以任何形式呈现**。
后果：用户不在电脑前、屏幕太小看不清、或二维码渲染失败时，**没有任何替代路径**。

> 这与"自动换码"针对的是**不同场景**：自动换码解决"码在电脑屏幕上、人回来时它还没过期"，
> 备用链接解决"人根本看不到这块屏幕"。见 §5.1 Q2。

**（2）微信链路 `messages.external_key` 恒为 `None` —— 数据面缺口**

`service/dal/wechat/impl.rs:236` 的注释写着「iLink 协议无线程/回复字段，无外部键可映射」——
这个结论**已被官方类型定义推翻**：顶层 `message_id` 就是权威平台侧 ID，出站 `SendMessageResp.message_id` 亦然。

当前后果：
- `messages` 表里微信消息**没有平台侧 ID 可对账**，跨渠道排障时缺一环；
- 飞书有 `lark:om_xxx`、邮件有 `email:<Message-ID>`，只有微信是空的，口径不齐；
- 未来若做引用消息（P3 #19），缺少现成的映射基础。

> ⚠️ 语义边界：`external_key` 最初的用途是"入站回复按平台 `parent_id`/`root_id` 反查父消息"
> （见 `migrations/20260909000004_add_external_key_to_messages.sql`），那是**飞书线程场景**。
> 微信没有 `parent_id`/`root_id` 字段，因此微信侧打通后它承载的是**"渠道消息平台 ID 通用存档"**
> 这一扩展语义，**不承担反查父消息的职责**。该边界写在 `AdaptedMessage.external_key` 的字段文档中
> （**不改历史迁移文件、不动表结构**），避免后人误判。

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

**B6.** `IlinkMessage` 补顶层 `message_id`（`Option<Value>`，数字/字符串双形态），
`message_key()` 优先级改为 `message_id` → `client_id` → item `msg_id`（`client_id` 保留为出站本地幂等键）。

**B7.** `external_key` 打通（决策 D8，含入站与出站两侧）：
- **入站**：`adapt_wechat` 填 `external_key = Some(format!("wechat:{}", message_id))`
  （`message_id` 缺失时回落 `client_id`；两者皆无则仍为 `None`，不伪造）；
- **出站**：`WechatChannelDao::push` 返回类型 `Result<()>` → `Result<Option<String>>`
  （携服务端 `SendMessageResp.message_id`），在 `dal/message_channel.rs::push_to_channel`
  的 `ChannelType::Wechat` 分支**照抄飞书分支**回写 `set_external_key(ctx, &message.po.id, "wechat:{id}")`，
  回写失败仅告警不阻断（与飞书口径一致）。

**B8.** `AdaptedMessage.external_key` 字段文档（`src/pkg/adapter/mod.rs`）同步扩展语义
（见 §3.5(2) 的边界说明）：微信侧承载"渠道消息平台 ID 存档"，**不承担反查父消息**职责。
**不动历史迁移文件、不改表结构**（`external_key` 已是普通索引、无唯一约束，新增写入无需 DDL）。

### 阶段 C（P2：扫码状态机）

**C1.** `get_bot_qrcode` 改 POST + `local_token_list`（该用户已有 `WechatIlink` 凭据的 `bot_token`，最多 10 个）。
**C2.** `IlinkQrStatusKind` 扩到 8 态；状态 DTO 增 `verify_code` 入参与 `redirect_host` 出参。
**C3.** `scaned_but_redirect` → 切 `https://{redirect_host}` 继续轮询。
**C4.** `binded_redirect` → 视为成功（幂等，不新建凭据）。
**C5.** `need_verifycode` → 前端补配对码输入框；`verify_code_blocked` → 明确提示。
**C6.** `expired` 处理（决策 D7）：**前端自动长轮询 + 过期自动换码（上限 3 次）**，
并**取代上一轮临时引入的「我已扫码完成」手动按钮**。

落地要点（照抄 `frontend/src/pages/finance/identity.rs` 的飞书绑定轮询样板，避开上一轮的坑）：

- **必须平铺 spawn**：轮询 `loop` 在点击回调内**直接** spawn，`loop` 体内只管 `await`。
  ⚠️ 上一轮"自动轮询不工作"的根因是**嵌套 spawn**（外层 async 内再 spawn，拿不到 Dioxus 作用域）；
  飞书绑定轮询用同样结构且工作正常，证明平铺写法可行。
- **循环体天然就是长轮询**：`poll_wechat_login_status` 单次调用服务端 hold ~35s（客户端超时 45s），
  故**不需要额外 `sleep`**，`loop` 体即「查询 → 处理 → 再查询」。
- **卸载守卫**：`Rc<Cell<bool>>`，组件卸载 / 关闭弹窗时置 `false`（同飞书 `bind_poll_running`）。
- **换码动作**：`expired` → 重新调 `get_wechat_login_qrcode`，就地替换 `qr_id` / `qr_img` / 重置 `qr_stage`，
  刷新计数 +1；超过 3 次 → 停止轮询并提示「二维码多次失效，请关闭后重试」
  （对齐官方 `MAX_QR_REFRESH_COUNT`）。
- **在途查询作废**：换码后旧循环返回的响应必须自检 `qr_id` 不匹配即丢弃（现有代码已有该保护，保留）。
- **换码状态展示**：弹窗内显示「二维码已自动刷新 x/3」与阶段徽章
  （等待扫码 / 已扫码待确认 / 刷新中 / 已授权）。
- **去掉手动按钮**：confirmed 由轮询自动捕获，弹窗自动切「已授权」形态。
  这是本项最大的体验收益——上一轮的"必须手动点一下才拉取"是绕开嵌套 spawn 的权宜之计，不是设计。

> 官方参数参照：`ACTIVE_LOGIN_TTL_MS = 5min`（会话 TTL）、`MAX_QR_REFRESH_COUNT = 3`、轮询窗口 8min。
> 我方**服务端二维码实际 TTL 未实测**，阶段 C 落地时以真机观测为准，必要时把上限做成可调。

**C7.** 备用授权链接展示：把 `qrcode_img_content` 以**可复制 / 可点击**形式展示在弹窗内
（文案参照官方「若二维码无法使用，可用手机打开此链接继续」），补齐 §3.5(1) 的产品缺口。

**C8.** 凭据展示优化（决策 D9）：`WechatCredentialSnapshot` 补 `user_id` / `base_url` /
`created_at` / `updated_at`，前端凭据卡从「名称 + bot_id + 默认」扩展到含**扫码者标识、接入域、
绑定时间 / 最后轮换时间**；扫码弹窗「已授权」形态同步补 `user_id` 与绑定时间。

> 阶段 C 的 C1 + C4 是一组：没有 C1 的 `local_token_list`，服务端无从知道你已绑过这个 bot，
> 也就永远不会回 `binded_redirect`，C4 便是死代码。

---

## 五、决策清单（已定稿）

> D1–D6 沿用原推荐；D7 / D8 经讨论后调整，D9 为新增。执行时按本表口径，不再逐项确认。

| # | 决策 | 选项 | 结论 |
|---|------|------|------|
| **D1** | 协议常量落点 | ① `src/models/events/wechat.rs`（与 DTO 同文件）② `src/pkg/wechat_ilink.rs`（协议基建）③ `common` | **②**。配置面与消息面两个客户端都要用；`models` 已依赖 `pkg::aop`（`impl crate::pkg::aop::Event`），依赖方向无新增；`common` 是前后端共享层，此处无前端诉求 |
| **D2** | `channel_version` / `bot_agent` 填什么 | ① 镜像官方值（`2.4.9` / `OpenClaw`）② 我方标识（`ai_orz/<ver>`）| **②**。官方注释明确 `bot_agent`"仅用于观测，不参与鉴权与路由"、缺省即 `OpenClaw`；镜像官方属无必要冒名。`channel_version` 单独注释其语义（我方渠道实现版本）|
| **D3** | 客户端超时口径 | ① 保持固定 45s（> 服务端 hold，超时=异常）② 对齐官方固定 35s（超时=常态）③ 服务端建议值 + 10s 余量 | **③**。既采纳 `longpolling_timeout_ms`，又保住"客户端超时仍是异常信号"的监控语义 |
| **D4** | `-14` 暂停状态存哪 | ① 进程内（per-channel）② 落 `inbound_state` | **①**。暂停是自愈手段，重启后重试即恢复；落库会多出一处"必须两端闭合"的持久化运行态 |
| **D5** | 出站是否也拦暂停 | ① 拦（官方 `assertSessionActive` 口径）② 不拦 | **①**。省一次必然失败的请求，并给用户可读原因而非超时 |
| **D6** | `need_verifycode` 是否做 UI | ① 做配对码输入框 ② 只提示"请重新生成二维码" | **①**。这是风控/IDC 场景的唯一出路，不做等于该场景卡死 |
| **D7** | `expired` 处理与扫码交互 | ① 服务端有状态会话 + 自动换码（官方做法）② 保持无状态，前端重取（现状）③ 后端返回"已过期"，前端自动重取一次 ④ **前端自动长轮询 + 过期自动换码（上限 3 次）** | **④（已定）**。① 要给 handler 引入扫码会话状态，成本与收益不匹配（YAGNI）；②③ 是半程方案——**自动换码必须依托自动轮询**，只做换码而没有轮询等于没做；④ 让前端（其本身就是二维码的状态载体）承担循环、后端保持无状态，并把上一轮的手动按钮一并撤回。详见 §5.1 Q2 |
| **D8** | 顶层 `message_id` 是否进 `messages.external_key` | ① 只用于 `message_key` ② **一并打通 `external_key`（入站 + 出站回写）** | **②（已定）**。官方 `message_id` 即权威平台 ID，微信链路 `external_key` 恒空属**口径不齐**而非设计取舍；飞书分支已有现成的"推送成功后回写"样板，照抄即可。语义边界见 §3.5(2) |
| **D9** | 凭据展示字段（新增）| ① 维持现状（名称 + bot_id + 默认）② 补 `user_id` / `base_url` / `created_at` / `updated_at` | **②（已定）**。数据全部现成（`credential.po` 已有时间戳，`detail` 已有 `user_id` / `base_url`），仅需在快照 DTO 与前端卡片上透出 |

### 5.1 本轮拍板结论

**Q1（message id 打通）** —— 已定为**打通**（D8）。官方顶层 `message_id` 是服务端权威 ID、
`client_id` 是对端客户端生成、`msg_id` 在 `MessageItem` 上（item 级）；我方现有顶层 `msg_id` 是**错位字段**，
幂等键实际只有 `client_id` 在生效。由阶段 B 的 B6 / B7 / B8 一并落地。

**Q2（不做自动换码，用户是否受影响）** —— **受影响**，但要拆成三个不同场景，解法并不相同：

| 场景 | 不自动换码的后果 | 正解 |
|------|-----------------|------|
| 打开弹窗后**放置几分钟才扫**（找手机 / 切窗口 / 被叫走）| 二维码已过期 → 需手动点「重新生成」才能扫；且"已过期"提示易被误读为流程失败 | **自动换码**（C6）：屏幕上始终有可用码 |
| 扫到一半过期（手机端确认耗时超过码 TTL）| 旧码作废 → 必须重扫 | 自动换码**也救不了**（换码 = 作废旧码），但能立刻给出新码、免去一次手动点击 |
| **人根本看不到这块屏幕**（不在电脑前 / 屏幕太小 / 渲染失败）| 无任何替代路径 | **备用授权链接**（C7）：`qrcode_img_content` 本身可在手机微信直接打开 |

即：**自动换码解决的是"码在屏幕上、人回来时它还没过期"，解决不了"人看不到屏幕"**——后者靠 C7，两者互补。

> 必须澄清的一个关联：**自动换码无法脱离自动轮询存在**。上一轮把轮询降级为「我已扫码完成」手动按钮，
> 是为了绕开嵌套 spawn 的坑；改用飞书绑定轮询的**平铺 spawn** 写法即可恢复自动轮询，自动换码只是它的附带产物。
> 因此 C6 的实际收益**远大于**"省一次点击"——它把交互从"用户必须手动拉取"恢复为"扫完自动完成"。
>
> 另需澄清**官方为何持状态、我们为何不需要**：官方是 CLI，`displayQRCode` 往 stdout 打印，没有任何前端能持有 `qrcode`，
> 且要并发管理多个账号的登录会话（`activeLogins` 以 `sessionKey = accountId || randomUUID` 为键）——
> 那个会话表是被"没有前端"逼出来的，不是协议要求。我方前端就是那块"屏幕"，`qrcode` 本就由前端持有并逐轮回传，
> 因此换码动作（重调 `get_bot_qrcode`、就地替换 `qrcode` / `qrcode_img_content`）在前端做完全等价，**后端全程无状态**。
> 换码的另一个触发源是 `verify_code_blocked`（配对码连错，见 D6），与 `expired` 共用同一套换码动作。
> 据此，"刷新按钮"与"自动换码"的差别**不在换码本身**（两者都只是重调一次取码接口），
> 而在于**是否依托自动轮询**——只加按钮不恢复轮询，用户要完成「我已扫码完成」+「刷新」两次手动操作，是方案 ③ 的半程形态。

---

## 六、涉及文件清单

| 文件 | 角色 | 变更 |
|------|------|------|
| `src/pkg/wechat_ilink.rs` | 配置面协议客户端（pkg）| 协议常量 + 头/`base_info` 构造（D1）；扫码改 POST + `local_token_list` + 8 态 + `verify_code` |
| `src/models/events/wechat.rs` | 消息面 DTO + AOP 事件（models）| `text`/`content` 双读；`message_type`/`state` 双形态；**补顶层 `message_id`**；谓词改数字判定；`message_key()` 优先级调整（B6）|
| `src/service/dao/wechat/ilink.rs` | 消息面客户端 + 长轮询循环（DAO）| 出站体对齐；错误码 / 超时协商 / 暂停 / `notifyStart`；解析留痕；`send_text` 返回服务端 `message_id`（B7）|
| `src/service/dao/wechat/mod.rs` | 微信渠道 DAO trait | `push` 返回类型 `Result<()>` → `Result<Option<String>>`（B7）|
| `src/service/dao/wechat/http.rs` | DAO 门面 | 暂停期拦截出站 `push`（D5）；`push` 透出 `message_id`（B7）|
| `src/service/dal/wechat/impl.rs` | 入站适配 | 丢弃点 `log_debug!` → `log_info!`；**填 `external_key = "wechat:{message_id}"`**（B7）|
| `src/service/dal/message_channel.rs` | 渠道出站分发 | `ChannelType::Wechat` 分支回写 `external_key`（照抄飞书分支，B7）|
| `src/pkg/adapter/mod.rs` | 适配层契约 | `external_key` 字段文档登记微信侧扩展语义（B8；**不改历史迁移文件、不动表结构**）|
| `common/src/api/wechat_integration.rs` | 接口契约（微信）| 扫码状态扩展 + `verify_code` 入参 + `redirect_host`；**`WechatCredentialSnapshot` 补 `user_id` / `base_url` / `created_at` / `updated_at`**（D9）|
| `common/src/api/system.rs` | 接口契约（健康）| `wechat_poll` 增 `paused` 态 |
| `src/handlers/finance/wechat_integration/login_status.rs` | handler | `verify_code` 透传 + 新状态文案 |
| `src/service/domain/finance/identity_credential.rs` | 凭据编排 | `local_token_list` 构造；`binded_redirect` 幂等；轮换口径复核；**快照补字段组装**（D9）|
| `frontend/src/pages/finance/identity_wechat.rs` | 前端扫码弹窗 | **自动长轮询 + 过期自动换码（C6）**；备用授权链接（C7）；配对码输入 / 重定向提示（D6）；凭据卡扩展（C8）|
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

> 状态：✅ = 已通过；🔬 = 代码就绪、**须真机验证**（本机无法自证）。

- [x] ✅ `cargo test --lib` 全绿（1630+ 通过），且新增：数字/字符串双形态解析、`text`/`content` 双读、8 态解析与字面量往返、`redirect_host` 白名单、出站体快照（含数字 `message_type` 与 `text_item.text`）、请求头档位、版本号编码、暂停表与状态判定、前端阶段提示映射
- [x] ✅ `make ci`（fmt-check + clippy + clippy-fe + docs-lint + test）全绿
- [ ] 🔬 **真机端到端**：微信里向 bot 发一条文本 → 日志出现 `ilink inbound batch count=1` → `messages` 表落库 → bot 回复成功送达微信
- [ ] 🔬 **重启续拉**：重启进程，首轮日志 `resume_cursor=` 非空（游标回灌生效）
- [ ] 🔬 **监控判活**：健康页「微信长轮询」面板 `rounds` / `last_poll_at_ms` 持续推进
- [x] ✅ **错误码可见**：`ret`/`errcode` 非 0 时日志明确区分错误码，`-14` 触发 `SessionGuard` 暂停并进 `wechat_poll.state = paused`（不再静默当空轮次）
- [ ] 🔬 **`external_key` 落库**（D8）：微信入站消息行 `messages.external_key = "wechat:{message_id}"`；出站回复行在推送成功后同样被回写（接口与飞书一致）
- [ ] 🔬 **扫码自动轮询 + 自动换码**（C6）：打开弹窗后不作任何操作，扫完码后弹窗**自动**切「已授权」；让二维码自然过期一次，观察弹窗自动刷新出新码并显示「已自动刷新 1/3」
- [x] ✅ **备用授权链接**（C7）：弹窗内链接可点击 / 可复制（`qrcode_img_content` 原样呈现）
- [x] ✅ **凭据展示**（D9）：快照含 `user_id` / `base_url` / `created_at` / `updated_at`，凭据卡与「已授权」形态均已渲染

---

## 八、计划偏离登记

**执行结果**：阶段 A / B / C 全部落地，另含 D9 与文档同步。代码门禁（fmt / clippy / clippy-fe / dx check）
与单测全绿。下列为执行期的**有意偏离**或方案未覆盖而新增的决定：

| # | 项 | 方案原口径 | 实际做法与理由 |
|---|----|-----------|---------------|
| 1 | 扫码状态字面量的落点 | D1 定「协议常量落 `pkg/wechat_ilink.rs`」 | **改为落 `common::api::WECHAT_QR_STATUS_*`**，pkg 以短别名转发。理由：这几个值同时是前端的分支条件，放 pkg 会让前端只能硬编码字符串 → 必然漂移。协议常量的其余部分仍在 pkg（D1 不变）|
| 2 | `redirect_host` 的信任处理 | C3 只说「切 `https://{redirect_host}`」 | **加白名单校验**：只接受腾讯接入域内的裸主机名（无 scheme / 端口 / 路径），不合规则忽略并沿用当前接入点 + `warn`。理由：该值由客户端回传、最终成为我方出站目标，照官方直接拼接等于把出站目标交给客户端指定 |
| 3 | 扫码状态查询的客户端超时 | 沿用现有 45s | **保持 45s**（官方 `QR_LONG_POLL_TIMEOUT_MS` 为 35s）。理由：官方把客户端超时当"本轮无事件"，值等于服务端 hold 时那一轮响应常被丢弃（服务端状态在，下一轮仍能拿到，但确认会晚一整轮）；保留 10s 余量能当轮拿到 `confirmed` |
| 4 | 前端换码计数 | C6「上限 3 次」 | 常量落前端 `MAX_QR_REFRESH_COUNT = 3`（与官方同值）；展示 `已自动刷新 x/3`，首次换码即显示 1/3（官方的打印是 2/3 起，属其 off-by-one，未跟随）|
| 5 | 轮询循环去重 | 方案未提 | **新增 `poll_gen` 代次守卫**：每次发起扫码自增，旧循环据此自杀。否则"关弹窗 → 再点扫码授权"会留下两条循环同时轮询同一个 `qrcode` |
| 6 | `binded_redirect` 的凭据回显 | C4 只说「视为成功」 | **不猜测、不回显本地凭据**：该响应不带 `ilink_bot_id`，无法确认是用户哪一条凭据，回显"当前默认凭据"可能张冠李戴。前端提示"无需重复绑定"并刷新凭据列表（列表即真相）|
| 7 | `-14` 暂停的解除路径 | B2 只说「暂停 1 小时」 | **补 `ensure` 侧主动清除**：凭证指纹变化（重新扫码）时清暂停，否则用户会遇到"授权成功了但收不到消息"，须干等到期 |
| 8 | `bound_at` 的取值 | D9 提「绑定时间」 | 取**回读库中该凭据的 `updated_at`**，不在 Domain 里自取时钟——以库中真实值为准（新建即 `created_at`，轮换即本次写入时间）|
| 9 | 凭据快照的空值口径 | D9 未明确 | `base_url` 空串在**前端**显示为"未记录（按默认接入域）"而非留白；`user_id` 缺失显示 `—`（不伪造）|

**已知未做（登记为后续，非本轮范围）**：
- P3 能力面（媒体消息 / 引用消息 / 输入态 / 会话字段）——阶段一既定范围外，不做；
- `context_token` 确切有效期仍未实测（不影响本轮，主动推送本就未实现）；
- 服务端二维码实际 TTL 未实测：前端上限按官方 3 次实现，若真机发现 TTL 明显更短再调整；
- 前端 `copy_to_clipboard` 已在 4 处页面各自定义（org/chat/backup/本次），适合抽到 `crate::utils`——本轮未动无关页面，避免与其并行改动冲突。
