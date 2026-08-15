# 统一工具调用与装饰器收敛 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将工具装饰器逻辑收敛到 ToolCallDao 内部，统一 Auto/Manual 工具调用入口到 ToolDal，让 awakening 循环不再关心装饰细节。Manual 工具通过 `request_tool_call` / `send_tool_call_message` 两个特殊 tool 转发执行。

**Architecture:**

三层方法设计，避免循环调用：

```
上层（awakening 调用）:
  ToolDal.execute_auto(ctx, tool, args)   → Auto 工具直接执行
  ToolDal.execute_manual(ctx, tool, args) → Manual 工具通过特殊 tool 转发

中层（直接执行，含装饰器）:
  ToolDal.call_tool(ctx, tool, args)      → 装饰 + 调用 CoreTool::call
  （call_manual_tool_for_agent 内部调此方法，避免循环）

底层（DAO）:
  ToolCallDao.call_manual                  → clone + LoggingDecorator + call_with_entry
  ToolCallDao.decorate(tool)              → 内部装饰方法（供未来叠加 StatsDecorator）
```

`execute_manual` 内部流程：
1. 根据 `tool.po.config.dispatch_mode` 选择特殊 tool（`request_tool_call`=同步 / `send_tool_call_message`=异步）
2. 通过 `registry.create_tool(ToolPo)` 获取特殊 tool 的 CoreTool 实例（不加装饰器，特殊 tool 只是转发器）
3. 组织参数：把真实 `tool_id + args` 包装成特殊 tool 的参数格式
4. 调用特殊 tool 的 `CoreTool::call`
5. 特殊 tool handler 内部调 `call_manual_tool_for_agent` → `call_tool`（含装饰器，记录真实工具 trace）

**不循环的原因**：`execute_manual` → 特殊 tool → `call_manual_tool_for_agent` → `call_tool`（直接执行层），`call_tool` 不会再调 `execute_manual`。

**Tech Stack:** Rust, async-trait, serde_json, axum

---

## File Structure

**修改：**
- `src/service/dao/tool_call/mod.rs` - ToolCallDao trait 新增 `decorate` 方法（内部使用）
- `src/service/dao/tool_call/impl.rs` - 实现 `decorate`，确认 `call_manual` 使用装饰器
- `src/service/dal/tool.rs` - ToolDal trait 新增 `execute_auto` / `execute_manual`，保留 `call_tool` 作为直接执行层
- `src/service/domain/runtime/awakening.rs` - 简化循环，按 control_mode 调 `execute_auto` / `execute_manual`
- `src/handlers/finance/tool/request_tool_call.rs` - 去掉 `neural` 标记
- `src/handlers/finance/tool/send_tool_call_message.rs` - 去掉 `neural` 标记

**不修改：**
- `src/service/domain/runtime/tool_execution.rs` - `call_manual_tool_for_agent` 保持现有逻辑（调 `call_tool` 直接执行）
- `src/models/tool.rs` - `Tool.our_tool` 保持原始未装饰

---

## Task 1: ToolCallDao 新增内部 `decorate` 方法

**目标：** 装饰器逻辑收敛到 ToolCallDao，为未来叠加 StatsDecorator 等 middleware 做准备。

**Files:**
- Modify: `src/service/dao/tool_call/mod.rs`
- Modify: `src/service/dao/tool_call/impl.rs`

- [ ] **Step 1: 在 ToolCallDao trait 新增 `decorate` 方法**

修改 `src/service/dao/tool_call/mod.rs`，在 trait 中新增：

```rust
/// 装饰工具：应用 trace 记录装饰器
///
/// 内部方法，供 ToolCallDao 实现内部使用（如 call_manual）。
/// 未来可在此叠加 StatsDecorator 等多层装饰器，像 middleware 一样组合。
fn decorate(&self, tool: Box<dyn CoreTool + Send + Sync>) -> Box<dyn CoreTool + Send + Sync>;
```

