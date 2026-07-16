# 定时触发器前端体验优化计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 大幅优化定时触发器前端体验：Action 模板化、Cron 表达式支持、列表信息增强、编辑功能、JSON 校验

**Architecture:** 基于现有 Dioxus Router + Mistral CSS 设计系统 + 统一 API 客户端模式，复用现有组件（Modal/Button/Toast），在 `frontend/src/pages/system/triggers.rs` 中实现。

**Tech Stack:** Dioxus 0.7 + WebAssembly + Mistral CSS 设计系统

---

## 背景

当前定时触发器页面（`frontend/src/pages/system/triggers.rs`）功能可用但体验粗糙：

| 问题 | 影响 |
|------|------|
| Payload 手动写 JSON | 用户不知道格式，容易写错 |
| 列表只显示名称/cron/状态 | 信息密度低，看不出触发类型、下次执行时间 |
| 只有 interval + once 类型 | 缺失 Cron 表达式类型，后端 DAO 已支持 |
| 不能编辑触发器 | 修改只能删除重建 |
| 无 JSON 校验 | 格式错误要到后端返回才知道 |

## 后端现状（已存在，无需改动）

- **DTO**：`common/src/api/cron_trigger.rs` — `CreateCronTriggerRequest` / `UpdateCronTriggerRequest` / `CronTriggerDetail`（含 `next_run_at`、`last_run_at`、`trigger_type` 等完整字段）
- **TriggerType**：`common/src/enums/cron_trigger.rs` — Once(0) / Cron(1) / Interval(2) 三种类型
- **API 客户端**：`frontend/src/api/system.rs` — list/get/create/update/delete/pause/resume 完整
- **路由**：`/system/triggers`，组件 `SystemTriggers`

---

## 文件结构

| 文件 | 操作 | 说明 |
|------|------|------|
| `frontend/src/pages/system/triggers.rs` | 重写 | 主页面，包含列表+创建/编辑弹窗 |
| `frontend/src/components/cron_helper.rs` | 新建 | Cron 表达式辅助组件（常用预设+实时描述） |

---

## Task 1: 列表信息增强

**Files:**
- Modify: `frontend/src/pages/system/triggers.rs`

增强列表展示，充分利用 `CronTriggerDetail` 已有字段：

- [ ] **Step 1: 增加列表列**

表格列从 4 列扩展为 7 列：
- 名称（保留）
- 类型（新增）：显示"固定间隔" / "Cron表达式" / "一次性"
- 调度信息（替换 cron 列）：interval 显示 "每 5 分钟"，cron 显示表达式，once 显示执行时间
- 状态（保留）：运行中 / 暂停
- 下次执行（新增）：`next_run_at` 格式化时间
- 上次执行（新增）：`last_run_at` 格式化时间，无则显示"-"
- 操作（保留）：暂停/恢复 + 编辑 + 删除

- [ ] **Step 2: 添加格式化工具函数**

在文件内添加：
```rust
fn format_trigger_type(t: TriggerType) -> &'static str {
    match t {
        TriggerType::Once => "一次性",
        TriggerType::Cron => "Cron表达式",
        TriggerType::Interval => "固定间隔",
    }
}

fn format_schedule(t: &CronTriggerDetail) -> String {
    match t.trigger_type {
        TriggerType::Interval => {
            let secs = t.interval_seconds.unwrap_or(0);
            if secs < 60 { format!("每 {} 秒", secs) }
            else if secs < 3600 { format!("每 {} 分钟", secs / 60) }
            else if secs < 86400 { format!("每 {} 小时", secs / 3600) }
            else { format!("每 {} 天", secs / 86400) }
        }
        TriggerType::Cron => t.cron_expression.clone().unwrap_or_default(),
        TriggerType::Once => t.run_at.map(|ts| format_time(ts)).unwrap_or_default(),
    }
}

fn format_time(ts: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};
    let dt = Local.timestamp_opt(ts, 0).unwrap_or_else(|_| Local::now());
    dt.format("%Y-%m-%d %H:%M").to_string()
}
```

注意：检查 frontend/Cargo.toml 是否有 chrono 依赖。如果没有，改用简单的时间格式化函数（基于 `std::time` 手动拼接）。

- [ ] **Step 3: 运行 cargo check 验证**

Run: `cd frontend && cargo check`
Expected: PASS

---

## Task 2: Action 模板化 + JSON 校验

**Files:**
- Modify: `frontend/src/pages/system/triggers.rs`

将 Payload 文本框改为 Action 模板选择 + 参数填写，底层仍然生成 JSON payload 字符串。

