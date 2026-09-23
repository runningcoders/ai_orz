---
kind: RAG 原子知识卡
name: pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）
category: pkg层基础设施
scope:
- src/pkg/authorization/**
- src/pkg/tool_registry/policy_authorization_fusion.rs
- src/service/domain/finance/tool_authorization.rs
- src/handlers/finance/tool/confirm_authorization_via_chat*.rs
- src/handlers/finance/tool/create_request_authorization.rs
- src/handlers/finance/tool/decide_authorization.rs
- src/handlers/finance/tool/list_authorizations.rs
- src/handlers/finance/tool/revoke_authorization.rs
- common/src/api/tool.rs
- common/src/enums/event_topic.rs
- common/src/redaction/rule.rs
source_files:
- src/pkg/authorization/authorization_gate.rs（CreateAuthorizationCmd 含 call_id 与 tool_call 精确关联 + rule_idempotent + reason 审计留痕）
- src/pkg/authorization/model.rs（PendingAuthorization 六态状态机 + call_id 关联字段）
- src/pkg/tool_registry/policy_authorization_fusion.rs（confirm_with_authorization：拦截侧消费最小接口 → build_cmd 注入 call_id → gate.request_authorization）
- src/service/domain/finance/tool_authorization.rs（审批状态机 domain：decide/approve/revoke/list 六态流转 + 审计增补）
- src/handlers/finance/tool/decide_authorization.rs（审批决策入口 Handler：通过/拒绝 + 审计）
- src/handlers/finance/tool/list_authorizations.rs（管理面列表：归属查询 + 状态过滤）
- src/handlers/finance/tool/revoke_authorization.rs（即时撤销）
- common/src/redaction/rule.rs#L102-L115（authorization 规则 exclude=id，放行 authorization_id 标识符脱敏业务主键）
- common/src/api/tool.rs#L618-L625（AuthorizationDetailDto.call_id 追加，前端挂到对应调用记录）
- common/src/enums/event_topic.rs（AuthorizationRequested / AuthorizationDecided / AuthorizationRevoked 新增 topic）
- src/router.rs（工具授权审批面路由挂载）
- docs/wiki/zh/content/核心模块/工具与技能/工具系统/工具授权审批.md
- docs/wiki/zh/content/基础设施/工具注册表/工具安全控制.md
- docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md
- docs/wiki/knowledge/zh/工具输出与安全治理/工具输出与安全治理.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md
- docs/wiki/knowledge/zh/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行.md
- docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md
---

## §1 概述

**本卡角色**：工具授权审批系统的总知识卡。覆盖 pkg/authorization 新建基建包（CreateAuthorizationCmd / AuthorizationGate / PendingAuthorization 状态机）、policy_authorization_fusion（拦截侧 → 授权建单侧的最小接口）、domain 审批状态机（decide / approve / revoke / list 六态流转）、Finance tool handlers 5 个审批 handler（confirm_authorization_via_chat / create_request / decide / list / revoke）、脱敏规则放行 authorization_id 业务主键、前端 AuthorizationDetailDto.call_id 精确关联字段。**定位：排查 Agent 工具被拦截后为什么没弹窗、审批单与工具调用记录为什么对不上、脱敏后前端拿不到 authorization_id 时读。**

- **拦截侧消费最小接口**（`authorization_gate.rs`）：`CreateAuthorizationCmd` 携带 `call_id: Option<String>` 把拦截建单与触发拦截的那次工具调用精确对上——不再用 (agent, tool, 时间窗) 反推，审批面可直接在对应调用详情里给出通过/拒绝。主动建单（无被拦调用上下文）`call_id=None`。
- **六态审批状态机**（`PendingAuthorization.status`）：Pending → Approved（批准）/ Rejected（拒绝）→ Active（已签发、有效期内）→ Revoked（即时撤销）→ Expired（到期自动失效）。状态机由 domain 层（`tool_authorization.rs`）维护，handler 层只做 DTO 转换 + 审计增补。
- **脱敏规则放行**（`common/src/redaction/rule.rs`）：authorization 规则追加 `exclude: ["id"]`——`authorization_id` / `authorization_ids` 是业务主键（前端要拿它调审批接口），不得脱敏；真正的 `Authorization: Bearer xxx` 头（键名无 id）照旧脱敏。单元测试 `authorization_identifier_fields_preserved` + `authorization_id_survives_end_to_end_redaction` 覆盖。
- **与工具执行层的交叉**（`policy_authorization_fusion.rs` 被拦截 → `confirm_with_authorization` 调 `gate.request_authorization` → `call_id` 从 `ctx.tool_call_id()` 注入）——此路径的 Err 分支在 `tool_execution.rs` 修复了底层 Error field 不透出的 bug（见 Level 3 兄弟卡「工具系统三层调用架构」§9 硬约束）。

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| pkg/authorization/authorization_gate.rs AuthorizationGate trait | 拦截侧消费最小接口 | `request_authorization(ctx, cmd) → Result<PendingAuthorization>`；cmd 含 call_id（精确关联工具调用）/ command_signature / blocking_rule / reason / rule_idempotent | `:L1-L80` |
| pkg/authorization/model.rs | 授权域核心模型 | `PendingAuthorization { id, agent_id, tool_id, status, call_id, blocking_rule, ... }`；六态状态机 Pending/Approved/Rejected/Active/Revoked/Expired | `:L1-L120` |
| pkg/tool_registry/policy_authorization_fusion.rs | 拦截→建单融合层 | `confirm_with_authorization(ctx, blocked_by, reason)`：Policy `Confirm` 层推导 + 短路 → 构造 CreateAuthorizationCmd（call_id=ctx.tool_call_id()) → gate.request_authorization | `:L1-L150` |
| service/domain/finance/tool_authorization.rs | 审批状态机 domain | decide（通过/拒绝 + 审计增补）/ list（归属查询 + 状态过滤）/ revoke（即时撤销）；StatusMachine 守卫非法状态跳转（Revoked/Expired 状态拒绝 decide） | `:L1-L150` |
| handlers/finance/tool/confirm_authorization_via_chat.rs | Agent 对话中就地请求授权 | Agent 被拦截时弹出内嵌授权组件（不跳转审批面），Agent 点确认/拒绝直接修改 PendingAuthorization | 见 handler |
| handlers/finance/tool/decide_authorization.rs | 审批决策入口 | `POST /api/v1/finance/tool/authorizations/:id/decision`：通过/拒绝 + 审计增补 | 见 handler |
| common/src/redaction/rule.rs | authorization 规则放行 | `KeyRule { name: "authorization", patterns: ["authorization", "bearer"], exclude: ["id"] }`——authorization_id 标识符原样保留 | `:L102-L115` |
| common/src/api/tool.rs | DTO 新增 call_id | `AuthorizationDetailDto.call_id: Option<String>`——前端据此把授权单挂到对应调用记录 | `:L618-L625` |

**章节来源**
- [authorization_gate.rs](src/pkg/authorization/authorization_gate.rs)
- [tool_authorization.rs](src/service/domain/finance/tool_authorization.rs)
- [policy_authorization_fusion.rs](src/pkg/tool_registry/policy_authorization_fusion.rs)

## §3 架构约定

本卡与 [工具系统三层调用架构卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md)、[工具输出与安全治理卡](docs/wiki/knowledge/zh/工具输出与安全治理/工具输出与安全治理.md)、[策略引擎卡](docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md)、[Shell 工具全链路卡](docs/wiki/knowledge/zh/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行.md)、[共享工具凭据增强器卡](docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md) 构成 **Level 3 兄弟卡组**——各卡 scope 交集 25-30%，分别从「调用路由」「安全治理」「策略判定」「Shell 拦截」「凭据注入」「授权审批」六视角覆盖工具执行链路。本卡专注**拦截后的审批建单与状态机流转**，不重复其他卡的路由/凭据/策略判定内容。

**工具授权完整链路**（Agent 执行 Shell 危险命令触发拦截 → 审批 → 通过后继续执行）：

```
1. Agent 调用 shell_exec("git push --force")
2. shell_policy 评估 ShellRulePolicy → action()=Confirm（破坏性 git 命令）
3. tool_execution 拦截 → policy_authorization_fusion::confirm_with_authorization
   → gate.request_authorization(call_id=ctx.tool_call_id()) → 落 Pending
4. consumer 发布 authorization.requested AOP 事件
   → 前端 SSE 推 ToolExecEvent，弹出审批 Modal（挂到 trace call_id）
5. 人类审批 → decide_authorization Handler → domain decide → Approved
6. 同时触发 authorization.decided 事件 → 拦截层收到继续执行信号
7. shell_exec 继续 → 记录 ToolCallEntry + authorization_id 关联
8. 审计增发改由 domain 统一增补（handler 层零业务）
```

## §4 硬约束与回归红线（6 条）

1. **PendingAuthorization.call_id 必须等于触发拦截那次工具调用的 call_id**：policy_authorization_fusion::confirm_with_authorization 调 gate.request_authorization 时，call_id 从 `ctx.tool_call_id()` 注入——主动建单（管理面手动登记）为 None；禁止 handler / domain 层手动拼 call_id。测试：authorization_gate_test 验证 call_id 透传。
2. **domain tool_authorization.rs 是六态状态机唯一变更入口**：handler 层（decide_authorization / list_authorizations / revoke_authorization）只能构造 Command + 调 domain 方法，禁止手动 UPDATE 改 Status 字段。非法跳转（Revoked → Approved / Expired → Decide 再触发）必须在 domain 层拒绝。
3. **authorization 脱敏规则 exclude=["id"] 必须同步单元测试**：改 authorization 规则时必须改 rule.rs 末尾 `authorization_identifier_fields_preserved` + `authorization_id_survives_end_to_end_redaction` 两个测试——禁止只改规则表不补测试。
4. **AuthorizationDetailDto.call_id 是前端关联授权单与工具调用记录的唯一桥**：禁止在 DTO 层删除或改名 call_id 字段——前端 FinanceToolCallEntries 依赖它来挂审批 Modal。
5. **审计增补只在 domain 层**：handler 层调 domain decide 时禁止自己写审计记录——统一由 tool_authorization.rs 内部增补（decision_reason / decider / decided_at_ms 三字段）。
6. **Expired 状态自动失效由 domain 层定时扫描**：禁止 handler 层手动把 Status 改为 Expired——domain 层在每个周期扫描 Active+ 超期 + Pending+ 超时 → 批量改 Expired。

## §5 历史演进（变更摘要）

- **基建落地**（commit 1c8d52d9 + 1fa62f22）：新建 `src/pkg/authorization/` 基建包 + `AuthorizationGate` trait + domain `tool_authorization.rs` DTO 契约。
- **拦截门融合**（commit 958d2161）：`policy_authorization_fusion.rs` 新建——Confirm 层级推导 + 短路（工具执行层发现已存在有效的 Approved 授权单直接放行，不重复建单）。
- **审批 handler 层**（commit 7a24522d + e883da7e）：S4-core 审批双 handler（decide_authorization / list_authorizations）+ 红线④⑤细化 + 审计增补；S4-management 管理面三 handler（create_request_authorization 主动申请 / list_authorizations 归属查询 / revoke_authorization 即时撤销）。
- **脱敏规则放行 + trace 关联**（commit 5c7db3d6 + 9bfd24f9）：authorization 规则追加 exclude=["id"] 放行 authorization_id 业务主键；policy_authorization_fusion 注入 call_id；common/src/api/tool.rs DTO 追加 call_id。
- **Err 分支透出**（commit c2f34359）：tool_execution Err 分支不再构造新 Error 丢弃底层 field——修复点在本卡 §4 兄弟卡的 §9 硬约束。