- [ ] **Step 2: 在 ToolCallDaoImpl 实现 `decorate`**

修改 `src/service/dao/tool_call/impl.rs`：

```rust
fn decorate(&self, tool: Box<dyn CoreTool + Send + Sync>) -> Box<dyn CoreTool + Send + Sync> {
    Box::new(crate::pkg::tool_tracing::ToolCallLoggingDecorator::new(tool))
}
```

- [ ] **Step 3: 重构 `call_manual` 使用 `decorate`**

修改 `src/service/dao/tool_call/impl.rs` 中的 `call_manual`，用 `self.decorate()` 替代直接构造 `LoggingDecorator`：

```rust
async fn call_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: serde_json::Value,
) -> Result<(serde_json::Value, ToolCallEntry)> {
    let raw_tool = tool.our_tool.as_ref();
    let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&**raw_tool);
    // 使用 decorate 方法装饰（未来可叠加多层装饰器）
    let decorated = self.decorate(cloned);
    // call_with_entry 是 LoggingDecorator 的特有方法，需要 downcast
    // 但 decorate 返回 Box<dyn CoreTool>，无法直接调 call_with_entry
    // 解决：直接构造 LoggingDecorator（decorate 仅供 execute_auto 场景使用）
    let decorated = crate::pkg::tool_tracing::ToolCallLoggingDecorator::new(
        dyn_clone::clone_box(&**raw_tool),
    );
    let (result, entry) = decorated.call_with_entry(ctx, args).await;
    let value = result?;
    Ok((value, entry))
}
```

**说明：** `call_with_entry` 是 `LoggingDecorator` 特有方法（不在 `CoreTool` trait 上）。`call_manual` 需要 entry，必须直接构造 `LoggingDecorator`。`decorate` 方法供未来 `execute_auto` 场景使用（不需要 entry，只需 trace 被记录）。

- [ ] **Step 4: 验证编译通过**

```bash
cargo check --all-targets 2>&1 | head -30
```

- [ ] **Step 5: 提交**

```bash
git add src/service/dao/tool_call/mod.rs src/service/dao/tool_call/impl.rs
git commit -m "feat(tool_call): add internal decorate method to ToolCallDao"
```

---

## Task 2: ToolDal 新增 `execute_auto` 方法

**目标：** 提供 Auto 工具的统一执行入口，内部调用 `call_tool`（直接执行层）。

**Files:**
- Modify: `src/service/dal/tool.rs`

- [ ] **Step 1: 在 ToolDal trait 新增 `execute_auto` 方法签名**

修改 `src/service/dal/tool.rs`，在 trait 中新增（保留现有 `call_tool` / `call_manual` 不变）：

```rust
/// 执行 Auto 工具调用
///
/// 统一入口：awakening 循环调此方法执行 Auto 工具。
/// 内部调用 call_tool（直接执行层，含装饰器）。
async fn execute_auto(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<(Value, ToolCallEntry)>;
```

- [ ] **Step 2: 实现 `execute_auto`**

在 `ToolDalImpl` 中新增：

```rust
async fn execute_auto(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<(Value, ToolCallEntry)> {
    // Auto 工具直接执行（call_tool 内部含装饰器）
    self.call_tool(ctx, tool, args).await
}
```

- [ ] **Step 3: 验证编译通过**

```bash
cargo check --all-targets 2>&1 | head -30
```

- [ ] **Step 4: 提交**

```bash
git add src/service/dal/tool.rs
git commit -m "feat(tool_dal): add execute_auto method"
```

---

## Task 3: ToolDal 新增 `execute_manual` 方法（通过特殊 tool 转发）

**目标：** `execute_manual` 根据 `dispatch_mode` 选择同步/异步特殊 tool，通过 registry 创建实例并调用，把真实工具调用包装成特殊 tool 的参数转发出去。

**Files:**
- Modify: `src/service/dal/tool.rs`

