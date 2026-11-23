---
kind: 设计决策
module: 11-前端应用
name: workspace_scene_design
title: 工作台现场化设计
status: 草案
scope:
- frontend/src/pages/workspace/mod.rs
- frontend/src/pages/workspace/source.rs
- frontend/src/pages/workspace/scene/mod.rs
- frontend/src/pages/message/chat.rs
- frontend/src/utils/status.rs
- common/src/enums/agent.rs
- src/handlers/system/workspace_metrics.rs
related:
- 09-统计与监控/A-设计/workspace_topbar_metrics_design.md
- 11-前端应用/A-设计/frontend_architecture.md
- 11-前端应用/A-设计/ui_design_system.md
---

# 工作台现场化设计

> 🎯 **本文档定位**：确定工作台从「监控看板」转为「组织现场」的产品形态、语义/表现分层与数据契约，供评审拍板；接口与渲染细节以实际代码为准。
> 状态：v0.2 草案（已并入首轮评审意见，待确认后进入实现）
> 查阅场景：评审工作台形态、决定 v1 范围、讨论流式/实时数据接入时打开
>
> 关联文档：
> - [工作台顶栏聚合指标设计](docs/modules/09-统计与监控/A-设计/workspace_topbar_metrics_design.md) — 已有的聚合端点先例
> - [前端架构](docs/modules/11-前端应用/A-设计/frontend_architecture.md) — 路由与页面组织
> - [UI 设计系统](docs/modules/11-前端应用/A-设计/ui_design_system.md) — HUD 视觉规范

---

## 一、目标与决策表

### 1.1 一句话定位

工作台不是**看板**（看清楚），是**到场**（处理掉）。用户进来的唯一理由是「有事等我」。

与对话页的分工按时间性切分：对话页是**收件箱**（全部历史，异步）；工作台是**现场**（现在，在场）。

### 1.2 决策表

| # | 问题 | 决定 | 理由 |
|---|------|------|------|
| Q1 | 鸡肋的根因是不实时吗 | 否，是没有「现在为什么进来」 | 换成高频推送只会让鸡肋更闪 |
| Q2 | v1 要不要真事件流 | 不要 | 入口形态先立住，数据可先用轮询+diff |
| Q3 | 空间布局用什么 | 固定坐标，不用力导向决定位置 | 工位常驻要求位置稳定；力导向给的是关系稳定 |
| Q4 | 全景放多少 Agent | 只放关注集（≤8），其余用聚合表达 | 6 个是 demo，60 个是停车场 |
| Q5 | 深度交流是页面还是景深 | 景深层，不换路由 | 跳页会切断「走过去」的连续感 |
| Q6 | 前端是否直连轮询接口 | 否，只消费 `SceneSource` | 换数据源时只换适配器，组件不动 |
| Q7 | 事件契约放哪 | `common` | 后端将来直推同一批事件，双端必须同源 |
| Q8 | 语义中间态（presence / attention / blocked）算在哪 | 后端 `SceneProjection` | 多消费者同源、可观测、聚合成本最低、换传输不换计算 |
| Q9 | 表现中间态（状态机 / 插值 / 节流）算在哪 | 前端 `PresentationState` | 依赖帧率与视口，多端表现不同 |
| Q10 | 状态如何建模 | 维度 + 标签两层，UI 只依赖维度 | 后端新增事件类型时前端不改渲染 |
| Q11 | 高频事件合流（coalesce）放哪 | 后端 / 传输层一次；插值留前端 | 避免 N 个客户端各自合流 |
| Q12 | 场景形态 | 固定席位场（8 席），参照策略游戏编队界面 | 坐标天然稳定、上限天然成立，且绕开「项目/架构不是二维空间」的陷阱 |

---

## 二、产品形态

### 2.1 三层景深

```
焦点      全景 Overview          中景 Scene              特写 Focus
内容      该我出手 + 统计条       关注集工位 + 状态徽标     对话流 + 输入
时间粒度  分钟级                 分钟级                  秒级
数据      Projection 聚合         SceneEntity             消息 / 事件序列
```

三层共用同一批**场景实体 ID** 与同一套 `SceneEvent`，只有聚合粒度不同——镜头推进不换数据源。

v1 全景不做「分区」：现有数据里没有可靠的空间/区域概念，硬做等于造一个假办公室。

### 2.1.1 场景形态：固定席位场

参照策略游戏的编队界面，而不是具象办公室。8 个固定席位，谁被关注谁落座，空席位暗置。

```
┌──────────────────────────────────────┐
│ [该我出手 3]                  [概览] │
│                                      │
│   ● 席位1     ◐ 席位2     ○ 席位3    │
│   Working     Waiting     (空)       │
│                                      │
│   ◐ 席位4     ● 席位5     ○ 席位6    │
└──────────────────────────────────────┘
远景：光点 + 状态色   近景：卡片 + 标签
```