- [ ] **Step 1: 定义 Action 模板**

```rust
struct ActionTemplate {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    fields: Vec<ActionField>,
}

struct ActionField {
    key: &'static str,
    label: &'static str,
    field_type: FieldType,
    required: bool,
    placeholder: &'static str,
    default_value: &'static str,
}

enum FieldType {
    Text,
    Number,
    Textarea,
}

fn get_action_templates() -> Vec<ActionTemplate> {
    vec![
        ActionTemplate {
            key: "agent_rest",
            label: "Agent 记忆沉淀",
            description: "定时触发 Agent 休息，将短期记忆沉淀为长期知识",
            fields: vec![
                ActionField {
                    key: "agent_id",
                    label: "Agent ID",
                    field_type: FieldType::Text,
                    required: true,
                    placeholder: "agent-xxx",
                    default_value: "",
                },
                ActionField {
                    key: "settle_limit",
                    label: "每次沉淀数量",
                    field_type: FieldType::Number,
                    required: false,
                    placeholder: "默认 10",
                    default_value: "10",
                },
            ],
        },
    ]
}
```

- [ ] **Step 2: 修改弹窗表单结构**

将原有的 Payload JSON 文本框改为：
1. Action 类型下拉选择（"自定义" + 各模板名称）
2. 当选择模板时，动态渲染对应的参数字段输入框
3. 当选择"自定义"时，显示原始 JSON 文本框 + 实时校验

实时 JSON 校验：
```rust
fn validate_json(s: &str) -> Option<String> {
    if serde_json::from_str::<serde_json::Value>(s).is_err() {
        Some("JSON 格式错误".to_string())
    } else {
        None
    }
}
```

- [ ] **Step 3: 实现模板转 payload 函数**

```rust
fn build_payload(action_key: &str, fields: &[ActionField], values: &HashMap<String, String>) -> String {
    let mut extra = serde_json::Map::new();
    for f in fields {
        if let Some(v) = values.get(f.key) {
            if !v.is_empty() {
                match f.field_type {
                    FieldType::Number => {
                        if let Ok(n) = v.parse::<i64>() {
                            extra.insert(f.key.to_string(), json!(n));
                        }
                    }
                    _ => {
                        extra.insert(f.key.to_string(), json!(v));
                    }
                }
            }
        }
    }
    let payload = json!({
        "action": action_key,
        "extra": extra,
    });
    payload.to_string()
}
```

- [ ] **Step 4: 运行 cargo check 验证**

Run: `cd frontend && cargo check`
Expected: PASS

---

## Task 3: 新增 Cron 表达式触发类型 + 常用预设

**Files:**
- Modify: `frontend/src/pages/system/triggers.rs`

在类型下拉中增加"Cron表达式"选项，提供常用预设和人类可读描述。

- [ ] **Step 1: 类型下拉增加 Cron 选项**

将 `new_type` 的选项从 interval/once 扩展为 interval/cron/once：
```
"固定间隔触发" (interval)
"Cron 表达式" (cron)
"一次性触发" (once)
```

- [ ] **Step 2: Cron 表达式输入 + 常用预设**

当类型为 cron 时，显示：
- Cron 表达式输入框（5 字段标准格式：分 时 日 月 周）
- 常用预设快捷按钮：
  - 每分钟：`* * * * *`
  - 每小时：`0 * * * *`
  - 每天 0 点：`0 0 * * *`
  - 每天 9 点：`0 9 * * *`
  - 每周一早 9 点：`0 9 * * 1`
  - 每月 1 号：`0 0 1 * *`
- 简单格式提示："分 时 日 月 周，如 0 9 * * * 表示每天 9 点"

- [ ] **Step 3: 更新创建逻辑**

在 `handle_create` 中，`"cron"` 分支：
```rust
"cron" => CreateCronTriggerRequest {
    name: new_name(),
    trigger_type: TriggerType::Cron,
    cron_expression: Some(new_cron_expr()),
    interval_seconds: None,
    run_at: None,
    payload: final_payload,
},
```

- [ ] **Step 4: 运行 cargo check 验证**

Run: `cd frontend && cargo check`
Expected: PASS

---

## Task 4: 新增编辑功能

**Files:**
- Modify: `frontend/src/pages/system/triggers.rs`

新增编辑弹窗，复用创建弹窗的表单结构。

- [ ] **Step 1: 添加编辑相关状态**

