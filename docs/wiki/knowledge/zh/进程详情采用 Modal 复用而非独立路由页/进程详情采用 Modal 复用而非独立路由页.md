---
kind: design
name: 进程详情采用 Modal 复用而非独立路由页
source: session
category: adr
---

# 进程详情采用 Modal 复用而非独立路由页

_来源：eb51721 → 8be1663 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
进程管理需要同时从系统页面列表和 ChatSidePanel 工具调用 Tab 两处查看进程详情，若为每个入口建独立路由会导致重复实现。

## 决策驱动
- 组件复用
- 避免路由膨胀
- 与列表页/聊天侧栏共享同一视图

## 备选方案
- **独立路由页 /system/processes/{pid}** _（已否决）_ — 优点：URL 可直接分享；缺点：两处复用需复制逻辑或引入嵌套路由，增加复杂度
- **Modal 内嵌 ProcessDetailContent 组件** — 优点：列表页与聊天侧栏共用同一组件；无需新增路由；保持轻量；缺点：无法通过 URL 直接定位到某个进程详情

## 决策
在 `frontend/src/components/process_detail.rs` 抽取 `ProcessDetailContent` 组件，由 `/system/processes` 列表页和 ChatSidePanel 工具调用 Tab 以 Modal 形式复用，不新建路由级详情页。

## 影响
进程详情可被多处嵌入展示，但无法通过浏览器地址直接跳转；后续如需分享需额外设计分享链接方案。