三档视觉方案只取第一档：做「空间语法」（稳定坐标、景深、镜头、LOD），不做「具象化」（桌椅/Avatar），不做「玩法化」（移动/任务）。理由：用户要的是「谁在哪儿、我该去哪儿」，空间语法即可满足；具象化会放大 60 个 Agent 的拥挤问题，且每个形象都要能表达状态，成本与失真同时上升。

### 2.2 交互循环

```
进入 → 瞟一眼（谁需要我） → 走过去（选中工位） → 处理 → 退回到全景
```

### 2.3 v1 范围

| 做 | 不做 |
|----|------|
| 场景骨架 + 固定坐标持久化 | 自由漫游 / 自由缩放 |
| 「该我出手」队列 | 语音 / 视频 / 双向流 |
| 工位状态（现有 runtime + 派生类别） | AOP 事件流实时推送 |
| 选中推进到对话景深层，**且能在层内发消息 / 做一次确认** | 大规模 Agent 布局 |
| `SceneSource` 适配层（轮询实现） | 多端坐标同步 |

焦点层「能干活」是 v1 的成败线：只读的景深层等于给看板加了个动画。验收判据是**进来 30 秒内能完成一次处理**。

### 2.4 现有资产对账

| 已有 | 位置 | 缺口 |
|------|------|------|
| 全屏 Canvas 关系图层 | [workspace/scene/canvas.rs](frontend/src/pages/workspace/scene/canvas.rs) | 数据为 6 个模拟节点，力导向决定位置 |
| 运行时三态徽标 | [workspace/hud.rs](frontend/src/pages/workspace/hud.rs)、[status.rs](frontend/src/utils/status.rs#L192-L226) | 只表达「在忙/在睡」，不表达「等你」 |
| 运行时状态枚举 | [agent.rs](common/src/enums/agent.rs#L102-L130) | 无 Waiting / Stalled，也无维度 |
| 聚合指标端点 | [workspace_topbar_metrics_design.md](docs/modules/09-统计与监控/A-设计/workspace_topbar_metrics_design.md) | 无「需要我」维度 |
| 对话页面 | [chat.rs](frontend/src/pages/message/chat.rs) | 独立路由，非景深层 |

---

## 三、架构

### 3.1 页面分层

```
WorkspacePage                     页面壳，只持有场景状态机
├─ SceneLayer                     Canvas：坐标、拾取、镜头
│   ├─ SeatGridLayer              固定席位（8 席，坐标常量表）
│   ├─ NodeLayer                  工位节点（关注集）
│   └─ EdgeLayer                  关系线（叠加层，不参与布局）
├─ HudLayer                       浮层
│   ├─ AttentionQueue             「该我出手」，左上，恒在最前
│   ├─ GlobalStats                全景数字，右上
│   └─ SelectionCard              选中详情
└─ FocusLayer                     景深层，覆盖场景而非跳转
    ├─ ConversationStream         对话流（复用 chat 的渲染组件）
    └─ Composer                   输入（v1 整段提交，v2 流式）
```

> 当前实现：[workspace/mod.rs](frontend/src/pages/workspace/mod.rs)（已拆为 mod / helpers / hud / dock / source / presentation / scene 七个模块；全景席位场见 [scene/mod.rs](frontend/src/pages/workspace/scene/mod.rs)）

### 3.2 状态机

```
Overview ──select(entity)──▶ Focusing ──esc / 处理完成──▶ Overview
                                │
                                └──select(other)──▶ Focusing（切换工位）
```

### 3.3 数据入口

界面**只**订阅 `SceneSource`，不直接调用轮询接口。v1 实现 `PollingDiffSource`（轮询快照 → diff → 合成事件）；v2 换成 `EventSceneSource`（SSE/WS），组件零改动。

### 3.4 语义 / 表现分层

```
后端   SceneProjection = fold(SceneEvent)        语义：是什么
       ├─ dims（interruptible / category / blocked / since）
       ├─ labels（显示用描述，随事件更新）
       └─ attention
传输   coalesce（合流） + 单调 seq + 后端时间戳
前端   PresentationState                         表现：像什么
       ├─ 逻辑态 → 动画态 → 插值
       ├─ 节流 / LOD / 镜头缓动
       └─ 时间戳只读后端，不自造
```

判据：需要被多个消费者共用、需要可查证 → 后端；跟帧率、视口、设备有关 → 前端。

关键约束：`SceneProjection` 必须是**事件流的 fold 结果**。v1 的事件由轮询 diff 合成，v2 由 AOP 扇出，但 projection 是同一个函数——这样「替换数据源」换的是输入源，不是算法。

---

## 四、数据契约

### 4.1 场景实体

```
SceneEntity
├─ id               String          与 Agent / Task / Project 的业务 id 对齐
├─ kind             SceneEntityKind Agent | Task | Project | Tool
├─ position         Option<Pos>     持久化坐标；None 时由初始化布局写入后固化
├─ dims             PresenceDims    UI 只依赖这一层
│   ├─ interruptible bool           能否打扰（休息 / 专注中为否）
│   ├─ category      Working | Idle | Resting | Waiting | Stalled | Offline
│   ├─ blocked_on    Option<BlockReason>
│   └─ since         i64
├─ labels           Vec<StateLabel> 显示用：think 第 N 轮 / 调用某工具 / 等待某结果
├─ attention        Attention       None | Info | NeedsYou
├─ last_activity_at i64
└─ display          DisplayName
```

`category` 是 Agent 自己的状态，`attention` 是「与我有关且需要动作」——两者正交，否则「在忙」会被误读为「别烦我」。`labels` 是开放集合，后端新增事件类型只增标签，不改渲染。

### 4.2 category 推导（v1，全部基于现有数据）

| category | 推导规则 |
|----------|----------|
| Working | `Busy`，且未超过停滞阈值 |
| Stalled | `Busy` 且超过 30 分钟状态未变（仅 Info，不进 NeedsYou） |
| Resting | `Resting`，且不欠用户回复 |
| Idle | `Idle`，且不欠用户回复 |
| Waiting | `Idle` / `Resting`，且最近一条 Agent→用户消息晚于用户回复，**且属于任务类或被 @** |
| Offline | v1 不产生（输入只含已注册 Agent），保留给「已知但未运行」 |

优先级：忙碌（含停滞）高于「等你回话」——正在干活的 Agent 不该被算成在等你。

### 4.3 attention 推导

| 级别 | 规则 |
|------|------|
| NeedsYou | Waiting（任务类）/ 有待审批或待确认的任务 |
| Info | Stalled / 刚完成一次任务（限时衰减） |
| None | 其余 |

原则：**宁可漏，不可刷屏**。一旦「该我出手」里混入噪音，用户就再也不看这个区域了。

### 4.4 事件

```
SceneEvent
├─ EntityUpserted { entity }
├─ CategoryChanged { id, from, to, at }
├─ AttentionChanged { id, from, to, reason, at }
├─ Activity { id, at }                心跳用，驱动呼吸而非重排
└─ StreamDelta { conversation_id, seq, payload, at }   v2 预留
```

每条事件带后端时间戳与单调 `seq`；前端不自行生成时间。这是断线补拉与多端一致的前提。

### 4.5 订阅接口

```
SceneSource
├─ snapshot() -> SceneSnapshot        首屏全量（= projection 当前值）
└─ watch() -> Stream<SceneEvent>      增量；v1 由 diff 合成，带单调 seq
```

> 当前实现：尚无（本文档定义）。运行时三态现状见 [agent.rs](common/src/enums/agent.rs#L102-L130)

---

## 五、边界与红线

1. **坐标不由布局决定**。首次初始化后即固化，任何数据变化都不重排——否则「走过去」失去空间记忆。
2. **关注集缓慢变化**。新增工位从边缘淡入，移出的淡出但不立即释放槽位，pin 的永不移除；否则固定坐标会被关注集的抖动抵消。
3. **界面不直连轮询接口**。只消费 `SceneSource`。
4. **不做「装忙」**。画面只由事件驱动；无事件即静止，允许看起来「安静」。
5. **场景不承载全部 Agent**。超出关注集一律用聚合表达，保证 6 个和 60 个不是两套产品。
6. **对话是景深层，不是路由跳转**；且焦点层必须可操作（发消息 / 确认）。
7. **错误必须可见**。断流时显式降级（「数据停在 X 分钟前」），不允许静默显示旧值。
8. **时间戳与 seq 只能来自后端**。前端状态机只读不造，否则多端不一致、无法回放。

---

## 六、评审结论

| # | 事项 | 结论 |
|---|------|------|
| R1 | 关注集来源 | 已定：自动（运行中 ∪ 等我回话），上限 8，遵循红线 2 的缓变规则 |
| R2 | 「该我出手」聚合位置 | 已定：后端，作为 `SceneProjection` 的一部分，沿用 metrics 聚合先例 |
| R3 | 坐标持久化 | 已定：v1 落 localStorage，多端同步后置 |
| R4 | 现有顶栏指标 | 已定：保留接口，降级为全景条，不删除 |
| R5 | `workspace.rs` 拆分 | 已定：先拆再改（scene / hud / focus 三块 + 页面壳） |
| R6 | v1 是否含对话流式 | 已定：不含，流式单独一期 |
| R7 | `blocked_on` 是否进 trace 可查 | 待定：建议 v1 只做界面展示，v2 随事件流一并落 trace |

---

## 七、演进路径

```
v1  固定坐标 + 关注集工位 + 该我出手 + 景深推进可操作（轮询 diff → projection）
v2  AOP 事件 → SSE/WS 扇出；projection 不变，仅换输入源；工位获得真实节律
v3  对话流式 I/O（session + seq + 打断），景深层升级为深度交流模式
v4  多端在场、语音等媒体面接入（控制面与媒体面分离）
```