- [ ] **Step 1: 添加 `dispatch_mode` 解析辅助函数**

在 `src/service/dal/tool.rs` 顶部添加：

```rust
/// 从 ToolPo.config 解析 dispatch_mode，默认 "sync"
///
/// 复用现有 config JSON 字段表达同步/异步属性，不改 schema。
/// config 示例：{ "dispatch_mode": "async" }
fn parse_dispatch_mode(tool: &Tool) -> &'static str {
    if let Some(config) = &tool.po.config {
        if let Some(mode) = config.get("dispatch_mode").and_then(|v| v.as_str()) {
            if mode == "async" {
                return "async";
            }
        }
    }
    "sync"
}
```

- [ ] **Step 2: 在 ToolDal trait 新增 `execute_manual` 方法签名**

```rust
/// 执行 Manual 工具调用（通过特殊 tool 转发）
///
/// 统一入口：awakening 循环调此方法执行 Manual 工具。
/// 内部根据 dispatch_mode 分发：
/// - sync（默认）：通过 request_tool_call 特殊 tool 转发（同步执行）
/// - async：通过 send_tool_call_message 特殊 tool 转发（异步派发）
///
/// 特殊 tool 不加装饰器（它只是转发器），真实工具的 trace 在 call_tool 层记录。
async fn execute_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<(Value, ToolCallEntry)>;
```

- [ ] **Step 3: 实现 `execute_manual`**

在 `ToolDalImpl` 中新增：

```rust
async fn execute_manual(
    &self,
    ctx: RequestContext,
    tool: &Tool,
    args: Value,
) -> Result<(Value, ToolCallEntry)> {
    let mode = parse_dispatch_mode(tool);
    let special_tool_id = match mode {
        "async" => "send_tool_call_message",
        _ => "request_tool_call",
    };

    // 通过 registry 创建特殊 tool 实例（不加装饰器，特殊 tool 只是转发器）
    let special_po = ToolPo::new(
        special_tool_id.to_string(),
        special_tool_id.to_string(),
        "manual tool dispatcher".to_string(),
        common::enums::tool::ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        None,
    );
    let special_tool = crate::pkg::tool_registry::get_registry()
        .create_tool(special_po)
        .ok_or_else(|| {
            common::error::err!(
                Internal,
                "special tool {} not found in registry",
                special_tool_id
            )
        })?;

    // 组织参数：把真实工具调用包装成特殊 tool 的参数
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| common::error::err!(InvalidRequest, "Manual 工具调用缺少 Agent 上下文"))?
        .clone();
    let project_id = ctx.project_id().cloned();
    let task_id = ctx.task_id().cloned();

    let special_args = serde_json::json!({
        "tool_id": tool.po.id,
        "tool_name": tool.po.name,
        "params": args,
        "project_id": project_id,
        "task_id": task_id,
    });

    // 调用特殊 tool（不加装饰器）
    // 特殊 tool handler 内部会调 call_manual_tool_for_agent → call_tool（含装饰器）
    let result_value = special_tool.call(ctx.clone(), special_args).await?;

    // 构造占位 entry（真实 trace 在 call_tool 层记录）
    let entry = ToolCallEntry {
        call_id: format!("manual_dispatch_{}_{}", special_tool_id, uuid::Uuid::now_v7()),
        tool_id: tool.po.id.clone(),
        ..Default::default()
    };

    Ok((result_value, entry))
}
```

- [ ] **Step 4: 确认 `ToolCallEntry` 实现 `Default`**

如果 `ToolCallEntry` 未实现 `Default`，需要为其派生：

```bash
grep -n "struct ToolCallEntry" src/pkg/tool_tracing/
```

找到定义位置，添加 `Default` derive 或手动实现。如果字段都有默认值，加 `#[derive(Default)]`。

- [ ] **Step 5: 确认 `CoreTool::call` 的参数类型**

检查 `CoreTool` trait 的 `call` 方法签名，确认参数是 `Value` 还是其他类型：

