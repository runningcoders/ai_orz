---
kind: design
name: 进程详情采用共享组件 + Modal 弹窗，不新建独立路由页
source: session
category: adr
---

# 进程详情采用共享组件 + Modal 弹窗，不新建独立路由页

_来源：a756890 → eb51721 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
进程详情需要在两个场景复用：系统管理页面的「详情」按钮和 ChatSidePanel 工具调用 Tab 中点击 pid 徽标弹出的窗口。若各自建路由页会导致状态管理和 UI 行为不一致。

## 决策驱动
- 组件复用
- 避免多入口状态同步问题
- 轻量级详情页不需要路由级代码分割

## 备选方案
- **独立路由页 `/system/processes/:pid`** _（已否决）_ — 优点：可分享链接、支持浏览器前进后退；缺点：两处复用需复制状态逻辑，增加路由复杂度
- **共享 `ProcessDetailContent` 组件 + `components::modal::Modal` 弹窗** — 优点：单点实现、懒加载 shell_status、可在任意位置弹出；缺点：无独立 URL，无法直接分享详情链接

## 决策
新建 `frontend/src/components/process_detail.rs` 中的 `ProcessDetailContent { pid }` 组件，在系统管理页和 ChatSidePanel 中均通过 Modal 内嵌展示，数据懒加载并支持手动刷新与 kill 操作。

## 影响
详情视图天然可嵌入聊天侧栏，减少跨页面状态传递成本；代价是详情不可被直接 bookmark 或通过 URL 分享。