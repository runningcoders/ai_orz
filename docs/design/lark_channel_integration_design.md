# 飞书（Lark）渠道集成设计 —— 长连接协议对齐版

> 状态：**v1（2026-09-20，S2 协议层重建落地后定稿）**
> 协议依据：官方 `@larksuiteoapi/node-sdk@1.74.0`（`lib/index.js` 内含 pbbp2 protobuf 静态代码与 WSClient 生命周期）+ 官方 Go SDK `oapi-sdk-go`（`v3_main` 分支 `ws/`）+ 官方域名真机探测。取证过程与待修复登记见 [飞书链路对齐审查](../plan/飞书链路对齐审查.md)；施工分解见 [飞书链路对齐重构方案](../plan/飞书链路对齐重构方案.md)。
>
> ⚠️ **历史教训**：本文件落地前，飞书长连接按社区二手资料实现（JSON 文本帧 + `Bearer tenant_access_token` + `/open-apis/callback/ws/endpoints`），**端点实测 404、帧格式、鉴权模式全错**——连接从未建立过且无任何报错证据。协议形态（端点/传输编码）必须先证伪，再谈字段级对齐。

## 一、协议口径（SSOT：官方 SDK 源码）

### 1.1 建连（取 WS 配置）

```
POST https://open.feishu.cn/callback/ws/endpoint        ← 单数 endpoint，无 /open-apis 前缀
Content-Type: application/json

{ "AppID": "cli_xxx", "AppSecret": "xxx" }
```

- **鉴权只有两种模式**：`{AppID, AppSecret}` 或 `{AppID, ClientAssertion}`（JWT-bearer，商店应用/ISV），**二者互斥**（Node SDK 在 `clientAssertionProvider` 存在时把 `AppSecret` 置空；Go SDK 有 `ErrCodeAppSecretAndClientAssertionEmpty`）。**不接受 `tenant_access_token`**——REST 换 token 的产物喂不进 WS 建连，两条鉴权线互不相干。
- 响应携带 **`ClientConfig`**：`PingInterval`（默认 120s）、`ReconnectInterval`（默认 120s）、`ReconnectNonce`（默认 30s 抖动）——**全部由服务端下发**，pong 帧运行期还会再覆盖一次。禁止硬编码。
- 错误码分类：`internal_error` → 可重试；**`exceed_conn_limit(1000040350)` → 致命，停止重连**（每应用最多 50 连接，同 App 多客户端互踢）。

### 1.2 传输编码（pbbp2 protobuf）

帧为 **protobuf 二进制**（Node SDK `lib/index.js` L101274 起的 pbbp2 静态代码），字段 tag：

| 消息 | 字段 → tag | 类型 |
|---|---|---|
| `Header` | key=1, value=2 | string |
| `Frame` | SeqID=1, LogID=2, service=3, method=4, headers=5, payloadEncoding=6, payloadType=7, payload=8, LogIDNew=9 | uint64/uint32/string/repeated/string/string/uint32/bytes/string |

- `method=0` 控制帧（ping/pong，`msg_type` 取 header `type`：`ping`/`pong`）；`method=1` 数据帧（`event` / 分片）。
- **事件 ACK**：收到数据帧后**必须**回一帧——复用入帧的 `SeqID`/`LogID`/`service`/`method`/`payloadEncoding`/`payloadType`/`LogIDNew`，`headers` 追加 `biz_rt=<处理毫秒>`，`payload` 为 `{"code":200}`（失败回 500，交回官方重试）。**官方 3s 未 ACK 即重推** → ACK 在读循环内、入队后立即回。
- **分片重组**：`headers` 携带 `message_id` / `sum`（总片数） / `seq`（当前片序），`seq=0` 的帧携带事件类型与 schema 元数据。非法元数据（`sum<=0`、`seq>=sum`）留痕丢弃。

### 1.3 长连接部署级硬约束（官方文档）

1. 仅支持**企业自建应用**（商店应用走 ClientAssertion）。
2. 收到事件须 **3 秒内**处理完，否则超时重推 → 我方消费侧必须有**幂等查重**（§三 F20）。
3. 每应用最多 **50 连接**；取配置返回 `exceed_conn_limit` 时**停止重连**并在健康页标 `conflict`。
4. **集群模式、不支持广播**：同应用多客户端只有随机一个收到 → **仅一个实例订阅**（D8 决议），多实例靠 DB/AOP 分发。

## 二、集成架构（分层落点）

```
lark/ws.rs (WsClientAdapter)         ← 协议语义：取配置 → 建连 → pbbp2 编解码 → ACK → 事件
  │  复用 pkg::ws（连接生命周期：supervisor 退避重连 / 心跳 tick / 状态快照）
  │  协议参数全部走 adapter 接口：heartbeat() / reconnect_policy() / terminal_reason()
  ▼
AOP 事件 (LarkInboundEvent, order_key=app_id)
  ▼
consumer/lark_inbound.rs             ← ConsumeMode::Async；adapt 失败上报 Err（decide_retry 归消费者）
  ▼
dal/lark/impl.rs adapt_lark          ← 过滤（info 留痕）→ 幂等查重（external_key 反查）→ 渠道定位（cached_credential）
  ▼
MessageAdapterCallback → 上层路由（feishu_reception 档位链 / 渠道绑定 agent_id）
```