```bash
grep -n "fn call" src/models/tool.rs | head -5
```

如果 `call` 接收 `Value`，则 Step 3 的 `special_args` 直接传入即可。

- [ ] **Step 6: 验证编译通过**

```bash
cargo check --all-targets 2>&1 | head -50
```

- [ ] **Step 7: 提交**

```bash
git add src/service/dal/tool.rs
git commit -m "feat(tool_dal): add execute_manual with special tool dispatch"
```

---

## Task 4: 简化 awakening 循环

**目标：** awakening 循环根据 `control_mode` 调 `execute_auto` / `execute_manual`，移除循环内的 clone+decorate 逻辑。

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

- [ ] **Step 1: 确认 RuntimeDomainImpl 有 `tool_dal()` 方法**

```bash
grep -n "fn tool_dal\|tool_dal()" src/service/domain/runtime/mod.rs
```

如果没有，在 `RuntimeDomainImpl` 中添加：

```rust
fn tool_dal(&self) -> std::sync::Arc<dyn crate::service::dal::tool::ToolDal> {
    crate::service::dal::tool::dal()
}
```

- [ ] **Step 2: 重写工具调用循环**

修改 `src/service/domain/runtime/awakening.rs` 的工具调用循环（约第 240-300 行），替换为：

```rust
let think_result =
    match tokio::time::timeout(std::time::Duration::from_secs(THINK_TIMEOUT_SECS), async {
        let mut messages = vec![ChatMessage::user(prompt.clone())];
        for _ in 0..MAX_TOOL_ITERATIONS {
            let result = self
                .brain_dal()
                .think(ctx.clone(), brain, &messages, &tool_descriptors)
                .await?;
            match result {
                ThinkResult::Final { content, .. } => return Ok(content),
                ThinkResult::ToolCall {
                    content,
                    tool_calls,
                    ..
                } => {
                    // 追加助手消息（含 tool_calls）
                    messages.push(ChatMessage::Assistant {
                        content,
                        tool_calls: Some(tool_calls.clone()),
                    });
                    // 按 control_mode 分发执行
                    for tc in tool_calls {
                        match agent.tools().iter().find(|t| t.po.name == tc.name) {
                            Some(tool) => {
                                let call_result = match tool.po.control_mode {
                                    common::enums::tool::ControlMode::Auto => {
                                        self.tool_dal()
                                            .execute_auto(ctx.clone(), tool, tc.arguments)
                                            .await
                                    }
                                    common::enums::tool::ControlMode::Manual => {
                                        self.tool_dal()
                                            .execute_manual(ctx.clone(), tool, tc.arguments)
                                            .await
                                    }
                                };
                                match call_result {
                                    Ok((value, _entry)) => {
                                        messages.push(ChatMessage::tool(
                                            tc.id,
                                            format!("{}", value),
                                        ));
                                    }
                                    Err(e) => {
                                        messages.push(ChatMessage::tool(
                                            tc.id,
                                            format!("Error: {}", e),
                                        ));
                                    }
                                }
                            }
                            None => {
                                messages.push(ChatMessage::tool(
                                    tc.id,
                                    format!("Error: tool {} not found", tc.name),
                                ));
                            }
                        }
                    }
                }
            }
        }
        Err(err!(
            Internal,
            "think loop exceeded max {} iterations",
            MAX_TOOL_ITERATIONS
        ))
    })
    .await
    {
        Ok(result) => result,
        Err(_elapsed) => Err(err!(
            Internal,
            "brain think timeout after {}s",
            THINK_TIMEOUT_SECS
        )),
    };
```

- [ ] **Step 3: 移除不再使用的 import**

删除 `awakening.rs` 中不再需要的 `dyn_clone` 和 `ToolCallLoggingDecorator` 相关代码（如果 clippy 报 unused import）。

- [ ] **Step 4: 验证编译通过**

```bash
cargo check --all-targets 2>&1 | head -50
```

