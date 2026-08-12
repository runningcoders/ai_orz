# lark-cli 集成与飞书渠道多应用化设计

> 状态：设计定稿（2026-08-12）；一期 + 二期均已落地（2026-08-12，分期记录见 §4）
> 关联：[message_channel_design.md](./message_channel_design.md)、[tool_design.md](./tool_design.md)、[LAYERED_ARCHITECTURE_PRACTICE.md](../LAYERED_ARCHITECTURE_PRACTICE.md)

## 1. 背景与目标

### 1.1 背景

- **lark-cli** 是飞书官方开源 CLI（`larksuite/cli`，Go 编写、npm 分发），将飞书开放平台
  2500+ OpenAPI 封装为三层命令（`+` 快捷命令 / API 命令 / Raw API），专为 AI Agent 设计：
  - `config init --new`：输出可编程处理的授权 URL（无头环境友好）
  - `auth login --no-wait --device-code`：OAuth 设备码流程，token 自动刷新
  - 应用凭证存 `~/.lark-cli/config.yaml`（App Secret 落盘加密），支持多应用配置
- 现有飞书渠道为**全局单应用**：`[lark]` 全局配置提供唯一一份 app_id/app_secret，
  `LarkDao` 单例单 WS 连接。无法满足"每个用户用自己的飞书应用（智能体）对话 + 操作飞书"。

### 1.2 目标

1. Agent 可通过内置工具 `lark_cli` 操作飞书全域能力（消息/文档/日历/多维表格/任务等）。
2. 每个用户可绑定**自己的飞书自建应用**，渠道对话与工具操作复用同一身份，权限统一。
3. 移除全局 `[lark]` 配置（应用处于测试阶段，直接采用最优方案，不做兼容迁移）。

### 1.3 非目标

- 不实现飞书群聊消息接入（维持现状：仅 P2P 私信）。
- 不封装 lark-cli 的具体业务命令为独立工具（Agent 通过 `lark_cli` 通用工具 + 技能文档自主编排）。
- 用户 OAuth token（user_access_token）授权流程列为二期，一期仅支持应用身份（tenant token）。

## 2. 核心设计认知

### 2.1 身份锚点：飞书自建应用

用户在飞书开放平台创建的自建应用是身份中枢，两头复用同一份凭证：

| 消费方 | 身份形态 | 用途 |
|--------|----------|------|
| 消息渠道入站 | 应用 bot（WS 长连接） | 用户在飞书里与自己的 Agent 对话 |
| 消息渠道出站 | tenant_access_token | Agent 回复推送回飞书 |
| `lark_cli` 工具 | tenant_access_token（一期） | Agent 以该应用身份操作飞书 |

### 2.2 权限统一性

飞书权限模型中，**应用权限范围（开放平台后台配置的 scope）是唯一权限事实源**。
渠道侧能收发消息、工具侧能操作哪些业务域，均由同一应用的 scope 决定，
无需系统侧做任何权限同步。用户 token 授权（二期）仅是"以用户身份操作个人资源"
的增量能力，其上界仍受应用 scope 约束。

### 2.3 配置载体：MessageChannel（不新增凭证表）

> ⚠️ **二期修正（2026-08-12 落地）**：下述「凭证内联在 ChannelConfig」的一期方案已废弃。
> 应用凭证改存 **users 表 `identity_credentials` JSON 列**（类型化结构体，secret 加密），
> 为绑定关系唯一事实源；`ChannelConfig` 内联凭证字段直接删除（无回退、无数据迁移，测试阶段），
> 渠道仅存 `lark_credential_id` 引用 + `lark_identity_mode`（auto/bot/user）；
> 保留 `lark_open_id / lark_user_name / lark_listen_inbound`。

`ChannelConfig` 已具备全部所需字段（`lark_app_id / lark_app_secret / lark_encrypt_key /
lark_verification_token / lark_open_id / lark_user_name`），且 `LarkDao::push` /
`test_connection` 签名早已接收 `channel` 参数。凭证中枢直接落在 MessageChannel：

- 一个用户可拥有多条 Lark 渠道（不同应用或同应用绑不同 Agent）。
- 绑定飞书渠道 = 登记「以此应用身份通信，并授权 Agent 以此身份操作飞书」。
- `LarkDao` 从"全局配置单应用"改造为"按渠道凭证执行"的多应用执行层。

## 3. 详细设计

### 3.1 移除全局 `[lark]` 配置

