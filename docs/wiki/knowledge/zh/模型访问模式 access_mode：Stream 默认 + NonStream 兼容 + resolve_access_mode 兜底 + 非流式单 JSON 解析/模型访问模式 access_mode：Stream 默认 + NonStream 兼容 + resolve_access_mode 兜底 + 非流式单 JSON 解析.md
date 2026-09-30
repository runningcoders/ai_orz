---
kind: wiki_knowledge_card
name: 模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析
category: 系统模型提供商 / 下行调用协议
scope:
  - "src/service/dao/cortex/native/http.rs"
  - "src/models/model_provider.rs"
  - "common/src/enums/provider.rs"
  - "common/src/api/model_provider.rs"
  - "frontend/src/pages/finance/model_providers.rs"
  - "frontend/src/pages/finance/model_provider_detail.rs"
source_files:
  - common/src/enums/provider.rs#L175-L218 (ModelAccessMode 枚举：Stream 为默认 / NonStream；serde snake_case 线上取值契约 "stream"/"non_stream" + 默认值与 roundtrip 单测锁)
  - src/models/model_provider.rs#L40-L51 (ModelProviderConfig.access_mode: Option<ModelAccessMode> + timeout_ms；access_mode_or_default() 缺省即 Stream)
  - src/models/model_provider.rs#L260-L292 (config 兼容单测：存量 config JSON 无 access_mode 字段必须零变化兼容为 Stream；未设置时不序列化该字段)
  - src/service/dao/cortex/native/http.rs#L117-L169 (call_chat_completions：validate_provider_for_request → resolve_access_mode → build_chat_request_body → 按模式分流消费（stream=consume_think_stream / non_stream=parse_non_stream_response）→ 两路统一 finish_think_result)
  - src/service/dao/cortex/native/http.rs#L171-L189 (resolve_access_mode：config 缺省 → Stream；脏 config 解析失败 → 兜底 Stream + log_warn，不中断推理)
  - src/service/dao/cortex/native/http.rs#L191-L228 (build_chat_request_body：model/messages/tools 两路一致；stream 模式附加 stream:true + stream_options:{include_usage:true}；non_stream 不含任何流式字段)
  - src/service/dao/cortex/native/http.rs#L230-L307 (parse_non_stream_response / parse_non_stream_body：单 JSON 解析 → 取首个 choice → 组装与流式同构的 StreamAccumulator；缺 finish_reason 视为网关截断硬错误、finish_reason=length 走 check_stream_end 同源 max_tokens 防护)
  - common/src/api/model_provider.rs (Create/Update/Get/List DTO 含 access_mode: Option<ModelAccessMode>；Update 语义 None = 不变更)
  - frontend/src/pages/finance/model_providers.rs (创建 Modal「访问模式」select：仅非 Embedding 能力显示，Embedding 恒传 stream；列表「类型」列仅 non_stream 渲染 badge-ghost 徽标)
  - frontend/src/pages/finance/model_provider_detail.rs (编辑 Modal「访问模式」select 回显；详情信息区「访问模式」信息格——Embedding 不显示)
  - tests/integration/model_provider_access_mode_test.rs (access_mode 集成测试：配置读写 + 非流式调用路径)
  - docs/design/model_provider_access_mode_design.md (UI 设计规范：4 处落点 + 零新增设计令牌 + M1-M7 验收清单)
  - docs/wiki/zh/content/功能模块/模型提供商管理.md
  - docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/模型提供商管理.md
  - 【平行卡】docs/wiki/knowledge/zh/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md
---

# 模型访问模式（access_mode）：Stream 默认 + NonStream 兼容

## §1 概述

**本卡角色**：模型提供商「下行调用访问模式」的原子卡——回答"平台调模型时，走流式 SSE 还是非流式单 JSON"。`access_mode` 是 `ModelProviderConfig`（config JSON）内的**可选字段，非 DB 独立列**，二值枚举 `Stream`（默认）/ `NonStream`，线上取值契约 `"stream"` / `"non_stream"`。核心口径：**缺省/脏配置恒为 Stream = 平台历史行为**，因此存量配置零影响、无 migration。