- [ ] **Step 5: 提交**

```bash
git add src/service/domain/runtime/awakening.rs src/service/domain/runtime/mod.rs
git commit -m "refactor(awakening): simplify tool call loop using execute_auto/execute_manual"
```

---

## Task 5: 去掉两个 handler 的 `neural` 标记

**目标：** `request_tool_call` 和 `send_tool_call_message` 不再作为神经工具暴露给 Agent，保留为普通 HTTP handler 和 Builtin tool。

**Files:**
- Modify: `src/handlers/finance/tool/request_tool_call.rs`
- Modify: `src/handlers/finance/tool/send_tool_call_message.rs`

- [ ] **Step 1: 去掉 `request_tool_call` 的 `neural` 标记**

修改 `src/handlers/finance/tool/request_tool_call.rs`，从 `#[register_handler_tool(...)]` 中移除 `neural`：

```rust
#[register_handler_tool(
    id = "request_tool_call",
    name = "request_tool_call",
    description = "Call a manual tool synchronously and get the result immediately",
    params = "common::api::RequestToolCallParams",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn request_tool_call(
    ctx: RequestContext,
    params: RequestToolCallParams,
) -> Result<RequestToolCallResponse> {
    // 函数体保持不变
    let mut builder = ctx.to_builder();
    if let Some(project_id) = &params.project_id {
        builder = builder.project_id(project_id.clone());
    }
    if let Some(task_id) = &params.task_id {
        builder = builder.task_id(task_id.clone());
    }
    let ctx = builder.build();

    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| common::error::err!(InvalidRequest, "当前请求缺少 Agent 上下文"))?
        .clone();

    let result = runtime::domain()
        .tool_execution()
        .call_manual_tool_for_agent(ctx, agent_id, params.tool_id, params.params)
        .await?;

    Ok(RequestToolCallResponse {
        tool_call_id: result.trace_ref.call_id,
        status: "completed".to_string(),
        result: result.result,
    })
}
```

- [ ] **Step 2: 去掉 `send_tool_call_message` 的 `neural` 标记**

修改 `src/handlers/finance/tool/send_tool_call_message.rs`，从 `#[register_handler_tool(...)]` 中移除 `neural`：

```rust
#[register_handler_tool(
    id = "send_tool_call_message",
    name = "send_tool_call_message",
    description = "Dispatch a manual tool call asynchronously. Returns immediately with a request_id; the tool result arrives later via a ToolCallResult message in the next awaken round. Use this to invoke manual tools without blocking.",
    params = "common::api::SendToolCallMessageParams",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn send_tool_call_message(
    ctx: RequestContext,
    params: SendToolCallMessageParams,
) -> Result<SendToolCallMessageResponse> {
    // 函数体保持不变
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| common::error::err!(InvalidRequest, "当前请求缺少 Agent 上下文"))?
        .clone();

    let request_id = uuid::Uuid::now_v7().to_string();

    let cmd = SendToolCallRequestCommand {
        request_id: &request_id,
        tool_id: &params.tool_id,
        tool_name: &params.tool_name,
        from_agent_id: &agent_id,
        to_executor_id: "system",
        project_id: params.project_id.as_deref(),
        task_id: params.task_id.as_deref(),
        reply_to_id: None,
        args: params.params,
    };

    let message = message::domain()
        .delivery()
        .send_tool_call_request(ctx, cmd)
        .await?;

    Ok(SendToolCallMessageResponse {
        request_id,
        message_id: message.po.id,
        status: "dispatched".to_string(),
    })
}
```

- [ ] **Step 3: 验证编译通过**

```bash
cargo check --all-targets 2>&1 | head -30
```

- [ ] **Step 4: 提交**

```bash
git add src/handlers/finance/tool/request_tool_call.rs src/handlers/finance/tool/send_tool_call_message.rs
git commit -m "refactor(handlers): remove neural flag from manual tool dispatchers"
```