| 变更点 | 说明 |
|--------|------|
| `common/src/config.rs` | 删除 `LarkConfig` 结构体与 `AppConfig.lark` 字段 |
| `.ai_orz/ai_orz.toml` / `.env.example` | 删除 `[lark]` 段落及示例 |
| `src/service/dal/lark.rs::init()` | 不再读 `config.lark.enabled`；改为：无启用渠道则跳过注册（日志说明） |
| `src/service/dao/lark/http.rs::init()` | 单例不再携带全局凭证；`new_with_config` 保留供测试注入 |

### 3.2 LarkDao 多应用化

```text
LarkDaoHttpImpl
  ├─ token_cache:  RwLock<HashMap<app_id, TokenCache>>   // per-app token 缓存
  ├─ ws_conns:     RwLock<HashMap<app_id, WsConnState>>  // per-app WS 连接池
  └─ 方法签名调整：
       push(ctx, message, channel)                     // 签名不变，改用 channel 凭证取 token
       test_connection(ctx, channel)                   // 签名不变，消费 channel 凭证（原 _channel）
       start_event_listener(channel, handler)          // 新增 channel 参数，按 app_id 建连
       stop_event_listener(app_id)                     // 支持按应用停连
       stop_all_event_listeners()                      // 优雅退出编排调用
```

要点：

- **连接去重**：WS 池以 `app_id` 为键。同一应用被多条渠道（不同 Agent）引用时只建一条连接。
- **凭证校验**：建连/推送前校验 `channel.config().lark_app_id/lark_app_secret` 非空，
  缺失时报 `InvalidRequest`（fail-fast，不落全局兜底）。
- **断线处理**：维持现有 recv 错误即退出的策略，重连由外层监听循环负责（二期可加退避重连）。

### 3.3 入站路由：(app_id, open_id) 二维定位

现状 `adapt_lark` 遍历所有启用渠道按 `open_id` 匹配。改造后：

1. 事件到达时，WS 连接已知归属 `app_id`，事件回调携带该信息传入 `LarkAdapterHandler`。
2. `find_channel_by_lark_identity(app_id, open_id)`：先按 `config.lark_app_id` 过滤，
   再匹配 `lark_open_id`，消除"不同应用下 open_id 巧合/冲突"的歧义。
3. 渠道的 `agent_id` 决定路由目标 Agent（维持现有 consumer 语义，producer 零改动）。

### 3.4 入站监听开关

新增 `ChannelConfig.lark_listen_inbound: Option<bool>`（默认 true，兼容存量语义）：

- `true` / 缺省：渠道启用时建立 WS 连接（入站 + 出站均可用）。
- `false`：不建连，渠道仅作**出站推送 + lark_cli 工具凭证来源**（"纯工具用途"场景）。

### 3.5 渠道生命周期联动 WS 连接

| 触发点 | 动作 |
|--------|------|
| 启动初始化（adapter `start`） | 查询全部启用 Lark 渠道，按 `app_id` 去重建连（仅 `listen_inbound`） |
| 渠道创建/启用（DAL 层） | 若 `listen_inbound` 且该 app 无连接 → 建连 |
| 渠道停用/删除（DAL 层） | 若无其他渠道引用该 app → 停连 |
| 优雅退出 | `stop_all_event_listeners()` 挂入现有关停编排（消息渠道停服阶段） |

### 3.6 lark_cli 内置工具

参照 `shell_exec` 模式，`src/pkg/tool_registry/lark_cli.rs`：

```text
LarkCliToolFactory implements BuiltinToolFactory
  ├─ ToolPo: id="lark_cli", tags=["lark"], ControlMode=Auto, protocol=Builtin
  ├─ 参数: { command: string }   // lark-cli 子命令与参数，如 "calendar +agenda"
  └─ 执行流程:
       1. ctx.user_id → 查询该用户启用的 Lark 渠道 → 取 app 凭证
          （未绑定 → 返回引导性错误："请先在渠道管理中绑定飞书应用"）
       2. HOME 隔离：注入 HOME=.ai_orz/integrations/lark/{user_id}
          首次执行幂等写入该目录下的 lark-cli config（app_id/app_secret）
       3. spawn lark-cli 进程，带超时（复用 shell 执行的超时/输出截断策略）
       4. 返回 stdout/stderr 摘要；超长输出落日志附件（对齐 shell_exec）
```

- 注册进 `GENERIC_BUILTIN_TOOLS`，启动两阶段初始化自动同步进 DB（所有权分界刷新）。
- 工具包化：tags=`["lark"]`，Agent 安装 `lark` 工具包即可使用；配套预置技能文档
  （lark-cli 命令速查与编排指南）随技能库分发。