- **为什么需要它**：平台默认以 SSE 流式调用下游网关（`stream: true` + `stream_options: {include_usage: true}`）。但部分下游网关/中转不支持流式，返回的不是 SSE 事件流而是单个 JSON——此时按流式解析会直接失败。`NonStream` 即为这类网关提供的兼容模式。
- **两路统一收口**：无论哪种模式，最终都组装成同一个 `StreamAccumulator`，再经同一个 `finish_think_result` 产出上层 `ThinkResult`。上层（BrainDal / cortex）对访问模式**完全无感**。
- **前后端 SSOT**：枚举定义在 `common/src/enums/provider.rs`（双端共享）；DTO 字段在 `common/src/api/model_provider.rs`；前端展示层负责 `stream → 「流式」` / `non_stream → 「非流式」` 的值映射，不在 UI 暴露 raw enum 字符串。

---

## §2 关键文件与职责表

| 文件 | 角色 | 关键内容 | 锚点 |
|------|------|---------|------|
| [common/src/enums/provider.rs](common/src/enums/provider.rs) | 枚举 SSOT | `ModelAccessMode{Stream(默认), NonStream}`；`#[serde(rename_all="snake_case")]` | `:L175-L218` |
| [src/models/model_provider.rs](src/models/model_provider.rs) | config 载体 | `ModelProviderConfig.access_mode: Option<ModelAccessMode>` + `access_mode_or_default()` + 兼容单测 | `:L40-L51`, `:L260-L292` |
| [src/service/dao/cortex/native/http.rs](src/service/dao/cortex/native/http.rs) | 调用分流核心 | `resolve_access_mode` / `build_chat_request_body` / `parse_non_stream_response` / `parse_non_stream_body`；`call_chat_completions` 两路分流 | `:L117-L307` |
| [common/src/api/model_provider.rs](common/src/api/model_provider.rs) | DTO 契约 | Create/Update/Get/List 均含 `access_mode`；Update 的 `None` = 不变更 | — |
| [frontend/src/pages/finance/model_providers.rs](frontend/src/pages/finance/model_providers.rs) | 创建 Modal + 列表徽标 | 「访问模式」select + non_stream 徽标 | — |
| [frontend/src/pages/finance/model_provider_detail.rs](frontend/src/pages/finance/model_provider_detail.rs) | 编辑 Modal + 详情信息格 | 回显 + 信息格 | — |
| [docs/design/model_provider_access_mode_design.md](docs/design/model_provider_access_mode_design.md) | UI 设计规范 | 4 处落点 + 令牌核对表 + M1-M7 验收清单 | — |
| [tests/integration/model_provider_access_mode_test.rs](tests/integration/model_provider_access_mode_test.rs) | 集成测试 | 配置读写 + 非流式调用路径 | — |

---

## §3 架构约定

本卡与「Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵」构成互补视角：该卡聚焦 Embedding Provider 的业务生命周期与向量重建触发条件，本卡聚焦 Chat 模型的下行调用访问模式（stream / non_stream）。

### 3.1 请求体分流矩阵

| 字段 | Stream（默认） | NonStream |
|------|---------------|-----------|
| `model` / `messages` / `tools` | ✅ 两路一致 | ✅ 两路一致 |
| `stream` | `true` | **不出现** |
| `stream_options: {include_usage: true}` | ✅ | **不出现** |
| 响应消费 | `consume_think_stream`（SSE 逐行解析 + 空闲/总超时双层） | `parse_non_stream_response`（单 JSON 一次解析） |

### 3.2 取值与兜底优先级

```
config JSON 可解析 → cfg.access_mode（None → Stream）
config JSON 解析失败（脏配置）→ Stream + log_warn（不中断推理）
```

- 缺省（未配置）与脏配置**都**收敛到 `Stream`，保证「平台历史行为」是绝对默认。
- Embedding 链路**不受影响**：embedding 请求本就非流式，其调用不走 `access_mode` 分支，UI 也整体隐藏该字段。