---

## Task 6: 集成测试验证

**目标：** 运行集成测试，确保统一调用路径正确工作。

- [ ] **Step 1: 运行 CI 默认测试**

```bash
cargo test --test tool_call_test
```

预期：Part A（3 tests）+ Part B（2 tests）passed，Part C（1 test）ignored

- [ ] **Step 2: 运行真实 LLM 测试**

```bash
cargo test --test tool_call_test test_real_llm_auto_tool_call -- --ignored
```

预期：PASS（验证统一调用路径）

- [ ] **Step 3: 运行 awaken 测试**

```bash
cargo test --test agent_awaken_test -- --ignored
```

预期：PASS

- [ ] **Step 4: 如有失败，分析修复**

常见问题：
- `execute_manual` 调用特殊 tool 失败：检查 registry.create_tool 是否返回 Some
- 参数格式不匹配：检查 special_args 的 JSON 结构是否匹配 `RequestToolCallParams` / `SendToolCallMessageParams` 的字段名
- `ToolCallEntry::default()` 编译失败：确认 Default trait 已派生

- [ ] **Step 5: 提交（如有修复）**

```bash
git add .
git commit -m "test: verify unified tool execution path"
```

---

## Task 7: fmt + clippy 全量检查

- [ ] **Step 1: 运行 fmt**

```bash
cargo fmt --all
cargo fmt --all -- --check
```

- [ ] **Step 2: 运行 clippy（backend）**

```bash
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 3: 运行 clippy（frontend）**

```bash
cargo clippy -p frontend --target wasm32-unknown-unknown --all-targets -- -D warnings
```

- [ ] **Step 4: 修复所有警告**

- [ ] **Step 5: 提交**

```bash
git add .
git commit -m "chore: fmt and clippy fixes"
```

---

## Task 8: 推送到远程

- [ ] **Step 1: 推送**

```bash
git push origin main
```

- [ ] **Step 2: 确认 CI 通过**

---

## Self-Review Checklist

1. **Spec coverage:**
   - 观点1（assemble_core_tool 保持原始，提供装饰方法）：Task 1 新增 `decorate`，不改 `assemble_core_tool`，`Tool.our_tool` 保持原始 ✅
   - 观点2（统一工具调用逻辑，通过特殊 tool 转发）：Task 2-3 提供 `execute_auto` / `execute_manual`，`execute_manual` 通过 registry 创建特殊 tool 实例并调用 ✅
   - 观点3（同步/异步复用现有字段）：Task 3 用 `ToolPo.config.dispatch_mode` 表达，不改 schema ✅
   - 两个 handler 保留去 neural：Task 5 ✅
   - 装饰器只在 ToolCallDao 内收敛：Task 1 ✅
   - 不对特殊 tool 做额外包装：Task 3 直接调用 `CoreTool::call`，不加装饰器 ✅

2. **Placeholder scan:**
   - 无 TBD / TODO
   - 所有代码片段完整
   - 所有命令明确

3. **Type consistency:**
   - `execute_auto` / `execute_manual` 签名一致：`(ctx, tool, args) -> Result<(Value, ToolCallEntry)>`
   - `call_tool` 保留为直接执行层，不变
   - `ControlMode::Auto` / `Manual` 保持不变
   - `dispatch_mode` 解析返回 `&'static str`

4. **循环调用风险:**
   - `execute_manual` → 特殊 tool → `call_manual_tool_for_agent` → `call_tool`（直接执行）
   - `call_tool` 不调 `execute_auto` / `execute_manual`，无循环 ✅

5. **风险点:**
   - Task 3 Step 3 的 `special_args` JSON 结构需匹配 `RequestToolCallParams` / `SendToolCallMessageParams` 的字段名（snake_case）
   - Task 3 Step 4 的 `ToolCallEntry::default()` 需确认 Default 已派生
   - `CoreTool::call` 的返回类型是 `Result<Value>` 还是其他，需确认
