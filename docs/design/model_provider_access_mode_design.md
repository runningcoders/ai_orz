# 模型访问模式（access_mode）UI 设计规范

> 项目：01a0cea2（模型访问模式兼容与 jev 模型支持专项）
> 任务：T2 UI 设计（01a0cea8-ea32-7632-af1e-48f310ce4969）｜作者：UI 设计师
> 设计依据：T1 调研方案（artifact 01a0ceb8-29f8-7e43-af44-d7d3610c35c2）§五 UI 需求输入 + §2.2 API 契约草案
> 适用文件：frontend/src/pages/finance/model_providers.rs（列表 + 创建 Modal）、model_provider_detail.rs（详情 + 编辑 Modal）
> 令牌预算：**零新增设计令牌**，全部复用 HUD 体系既有类（ui_design_system.md v3.1）

## 1. 设计范围与边界

覆盖模型配置界面对 access_mode 的全部 4 处 UI 暴露点（对应方案 §三 前端改动 3 处 + §五 列表徽标建议项）：

1. 创建 Modal「添加模型提供商」新增字段；
2. 详情编辑 Modal「编辑模型提供商」新增字段；
3. 详情页信息区新增信息格；
4. 列表页「类型」列新增非流式徽标（方案建议项，采纳）。

明确不改动：调用测试 Modal、切换 Embedding Provider Modal、删除确认对话框、统计面板、路由与页面结构、既有字段的一切行为。

## 2. 设计令牌核对表（零新增）

| 用途 | 类组合 | 出处 |
|---|---|---|
| 表单控件 | `select select-bordered w-full` | 现有「类型」「能力类型」select 同款 |
| 字段标签 | `label` > `span.label-text.font-medium` | 相邻字段同款 |
| 辅助文案 | `p.text-xs.text-base-content/60` | 现有最大上下文长度说明行同款 |
| 「非流式」徽标 | `badge hud-badge badge-ghost badge-sm` | daisyUI 5.7 `.badge-ghost` 存在；input.css L1959 已定义 `.badge.hud-badge.badge-ghost` |
| 状态徽标 | `badge hud-badge badge-success/badge-neutral` | 现状不变，本设计不触碰 |

## 3. 配置项定义（交互规格）

- 字段名（标签文案）：**访问模式**
- 数据键：`access_mode`；取值二值枚举：`"stream"`（流式，默认）/ `"non_stream"`（非流式）
- 控件形态：**select 下拉**。理由：① 方案原文即指定 select；② 与相邻「类型」「能力类型」控件同语言；③ 它是"模式选择"而非"启用开关"，toggle 的开关语义会造成误读；④ 预留后续扩展 >2 种访问模式的空间
- 默认值：**流式（stream）**＝与现状行为完全一致（存量配置零影响）
- 显隐规则：仅对话（Agent）能力显示；Embedding 能力一律隐藏（embedding 请求本就非流式，方案 §一.5）
- 校验：二值枚举无自由文本，无需校验提示；select 恒有合法值

## 4. 四处落点规格

### 4.1 创建 Modal「添加模型提供商」（model_providers.rs）

- 位置：「能力类型 *」select 之后、「模型名称 *」之前（紧跟能力相关字段分组）
- 结构（与相邻字段完全同构）：

```html
div.form-control.w-full
  label.label > span.label-text.font-medium 「访问模式」
  select.select.select-bordered.w-full        <!-- 绑定 access_mode signal -->
    option value="stream"      「流式」
    option value="non_stream"  「非流式」
  p.text-xs.text-base-content/60 「仅当下游网关不支持 SSE 流式时选择非流式」
```

- 显隐：rsx 条件 `if new_capability() != ModelCapability::Embedding as i32`；切换到 Embedding 时控件与辅助文案整体隐藏，切回 Agent 恢复显示且保持上次选值（隐藏 ≠ 清除，signal 不重置）
- 初始值：signal 初值 `"stream"`；成功创建后重置回 `"stream"`
- 辅助文案：创建态恒显示（首次接触该概念的场景）
- 提交契约：Create 恒传 `access_mode = 当前选值`；能力为 Embedding 时恒传 `"stream"`（保持配置语义干净）
- data-testid：`mp-create-access-mode`（与既有 `mp-create-capability` 命名风格一致）

### 4.2 详情编辑 Modal「编辑模型提供商」（model_provider_detail.rs）

- 位置：「提供商类型」select 之后、「模型名称」之前
- 结构与 4.1 完全同构（同一字段规格，辅助文案同样保留）
- 初始值：打开编辑时由详情响应 `p.access_mode` 初始化（Get 响应恒有值，无需判空回退分支）
- 显隐：`if !editing_is_embedding`
- 提交契约：Update 恒传 `access_mode = 当前选值`（方案语义 `None = 不变更` 仅供 API 层使用，表单不制造 None 分支）
- data-testid：`mp-edit-access-mode`