- **`pkg::ws` 是共享组件**（联邦 `organization_link` 同用）：本轮全部走「新增类型 + trait 默认实现」加法——`WsFrame`/`WsOutFrame`/`on_message`/`heartbeat()`/`reconnect_policy()`/`terminal_reason()`，联邦零改动；不给 `FrameAction` 加变体（避免破坏穷尽匹配）。
- **判活**（`WsConnState`）：`last_frame_at_ms` / `frames_received` / `last_close_code` / `last_close_reason` / `terminal_reason`。半开连接（TCP 黑洞）下心跳写进内核缓冲区也算成功——「connected 但帧停走」必须靠 `last_frame_at_ms` 识别（健康页阈值 300s = 2.5×PingInterval，**不是**微信的 90s）。
- **终局停机**单一机制 `terminal_reason()`：同时覆盖建连前（取配置 `exceed_conn_limit`）与建连后（致命 close）。

## 三、关键设计决策

| # | 决策 | 依据 |
|---|---|---|
| D0 | 全量对齐官方（端点+鉴权+protobuf+ACK 一次到位） | 只修端点不修帧格式 = 没修；分两次要重复动 `pkg::ws` trait |
| D1 | 契约清理同轮做 | 恒 None 字段要么实现要么删（与微信侧同规） |
| D6 | `appSecret` 来源：扫码建应用只取 `appId`（CLI `--json` 输出明文），secret 用户在开发者后台复制、前端粘一次；**不反读 keychain** | 官方 secret 只落 OS keychain，外部子进程读不到也不该读；`keychain-downgrade` 面向 CI 场景且不输出明文 |
| D7 | 坚持**长连接**，不引入 webhook | 本地部署无公网入口，webhook 不可达 |
| D8 | **仅一个实例订阅**；文档 + 健康页 `conflict` 呈现 | 官方集群非广播 + 3s 重推；与 AOP「同 order_key 同消费者」不变量一致 |
| D2 | 心跳照服务端 `ClientConfig` 下发值，每 tick 重查 | SDK 默认值 ≠ 线上值，pong 运行期覆盖 |

**凭据传递方向（红线）**：凭据 SSOT 是我方加密库（user_credentials），用 `--app-secret-stdin` 注入 CLI（`ensure_cli_config` 形态）；**绝不是**从 CLI 反读。扫码建应用（`config init --new --json`）只产出 `appId`（明文）+ `appSecret:"****"`（不可读）——F17 修复就是把 stdout 的 `appId` 解析回填，前端预填、只补填 secret。

## 四、红线（已入 working memory 速查）

1. 端点是 `/callback/ws/endpoint`（单数、无 `/open-apis`）；鉴权是 `{AppID, AppSecret}`，**禁** Bearer tenant token。
2. 帧是 pbbp2 protobuf 二进制；`pkg::ws` 的 `Message::Text` 通路只属于联邦——飞书数据帧禁走文本通路。
3. 数据帧必须回 ACK（同 headers + `biz_rt` + `{"code":200}`），在读循环内入队后立即回（3s 窗口）。
4. 心跳/重连参数以服务端 `ClientConfig` 下发值为准；`exceed_conn_limit` → `terminal_reason` 停机，**禁**无限重连。
5. 入站幂等查重（`find_id_by_external_key`）是 D8 单实例假设的前提，**禁**跳过；反查失败降级为不去重时必须 warn 留痕。
6. 协议字段级修复只依据官方 SDK 源码；社区文章的「源码行号引用」必须回仓核实（已发现伪造引用先例）。

## 五、验收

### 本机可自证（已过）
- pbbp2 编解码往返 / ACK 构造（保留入帧头 + `biz_rt` 追加）/ 分片重组 / 非法元数据丢弃。
- `pkg::ws`：二进制帧投递到 `on_message`、close code/reason 留痕、`terminal_reason` 停机、参数热变更、联邦既有测试零改动通过。
- `extract_app_id`：官方 JSON 输出 fixture（appId 提取 / secret 永不读出 / 非 JSON 跳过）。
- `judge_bind_status` 未知态 → Continue；`send_text_message` 空 message_id → `None`（脏键 `"lark:"` 消灭）。
- `cargo test --lib` 1659 通过；`make ci` / `make lint` / `dx check` 全绿。

### 真机（🔬 需飞书企业自建应用）
1. 扫码建应用 → 弹窗自动切 Done 且 **App ID 已预填** → 粘贴 secret 建凭证 → 长连接建立（日志 `lark ws connected`）。
2. 飞书里向机器人发一条文本 → `lark inbound adapted` → `messages` 落库（`external_key = "lark:{message_id}"`）→ bot 回复送达。
3. 让同一事件重推（断 ACK 模拟）→ `duplicate inbound event skipped` info，不重复落库。
4. 健康页：`frames_received` 推进；`kill -STOP` 对端 → 300s 后收帧列标红（stale）；第二实例启动 → `conflict` 终局呈现。
5. 群里 @ 机器人 / 发图片 / 发空文本 → 各有一行 info 留痕（含 event_id）。