### 3.3 非流式解析的截断防护

非流式响应只有一次机会解析，因此比流式多一层显式防护：

1. 解析失败 → `Internal` 硬错误（带截断后的 body 便于排查）；
2. 无 `choices` → 硬错误；
3. `finish_reason` 缺失 → 视为被网关/代理**截断**，半截 content/arguments 不当成功返回；
4. `finish_reason = length` → 走 `check_stream_end`，与流式**同源**的 max_tokens 截断防护。

### 3.4 UI 四处落点（零新增设计令牌）

| 落点 | 规则 |
|------|------|
| 创建 Modal | 「能力类型」之后、「模型名称」之前；`select` 两项（流式/非流式），默认流式；能力为 Embedding 时整体隐藏且提交恒传 `stream` |
| 编辑 Modal | 同上结构；初值回显 provider 当前 `access_mode`；`editing_is_embedding` 时隐藏 |
| 列表「类型」列 | 仅 `non_stream` 行渲染 `badge hud-badge badge-ghost badge-sm`「非流式」；stream 行零徽标（默认态降噪） |
| 详情信息区 | 「状态」格之后新增「访问模式」格；Embedding 不显示 |

---

## §4 硬约束与回归红线

1. ✅ **缺省与脏配置必须恒为 Stream**：`access_mode_or_default()` 与 `resolve_access_mode` 的兜底都必须落到 `Stream`；违反 = 存量配置行为漂移。
2. ❌ **禁止为 access_mode 增加 DB 列或 migration**：它只能是 `ModelProviderConfig`（config JSON）内的可选字段；违反 = entitlement 之外的 schema 变更。
3. ❌ **禁止 non_stream 请求携带任何流式字段**：`stream` / `stream_options` 在 NonStream 分支必须完全不出现；违反 = 不支持流式的网关直接报错。
4. ❌ **禁止两条消费路径各自组装 ThinkResult**：stream 与 non_stream 必须都产出 `StreamAccumulator` 并统一经 `finish_think_result`；违反 = 两路字段映射漂移。
5. ✅ **非流式必须做 finish_reason 截断防护**：缺 `finish_reason` = 截断硬错误，`length` 走 `check_stream_end`；违反 = 半截回答被当成功返回。
6. ✅ **Embedding 必须全链路隐藏且强制 stream**：UI 不显示、创建时恒传 `"stream"`；违反 = 展示无效配置制造误解。
7. ✅ **前端展示必须做值转换**：`stream → 「流式」` / `non_stream → 「非流式」`，不得直出 raw enum 字符串。
8. ✅ **Update 的 `None` 语义仅属 API 层**：表单恒传当前选值，不制造 `None` 分支（`None` = 不变更只供 API 表达"未改动"）。
9. ✅ **列表徽标只在 non_stream 时出现**：默认态不制造噪音；徽标必须用 `badge-ghost`（中性、非状态语义色）。

---

## §5 历史演进

| 版本 | 变更 | 影响范围 |
|------|------|---------|
| v1.0（access_mode 落地） | `ModelAccessMode` 枚举（Stream 默认 / NonStream）下沉 `common/src/enums/provider.rs`；`ModelProviderConfig.access_mode` 可选字段；`resolve_access_mode` + `build_chat_request_body` 请求侧分流；`parse_non_stream_response` / `parse_non_stream_body` 非流式消费 + 截断防护；UI 4 处落点 | `common/src/enums/provider.rs` + `models/model_provider.rs` + `cortex/native/http.rs` + `common/src/api/model_provider.rs` + 前端两个模型提供商页 + `tests/integration/model_provider_access_mode_test.rs` |

**引入原因**：平台原先只有 SSE 流式一条路，遇到不支持流式的下游网关（含部分 OpenAI 兼容中转 / 自建推理服务）会解析失败。新增 NonStream 兼容模式把「网关能力差异」收敛为一个可配置项，同时用「缺省恒 Stream」保证零影响、用「UI 隐藏 Embedding」保证语义干净。