### 4.3 列表页徽标（model_providers.rs 表格）

- 落点：「类型」列 td，provider_type 徽标之后；td 内层包 `div.flex.items-center.gap-1` 承载两枚徽标
- 规则：仅 `p.access_mode == "non_stream"` 时渲染 `badge hud-badge badge-ghost badge-sm`「非流式」；stream 行**零徽标**（默认态降噪——"非默认"才是需要被看见的信息）
- 不新增表格列；不动行高、斑马纹、操作按钮
- Embedding 行天然无此徽标

### 4.4 详情页信息区（HudPanel grid 信息格）

- 落点：「状态」信息格之后新增一格（md:grid-cols-2 自动排布，不跨列）
- 结构：

```html
div
  label.label > span.label-text.font-medium 「访问模式」
  div > span.badge.hud-badge.badge-ghost.badge-sm  「流式」或「非流式」
```

- Embedding provider 不显示该格（`if !capability.is_embedding()`）
- 展示值恒来自响应 `access_mode`（恒有值）

## 5. 状态与提示枚举

- 值→展示映射：`stream`→「流式」、`non_stream`→「非流式」（前端展示层转换，不在 UI 暴露 raw enum 字符串）
- 错误态：本字段无独立错误态；保存失败沿用现有 `toast.error("更新失败: {e}")`
- 成功态：沿用「创建成功」「已更新」toast，不因 access_mode 单独分支
- 空态：不适用（值恒有）

## 6. 设计取舍说明（给评审的理由）

1. **select 而非 toggle**：access_mode 是"模式选择"而非"启用开关"；select 与现有表单控件同语言，方案原文即指定 select。
2. **默认值摆上台面**：创建/编辑均显式展示「流式」默认值，而非折叠进高级选项——让"为什么模型调用走了 SSE"这类排查第一眼可答。
3. **仅 non_stream 打列表徽标**：默认态不制造噪音；徽标用方案指定的 badge-ghost（中性、非状态语义色）。
4. **Embedding 全链路隐藏**：embedding 请求本就非流式，展示无效配置只会制造误解。
5. **辅助文案用方案原句、创建与编辑双保留**：一句话讲清"什么时候选非流式"，避免管理员在编辑场景忘记语义。

## 7. 验收清单（M3 UI 验收逐条对照）

**必修 M1-M7：**

- M1 创建 Modal 存在「访问模式」select：位置＝能力类型之后、模型名称之前；选项恰为 流式/非流式 两项；默认选中 流式。
- M2 创建 Modal 显隐：能力类型切为 Embedding 后控件与辅助文案整体隐藏；切回 Agent 恢复显示且保持上次选值（隐藏不清除 signal）。
- M3 创建 Modal 辅助文案逐字＝「仅当下游网关不支持 SSE 流式时选择非流式」，样式 `text-xs text-base-content/60`，位于 select 下方。
- M4 编辑 Modal「访问模式」初始值正确回显该 provider 当前 access_mode；保存成功后详情页信息格即时更新。
- M5 Embedding provider：编辑 Modal 无该控件；详情页信息区无「访问模式」格；创建切至 Embedding 能力时控件隐藏。
- M6 列表页：non_stream 行「类型」列出现 `badge hud-badge badge-ghost badge-sm`「非流式」；stream 行与 Embedding 行无该徽标；表格未新增列、既有列结构不变。
- M7 详情页信息区「访问模式」格存在且位于「状态」格之后，值徽标 `badge hud-badge badge-ghost badge-sm`（流式/非流式两态）。

**建议 S1-S3：**

- S1 data-testid：`mp-create-access-mode` / `mp-edit-access-mode`，与既有 `mp-create-capability` 命名风格一致。
- S2 回归路径：创建 non_stream provider 成功后，列表刷新可见其「非流式」徽标。
- S3 提交契约回归：Create 恒传 access_mode（Embedding 时传 stream）；Update 恒传当前选值，无 None 分支。

## 8. 对前端实现的影响面（对齐方案 §三 前端 3 处）

1. `api/finance.rs`：Create/Update/Get/List 类型补 `access_mode` 字段（按方案 §2.2 契约）。
2. `model_providers.rs`：创建 Modal 新增字段块 + 提交组装 + capability 显隐；列表「类型」格加条件徽标。
3. `model_provider_detail.rs`：编辑 Modal 新增字段块 + 初始值回显；详情信息区新增一格。

## 9. 红线遵守声明

本任务零代码改动：仅本文档落 `docs/design/` 与 artifact 注册。不做平台配置/DB 写入；不涉及 git 操作。技术实现疑问回技术负责人确认。