```rust
let mut show_edit_modal = use_signal(|| false);
let mut editing_id = use_signal(String::new);
let mut edit_name = use_signal(String::new);
let mut edit_type = use_signal(|| "interval".to_string());
let mut edit_interval = use_signal(|| "300".to_string());
let mut edit_cron_expr = use_signal(|| "0 * * * *".to_string());
let mut edit_action = use_signal(|| "agent_rest".to_string());
let mut edit_payload = use_signal(String::new);
let mut edit_fields_values = use_signal(HashMap::<String, String>::new);
let mut updating = use_signal(|| false);
```

- [ ] **Step 2: 添加编辑按钮到列表操作列**

在暂停/恢复和删除之间加"编辑"按钮，点击时：
1. 调用 `get_cron_trigger(id)` 获取详情
2. 填充编辑表单各字段
3. 解析 payload 回填 action 选择和参数
4. 打开编辑弹窗

- [ ] **Step 3: 实现 payload 解析回填**

```rust
fn parse_payload(payload: &str) -> (String, HashMap<String, String>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
        let action = v.get("action")
            .and_then(|a| a.as_str())
            .unwrap_or("custom")
            .to_string();
        let mut fields = HashMap::new();
        if let Some(extra) = v.get("extra") {
            if let Some(obj) = extra.as_object() {
                for (k, v) in obj {
                    fields.insert(k.clone(), v.to_string().trim_matches('"').to_string());
                }
            }
        }
        (action, fields)
    } else {
        ("custom".to_string(), HashMap::new())
    }
}
```

- [ ] **Step 4: 实现编辑提交**

```rust
let handle_update = move |_| {
    spawn(async move {
        // 构建 UpdateCronTriggerRequest
        // 调用 update_cron_trigger
        // 成功后关闭弹窗、刷新列表、toast 提示
    });
};
```

- [ ] **Step 5: 运行 cargo check 验证**

Run: `cd frontend && cargo check`
Expected: PASS

---

## Task 5: 列表刷新优化 + 空状态完善

**Files:**
- Modify: `frontend/src/pages/system/triggers.rs`

- [ ] **Step 1: 提取刷新函数**

将列表加载逻辑提取为 `load_triggers` 函数，避免各操作重复写加载代码。

- [ ] **Step 2: 手动刷新按钮**

在 card-header 标题旁边加一个刷新按钮（🔄），点击立即刷新。

- [ ] **Step 3: 操作成功 toast 提示**

所有操作（创建/编辑/暂停/恢复/删除）成功后用 `toast.success()` 提示。

- [ ] **Step 4: 运行 cargo check 验证**

Run: `cd frontend && cargo check`
Expected: PASS

---

## Task 6: CSS 样式完善

**Files:**
- Modify: `frontend/src/index.html`（如果需要）或内联 style

检查是否需要新增 CSS class：
- `.form-row` - 表单行布局
- `.form-col` - 表单列
- `.cron-presets` - Cron 预设按钮组
- `.json-error` - JSON 校验错误红色提示
- `.action-select` - Action 选择下拉
- `.trigger-type-badge` - 触发类型徽章

如果已有 CSS 变量和类够用就不用加。遵循 Mistral 设计系统。

- [ ] **Step 1: 审查现有 CSS 类是否足够**

检查 `frontend/src/index.html` 中现有的 form/table/button 相关类。

- [ ] **Step 2: 如有需要新增必要的 CSS 类**

只加必须的，复用现有类优先。

- [ ] **Step 3: 运行 cargo check 验证**

Run: `cd frontend && cargo check`
Expected: PASS

---

## API 接口（后端零改动）

前端全部复用已有 API：
- `GET /api/v1/system/cron-triggers` — 列表
- `GET /api/v1/system/cron-triggers/:id` — 详情（编辑回填用）
- `POST /api/v1/system/cron-triggers` — 创建
- `PUT /api/v1/system/cron-triggers/:id` — 更新
- `DELETE /api/v1/system/cron-triggers/:id` — 删除
- `POST /api/v1/system/cron-triggers/:id/pause` — 暂停
- `POST /api/v1/system/cron-triggers/:id/resume` — 恢复

---

## 验证计划

1. **功能验证**：
   - 列表显示 7 列信息正确
   - 三种触发器类型都能创建（interval/cron/once）
   - Action 模板生成 payload 正确
   - 自定义 JSON 实时校验
   - 编辑功能：打开回填正确，保存生效
   - 暂停/恢复/删除正常工作

2. **回归验证**：
   - 其他页面不受影响
   - 前端 cargo check 0 错误

---

*最后更新：2026-07-16*