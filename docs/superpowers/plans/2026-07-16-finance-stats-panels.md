# Finance 域统计面板补充计划

## 目标

为 Tool 和 ModelProvider 补充详情页统计面板，与 Agent/Project/Task 的统计面板保持一致的体验。

## 背景

- 后端已为 Tool 和 ModelProvider 的 GET 详情接口支持统计参数：`with_stats`/`with_model_call_stats`
- API 客户端 `get_tool`/`get_model_provider` 已扩展支持 `StatsOptions`
- 前端当前只有 Tool 和 ModelProvider 的列表页，**没有详情页**
- Agent/Project/Task 的统计面板已在前端完成集成

## 统计数据结构

### ToolStats
```rust
pub struct ToolStats {
    pub call_summary: Option<CallSummary>,  // total_calls, avg_qps, instant_qps
    pub failed_count: Option<u64>,
}
```

### ModelCallStats (ModelProvider)
```rust
pub struct ModelCallStats {
    pub call_summary: Option<CallSummary>,        // total_calls, avg_qps, instant_qps
    pub token_summary: Option<TokenSumResult>,    // total_tokens_input, total_tokens_output
    pub model_call_time_series: Option<Vec<TimeSeriesPoint>>,
}
```

## 任务清单

### Task 1: 新增统计面板组件

**文件**: `frontend/src/components/stats.rs`

新增两个组件：

- `ToolStatsPanel`: 展示调用次数、平均 QPS、瞬时 QPS、失败次数
- `ModelProviderStatsPanel`: 展示模型调用次数、平均 QPS、瞬时 QPS、输入/输出 Token

与已有面板风格保持一致（`StatsCard` 复用）。

### Task 2: 创建 Tool 详情页

**文件**: `frontend/src/pages/finance/tool_detail.rs`

参考 `project/task_detail.rs` 和 `hr/agent_detail.rs` 的设计：

1. 接收 `:id` 路由参数
2. 调用 `get_tool(id, Some(&stats_options))` 加载详情（带统计）
3. 展示 Tool 基本信息（名称、描述、协议、状态、标签、参数 Schema）
4. 集成 `ToolStatsPanel`
5. 提供状态切换、删除等操作

### Task 3: 创建 ModelProvider 详情页

**文件**: `frontend/src/pages/finance/model_provider_detail.rs`

参考 `hr/agent_detail.rs` 的设计：

1. 接收 `:id` 路由参数
2. 调用 `get_model_provider(id, Some(&stats_options))` 加载详情（带统计）
3. 展示 Provider 基本信息（名称、类型、模型、Base URL、描述）
4. 集成 `ModelProviderStatsPanel`
5. 提供测试连接、调用测试、删除等操作

### Task 4: 列表页添加跳转链接

**文件**: `frontend/src/pages/finance/tools.rs`

在工具列表的每一行添加"查看详情"按钮，跳转到 `/finance/tools/:id`。

**文件**: `frontend/src/pages/finance/model_providers.rs`

在 Provider 列表的每一行添加"查看详情"按钮，跳转到 `/finance/model-providers/:id`。

### Task 5: 路由配置

**文件**: `frontend/src/pages/mod.rs`

新增路由：
```rust
#[route("/finance/tools/:id")]
FinanceToolDetail { id: String },
#[route("/finance/model-providers/:id")]
FinanceModelProviderDetail { id: String },
```

### Task 6: 模块导出

**文件**: `frontend/src/pages/finance/mod.rs`

新增：
```rust
pub mod model_provider_detail;
pub mod tool_detail;
```

### Task 7: 全量验证

- `cargo check` 前端编译通过
- `cargo test` 后端测试通过（697 个）

## 影响范围

| 文件 | 操作 | 说明 |
|------|------|------|
| `frontend/src/components/stats.rs` | 编辑 | 新增 ToolStatsPanel、ModelProviderStatsPanel |
| `frontend/src/pages/finance/tool_detail.rs` | 新建 | Tool 详情页 |
| `frontend/src/pages/finance/model_provider_detail.rs` | 新建 | ModelProvider 详情页 |
| `frontend/src/pages/finance/tools.rs` | 编辑 | 添加详情跳转链接 |
| `frontend/src/pages/finance/model_providers.rs` | 编辑 | 添加详情跳转链接 |
| `frontend/src/pages/finance/mod.rs` | 编辑 | 导出新模块 |
| `frontend/src/pages/mod.rs` | 编辑 | 新增路由 + 导入 |

## 备注

- Tool 和 ModelProvider 详情页保持与已有详情页一致的视觉风格
- 统计面板在数据存在时自动渲染，无数据时不显示
- 所有组件使用 owned 值（clone）传递，符合 Dioxus 组件规范
