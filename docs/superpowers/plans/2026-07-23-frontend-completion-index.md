# 前端功能补全 — Plan 索引

> **For agentic workers:** 本索引汇总了"后端有 API 但前端未实现"的所有功能补全计划，拆分为 3 个独立可交付的子 plan，按优先级 P0 → P1 → P2 顺序执行。每个子 plan 都能独立编译、独立交付、独立验证。

## 背景

经对比后端 135 个 HTTP API 与前端 27 条路由 + 105 个 API 封装函数，识别出以下功能缺口：

- **完全无前端页面**：Skill 文件管理（5 个 API）、附件内容读写（2 个）、Artifact 内容读写（2 个）、工具调用记录查询（2 个）
- **已封装 API 未被调用**：13 个函数（update_agent / update_model_provider / update_project / update_user / update_skill / get_skill / get_attachment 等）
- **CRUD 不完整**：9 个页面缺 Edit，1 个页面缺 Create
- **缺详情页**：4 个实体（Skill / Artifact / Attachment / MCP / MessageChannel）

## 已存在（无需重复实现）

- **登出按钮**：`frontend/src/layouts/navbar.rs:460-466` 已实现"退出登录"按钮 + `handle_logout`（已修复 HIGH #9 + #8）
- **TaskEditModal 的 Edit 模式**：`frontend/src/pages/project/task_edit_modal.rs` 已实现，仅未被任何页面以 Edit 模式调用

## 子 Plan 清单

| # | Plan 文件 | 优先级 | 范围 | 工作量 |
|---|---|---|---|---|
| 1 | [2026-07-23-p0-skill-detail-and-task-edit.md](./2026-07-23-p0-skill-detail-and-task-edit.md) | P0 | Skill 详情页（含文件浏览+编辑）+ 任务详情 Edit 入口 | 中等 |
| 2 | [2026-07-23-p1-crud-edit-modals.md](./2026-07-23-p1-crud-edit-modals.md) | P1 | Agent / ModelProvider / Project / User 4 个 Edit Modal | 中等 |
| 3 | [2026-07-23-p2-detail-pages-and-tool-calls.md](./2026-07-23-p2-detail-pages-and-tool-calls.md) | P2 | Artifact / Attachment / MCP / MessageChannel 4 个详情页 + 工具调用记录查询页 | 较大 |

## 执行原则

1. **DRY**：所有 Edit Modal 复用现有 `Modal` + `ConfirmDialog` 组件，不重复造轮子
2. **YAGNI**：只做已封装 API 对应的功能，不引入未使用的后端接口
3. **TDD 适配**：前端 Dioxus 组件难做单元测试，验证以 `cargo build --release` 编译通过 + `dx serve` 手动验证为主；后端无改动故无需新增后端测试
4. **频繁提交**：每个 Task 完成后立即 `git commit`
5. **保持一致**：所有新页面遵循 `model_provider_detail.rs` / `triggers.rs` 的现有模式（`AppLayout` 包裹、`Modal` + `ConfirmDialog` 组合、`use_effect` 加载、`toast` 反馈）

## 验证标准

每个子 plan 完成后需通过：
1. `cd frontend && cargo build --release` 无新增 error
2. `cd frontend && dx build --release` 通过
3. `cargo test --workspace`（后端无改动，应仍为 745 passed）
4. 手动在浏览器验证新增页面的核心交互

## 整体进度

- [ ] Plan 1 (P0) — Skill 详情页 + 任务 Edit 入口
- [ ] Plan 2 (P1) — 4 个 CRUD Edit Modal
- [ ] Plan 3 (P2) — 4 个详情页 + 工具调用记录查询页
