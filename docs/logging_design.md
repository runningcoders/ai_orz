# 日志系统设计文档

## 概述

ai_orz 项目采用基于 `tracing` 库的统一日志宏系统，提供上下文感知的结构化日志能力。日志系统完全通过宏实现，零成本抽象，编译后等价于直接调用 tracing。

---

## 核心设计原则

### 1. 完全宏化

所有日志调用统一使用宏形式，不再使用函数调用。宏的优势：

- ✅ 支持完整的 tracing 格式化语法（`{}`, `{:?}`, `%`, `?` 等）
- ✅ 支持结构化字段（key-value 对）
- ✅ 零成本抽象，编译时展开
- ✅ 自动捕获代码位置（文件、行号）

### 2. 自动上下文检测

宏通过语法模式匹配自动检测调用形式，无需手动区分不同宏：

| 检测机制 | 模式 | 行为 |
|---------|------|------|
| **第一个参数是字符串字面量** | `log_info!("message")` | 无上下文模式，直接输出日志 |
| **第一个参数非字符串 + 第二个是字符串字面量** | `log_info!(&ctx, "op", "msg")` | 带上下文模式，自动创建 tracing span |

### 3. 匹配顺序（关键设计）

宏匹配按优先级从高到低：

1. **优先匹配无上下文模式**：检测第一个参数是否为字符串字面量
2. **兜底匹配带上下文模式**：第一个参数为表达式，第二个为字符串字面量

> 💡 **为什么这个顺序？**
> 
> 如果先匹配带上下文模式，`log_info!("operation", "message")` 会被误判为带上下文模式（第一个字符串会被当作 &ctx，第二个字符串被当作 operation）。
> 
> 正确的设计是：**字符串字面量永远首先被当作消息，而不是 operation**。

---

## API 参考

### 宏列表

| 宏名 | 级别 | 用途 |
|------|------|------|
| `log_info!` | INFO | 常规信息 |
| `log_warn!` | WARN | 警告信息 |
| `log_error!` | ERROR | 错误信息 |
| `log_debug!` | DEBUG | 调试信息 |
| `sys_info!` | INFO | 系统级无上下文日志（别名） |
| `sys_warn!` | WARN | 系统级警告（别名） |
| `sys_error!` | ERROR | 系统级错误（别名） |
| `sys_debug!` | DEBUG | 系统级调试（别名） |

> 💡 `sys_*` 系列是向后兼容的别名，等价于无上下文模式的 `log_*`。

---

## 使用规范

### 1. 无上下文日志（系统级别）

**适用场景**：初始化、配置加载、后台任务等没有请求上下文的场景。

```rust
// 简单字符串
log_info!("application started");

// 带格式化参数
log_info!("config loaded from {}", path);

// 带结构化字段
log_info!(count = 42, status = "ok", "batch processed");
```

### 2. 带上下文日志（请求级别）

**适用场景**：Handler、Service、DAL 等有请求上下文的场景。

**签名规范**：
```rust
log_{level}!(&ctx, "operation_name", <tracing fields>);
```

**参数说明**：
- `&ctx`：`&RequestContext` 引用，**必须传引用**
- `"operation_name"`：操作名称，字符串字面量，描述当前操作
- `<tracing fields>`：标准 tracing 日志内容

**示例**：
```rust
// DAL 层示例
log_info!(&ctx, "create_memory", "created memory id={}", memory_id);

// Handler 层示例
log_warn!(&ctx, "validate_input", "invalid email format: {}", email);

// 错误日志
log_error!(&ctx, "update_project", "db error: {:?}", err);
```

### 3. 上下文包含字段

当使用带上下文模式时，`create_span` 自动将以下字段添加到日志 span：

| 字段 | 说明 |
|------|------|
| `log_id` | 请求唯一标识 UUID |
| `user_id` | 当前用户 ID |
| `operation` | 操作名称（传入的第二个参数） |

**输出示例**：
```
INFO create_project: ai_orz::dal::project: created project id=123 user_id=456 log_id=abc-xyz
```

---

## 核心实现机制

### 宏定义结构（以 `log_info!` 为例）

```rust
macro_rules! log_info {
    // 分支 1: 无上下文模式（优先匹配）
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::info!($msg $(, $($fields)*)?);
    }};
    
    // 分支 2: 带上下文模式（兜底匹配）
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($op, $ctx);
        let _guard = span.enter();
        tracing::info!($($fields)*);
    }};
}
```

### 关键技术点

#### 1. `literal` 匹配器