- 凭证读取路径：工具实例 → DAL 查渠道（禁止工具直连 DAO，经 ToolDal/Domain 编排注入）。

### 3.7 密钥安全（本期一并偿还欠账）

现状：`app_secret` 明文存于 `config_json`，渠道查询 API 存在回显泄露风险。

| 措施 | 说明 |
|------|------|
| 落库加密 | `lark_app_secret` 写入前用应用级密钥（配置文件 `security.secret_key`）对称加密；读取时解密，仅 DAO 层内部可见明文 |
| 响应脱敏 | 渠道列表/详情 DTO 中 `lark_app_secret` 恒为 `"[REDACTED]"`（fail-closed，风格对齐 `redact_trace_values_for_tool`） |
| trace 脱敏 | `lark_cli` 为 Builtin 工具默认不脱敏，但其入参不含凭证（凭证由后端注入进程环境），输出中 token 字样按关键字二次过滤 |
| 日志 | 任何日志宏调用禁止携带 secret 字段（测试断言守护） |

### 3.8 绑定流程（二期，前端用户设置页「飞书集成」）

1. 手填 app_id/app_secret（一期最小路径）→ 后端 `test_connection` 验证 → 创建渠道。
2. 自动化路径：后端在用户隔离 HOME 下执行 `lark-cli config init --new`，
   将授权 URL 返回前端，用户浏览器确认后轮询完成。
3. 用户 token 授权：`auth login --no-wait --device-code` → 设备码链接返回前端 →
   轮询 `auth status` 确认；token 由 lark-cli 在 per-user HOME 钥匙串自管刷新。
   - 风险：Linux headless 钥匙串可用性需实测；不可用时降级为仅应用身份。

## 4. 分期计划

### 一期：多应用化 + lark_cli 工具（已落地）

1. 移除 `[lark]` 全局配置（3.1）
2. `LarkDao` 多应用改造：per-app token 缓存 + WS 连接池 + 签名调整（3.2）
3. 入站路由二维定位 + `lark_listen_inbound` 开关（3.3、3.4）
4. 渠道生命周期联动 WS（3.5）
5. `lark_cli` 内置工具 + 凭证解析 + HOME 隔离（3.6）
6. 密钥加密与脱敏（3.7）

### 二期：绑定体验 + 用户身份 + WS 稳定性（已落地，2026-08-12）

7. 前端 Settings「飞书集成」区块 + `config init --new` 自动化流程（3.8）；渠道创建 Modal 改凭证下拉，渠道页与 Settings 分开两页互为详情；11 端点统一挂 `/api/v1/user/lark-integration/`
8. 用户 OAuth token（device flow）接入：pkg/lark_integration.rs 会话编排 + keychain 降级引导；渠道级身份模式 `lark_identity_mode`（`config default-as` marker 幂等）
9. WS supervisor 指数退避重连（1s 起步、倍增封顶 60s、±20% 抖动）+ `WsTokenSource` 重连自愈 + `LarkWsMetrics` 挂入 health metrics

**二期关键落地认知：**

- 凭证数据模型：users 表 `identity_credentials` JSON 列为唯一事实源，渠道仅存引用（修正 §2.3）；凭证变更 Domain 编排联动（清 HOME config + WS release/ensure）；删凭证有引用报 Conflict
- 推送链路凭证解析双路径：`ChannelPushOptions.user` 附带优先，无则 user DAO 按 `channel.user_id` 兜底直查
- `config init --new` 实测结论（分支 B）：完成后 App Secret 存 keychain 不可读 → 前端引导手动补填 secret 建凭证；输出在 stderr
- 渠道删除为软删，凭证引用计数需过滤 Deleted 状态

## 5. 风险与开放问题

| 风险 | 应对 |
|------|------|
| WS 连接数随用户增长 | 每应用一连接 + 懒启动/闲置关停（二期治理）；飞书单应用 WS 连接数限制宽裕 |
| lark-cli 二进制分发 | 部署环境预装（`npx @larksuite/cli install` 或 release 二进制）；工具执行前探测，缺失返回引导错误 |
| per-user HOME 下钥匙串不可用（headless Linux） | 一期仅应用身份不依赖钥匙串；二期实测后决定是否降级 |
| 密钥落库加密的密钥管理 | 一期使用配置文件 `security.secret_key`；后续可演进为系统初始化时生成 |
