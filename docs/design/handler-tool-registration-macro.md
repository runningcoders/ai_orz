# 工具注册宏设计文档

## 设计目标

将现有的 HTTP Handler 直接注册为内置工具，复用 Handler 已有的：
- 完整权限校验
- 参数解析逻辑
- 错误处理
- 业务逻辑

保证 HTTP API 和工具调用行为一致，降低重复开发量，便于扩展。

## 架构设计

### 整体结构

```
ai-orz-macros/ (独立 proc-macro crate)
└── src/lib.rs
    └── register_handler_tool - 属性宏实现

src/pkg/tool_registry/handler_adapter/
├── mod.rs
│   ├── HandlerFn trait - 处理函数 trait
│   ├── GenericHandlerFn - 泛型实现
│   ├── HandlerToolAdapter - 适配器实现 CoreTool
│   └── 重导出宏
└── macros.rs
    └── 重导出 ai-orz-macros::register_handler_tool
```

### 核心 Trait

```rust
/// Trait for handler functions that can be adapted to CoreTool
///
/// This trait is implemented for handler functions that:
/// - Take `RequestContext` + parsed parameters
/// - Return `Result<Value, AppError>`
/// - `T` is `Serialize` for JSON response
#[async_trait]
pub trait HandlerFn<Params>: Send + Sync + DynClone
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, ctx: RequestContext, params: Params) -> Result<Value, AppError>;
}

dyn_clone::clone_trait_object!(<Params> HandlerFn<Params> where Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static);
```

### 适配器结构

```rust
/// Adapter that converts a Handler to CoreTool
#[derive(Clone)]
pub struct HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    po: ToolPo,
    parameters_schema: Value,
    inner: Box<dyn HandlerFn<Params>>,
    _phantom: PhantomData<Params>,
}
```

`CoreTool` 实现：

```rust
#[async_trait]
impl<Params> CoreTool for HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, mut ctx: RequestContext, args: Value) -> Result<Value, ToolError> {
        // Parse JSON args to Params type
        let params: Params = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => {
                return Err(ToolError::JsonError(e));
            }
        };

        match self.inner.call(ctx, params).await {
            Ok(result) => Ok(result),
            Err(app_error) => Err(ToolError::ToolCallError(app_error.to_string().into())),
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}
```

## 属性宏语法

### 使用方式

```rust
#[register_handler_tool(
    id = "list_skill_files",
    name = "list_skill_files",
    description = "List all files in a skill",
    params = "common::api::ListSkillFilesParams",
)]
async fn list_skill_files_handler(ctx: RequestContext, params: ListSkillFilesParams) -> Result<Value, AppError> {
    // implementation...
}
```

### 参数说明

| 参数 | 必须 | 说明 | 示例 |
|------|------|------|------|
| `id` | 是 | 工具唯一 ID | `id = "list_skill_files"` |
| `name` | 是 | 工具显示名称 | `name = "list_skill_files"` |
| `description` | 是 | 工具描述（给 LLM 看） | `description = "List all files in a skill"` |
| `params` | 是 | 参数类型完整路径 | `params = "common::api::ListSkillFilesParams"` |

### 宏生成代码

宏会自动生成：

1. **工厂结构体**：`{ID_TO_UPPER}_FACTORY` 例如 `LIST_SKILL_FILES_FACTORY`
2. **实现 `BuiltinToolFactory`**：
   - `create_po()` - 创建 ToolPo，自动从参数生成 JSON Schema
   - `create()` - 创建 `HandlerToolAdapter` 实例
3. **自动注册**：使用 `ctor` 宏在程序启动时自动注册到全局注册表

## 使用流程

### 新 Handler 注册为工具

1. 按项目约定实现 Handler 核心逻辑，签名为：
   ```rust
   async fn handler_name(ctx: RequestContext, params: ParamsType) -> Result<Value, AppError>
   ```
   > 注意：原 HTTP handler 的提取 path/query 参数部分已经在 handler 入口处理了，核心逻辑就是接收 `RequestContext` + 解析好的参数结构体

2. 添加属性宏：
   ```rust
   #[register_handler_tool(
       id = "handler_id",
       name = "handler_name",
       description = "description for LLM",
       params = "path::to::ParamsType",
   )]
   ```

3. 编译完成！宏自动生成工厂和注册代码，无需其他操作。

### 现有 Handler 改造为工具

现有 HTTP Handler 需要做一点小重构：

1. **拆分核心逻辑**：将原来 axum extractor 后的核心逻辑抽出来，变成：
   ```rust
   // 新：核心逻辑，供工具调用
   pub async fn list_skill_files(
       ctx: RequestContext,
       params: ListSkillFilesParams,
   ) -> Result<Value, AppError> {
       // ... 原有逻辑 ...
   }

   // 保留：HTTP 入口，调用核心逻辑
   pub async fn list_skill_files_handler(
       State(state): State<AppState>,
       Path(params): Path<ListSkillFilesParams>,
       Extractor(ctx): Extractor<RequestContext>,
   ) -> Response {
       let params = ListSkillFilesParams::from(params);
       list_skill_files(ctx, params).await.into()
   }
   ```

2. 在抽出来的核心逻辑上添加 `#[register_handler_tool]` 属性宏即可。

这样 HTTP 和工具共享同一份核心逻辑，保证行为一致性。

## 错误处理

| 场景 | 错误来源 | 转换方式 |
|------|----------|----------|
| JSON 参数解析失败 | `serde_json::from_value` | `ToolError::JsonError(e)` |
| Handler 业务错误 | `AppError` | `ToolError::ToolCallError(app_error.to_string().into())` |

## JSON Schema 生成

利用 `schemars::schema_for!` 自动从 `Params` 类型生成 JSON Schema，不需要手写。

## 优势

1. **零重复代码**：核心逻辑一份，HTTP 和工具共用
2. **一致性**：权限校验、错误处理行为一致
3. **自动注册**：加个属性宏就完事，不需要手动改注册表
4. **类型安全**：参数解析由 serde 完成，编译时检查
5. **参数文档自动生成**：JSON Schema 自动从类型生成

## 待验证

- [ ] 实际测试一个现有 Handler 改造，验证可用性
- [ ] 确认 `ctor` 全局注册是否正常工作
- [ ] 确认 Rig 调用流程是否能正常调用

## 更新记录

| 日期 | 更新内容 | 作者 |
|------|----------|------|
| 2026-06-21 | 初始设计文档完成 | AI Orz |