使用 `$msg:literal` 精确匹配**字符串字面量**，这是区分两种模式的核心机制。

#### 2. `expr` 匹配器

`$ctx:expr` 匹配任意表达式（包括 `&ctx` 引用）。

#### 3. `tt` 匹配器

`$($fields:tt)*` 匹配剩余的所有 token 树，完整透传给 tracing 宏，支持所有 tracing 语法。

---

## 迁移指南

### 从旧函数调用迁移

**之前（已废弃）**：
```rust
logging::info("some message");  // ❌ 已删除
```

**现在**：
```rust
log_info!("some message");      // ✅
```

### 从旧宏调用迁移

**之前**：
```rust
sys_info!("system message");    // 仍可用（兼容别名）
sys_info!(ctx, "op", "msg");    // ❌ 旧形式已不支持
```

**现在**：
```rust
log_info!("system message");                    // ✅ 无上下文
log_info!(&ctx, "operation", "message {}", x);  // ✅ 带上下文（必须传 &ctx）
```

### 所有权问题

**重要**：`create_span` 只接受 `&RequestContext` 引用，不接受所有权转移。

✅ **正确**：
```rust
log_info!(&ctx, "operation", "message");
```

❌ **错误**（编译错误）：
```rust
log_info!(ctx, "operation", "message");  // 传值而非引用
```

---

## tracing 语法速查

### 基本格式化

```rust
// Display 格式
log_info!("user {} logged in", user_id);

// Debug 格式
log_debug!("config: {:?}", config);

// 漂亮打印
log_debug!("config: {:#?}", config);
```

### 结构化字段

```rust
// 命名字段
log_info!(user_id = 123, action = "login", "user logged in");

// 简写（变量名即字段名）
let count = 42;
log_info!(count, "processed");  // 等价于 count = count
```

### 特殊前缀

```rust
// % 前缀 = Display
log_info!(%user, "user info");

// ? 前缀 = Debug
log_debug!(?config, "config loaded");
```

---

## 转义规则

在 tracing 宏字符串中，以下字符有特殊含义，需要转义：

| 字符 | 含义 | 转义方式 |
|------|------|---------|
| `{` | 格式化占位符开始 | `{{` |
| `}` | 格式化占位符结束 | `}}` |
| `%` | Display 前缀 | 字符串中正常使用无需转义 |
| `?` | Debug 前缀 | 字符串中正常使用无需转义 |

**示例**：
```rust
// 输出: json {"key": "value"}
log_info!("json {{\"key\": \"value\"}}");
```

---

## 文件位置

| 文件 | 内容 |
|------|------|
| `src/lib.rs` | 统一日志宏定义（4 个主宏 + 4 个别名） |
| `src/pkg/logging.rs` | `create_span` 辅助函数 |
| `src/pkg/logging_test.rs` | 日志模块单元测试 |

---

## 提交历史

| Commit | 说明 |
|--------|------|
| `f8baaff` | 修正宏匹配顺序，优先检测第一个参数是否为字符串 |
| `04bac1e` | 8 个宏合并为 4 个，自动检测上下文模式 |
| `4f21fd7` | 日志模块完全宏化，删除旧函数实现 |

---

## 最佳实践

### ✅ 推荐

1. **所有业务代码统一使用 `log_*` 宏**，不直接调用 `tracing::*!`
2. **带上下文场景必须传 `&ctx` 引用**，包含 log_id 便于追踪
3. **operation 命名规范**：使用动词+名词，如 `create_memory`, `update_project`
4. **operation 必须是字符串字面量**，不能是变量（宏匹配要求）

### ❌ 避免

1. 不混用 `sys_*` 和 `log_*`（除非确实是系统级日志）
2. 不在带上下文模式中省略 operation
3. 不传 `ctx` 值（必须传 `&ctx` 引用）

---

## 常见问题

### Q: 为什么不做类型检测自动识别 ctx？

宏只能做**语法层面**的模式匹配，不能做**类型检测**。在宏展开时，类型信息还不存在。

### Q: 为什么 operation 必须是字符串字面量？

因为我们用 `$op:literal` 匹配器来区分两种模式，这是最可靠的语法级检测方式。

### Q: 为什么 ctx 必须传引用？

为了避免所有权转移问题，RequestContext 通常在整个请求生命周期内存在，传引用更符合 Rust 惯例。

### Q: 可以在 tracing 字段中使用结构化日志吗？

完全可以！所有 tracing 支持的语法都可以透传使用。
