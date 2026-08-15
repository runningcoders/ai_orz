---
kind: design
name: 通过 common DTO 补全 Project/Task execution_plan/result 读取链路
source: session
category: adr
---

# 通过 common DTO 补全 Project/Task execution_plan/result 读取链路

_来源：b156529 → eb09a60 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
后端已存在 Update DTO 写入 execution_plan/execution_result，但 GetProjectResponse/GetTaskResponse 缺少对应字段，导致前端无法读取执行计划与结果。

## 决策驱动
- 前后端 DTO 复用（frontend 直接 use common::api::*）
- 最小侵入：仅扩展 Option<String> 字段
- 向后兼容：serde(default, skip_serializing_if)

## 备选方案
- **在 common crate 的 GetProjectResponse/GetTaskResponse 中新增 Option<String> 字段** — 优点：前端无需额外适配，handlers 映射后即可端到端打通
- **在前端单独定义新 DTO 类型** _（已否决）_；缺点：破坏 DTO 复用约定，增加维护成本

## 决策
在 common/src/api/project.rs 和 task.rs 的 Get*Response 中新增 execution_plan: Option<String>、execution_result: Option<String>，并在 handlers/project/projects/response.rs 与 handlers/project/task/response.rs 的 to_detail() 中完成 PO → Response 映射。

## 影响
前端复用 common DTO 即可读到执行计划与结果；旧客户端忽略可选字段不影响兼容性；后续如需只读/脱敏可在序列化层处理。