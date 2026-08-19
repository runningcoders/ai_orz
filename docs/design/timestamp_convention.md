# 时间戳处理约定

> 🎯 **定位**：全项目时间戳单位、生成、存储、展示的统一规范；供所有层级（DAO/DAL/Domain/Handler/前端）参考
> 状态：定稿
> 触发场景：任何新增时间戳字段、生成时间戳、格式化时间戳时必读
>
> 关联文档：
> - [通用开发约定](../../AGENTS.md) — 命名与代码规范总览
> - [API 协议约定](api_protocol_convention.md) — 前后端 DTO 规范

---

## 一、核心决策

### 1.1 统一单位

| 决策项 | 约定 | 原因 |
|--------|------|------|
| **存储单位** | 全部 **毫秒级** Unix 时间戳（i64） | 前端格式化已统一假设毫秒级；精度更高；事件系统已用毫秒 |
| **时区** | UTC（系统时区） | 存储统一，展示时再按用户时区转换 |
| **字段类型** | `INTEGER NOT NULL`（SQLite）/ `i64`（Rust） | SQLite INTEGER 足够存毫秒级时间戳到 2262 年 |

### 1.2 标准工具函数

使用 `common::constants::utils` 模块的公共函数，**禁止本地重复实现**：

```rust
// ✅ 正确：使用公共工具
use common::constants::utils;
let now_ms = utils::current_timestamp_ms();

// ❌ 禁止：本地实现
fn current_timestamp() -> i64 { ... }  // 绝对禁止
Utc::now().timestamp_millis()         // 在非 DAO 层禁止
SystemTime::now().duration_since(...) // 绝对禁止
```

### 1.3 允许 `Utc::now().timestamp_millis()` 的场景

仅在 **DAO 层 SQL 写入前的本地变量** 场景允许使用（用于减少 import 依赖）：

```rust
// ✅ DAO 层允许
let now = chrono::Utc::now().timestamp_millis();
sqlx::query("INSERT ... created_at = $1").bind(now).execute(...)
```

其他层级（Model/Domain/Handler）**必须**使用 `common::constants::utils::current_timestamp_ms()`。

---

## 二、字段命名规范

### 2.1 标准时间字段

| 字段名 | 含义 | 必填 | 说明 |
|--------|------|------|------|
| `created_at` | 创建时间 | ✅ | 记录首次写入时间，不可变 |
| `updated_at` | 更新时间 | ✅ | 每次修改自动更新 |
| `deleted_at` | 软删除时间 | ❌ | 0 或 NULL 表示未删除 |

### 2.2 业务时间字段（按需添加）

| 字段名 | 含义 | 单位 |
|--------|------|------|
| `start_at` | 开始时间 | 毫秒 |
| `end_at` | 结束时间 | 毫秒 |
| `due_at` | 截止时间 | 毫秒 |
| `expire_at` | 过期时间 | 毫秒 |
| `indexed_at` | 最近索引时间 | 毫秒 |
| `last_followup_at` | 最近跟进时间 | 毫秒 |
| `last_updated_at` | 最近更新时间（业务语义） | 毫秒 |

### 2.3 PO 字段注释规范

```rust
/// 创建时间（毫秒级 Unix 时间戳）
pub created_at: i64,
/// 更新时间（毫秒级 Unix 时间戳）
pub updated_at: i64,
```

**必须**在每个时间字段的文档注释中明确标注「毫秒级」。

---

## 三、数据库层规范

### 3.1 新表默认值（DEFAULT）

```sql
-- ✅ 正确：毫秒级默认值
CREATE TABLE example (
    ...
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') * 1000 AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') * 1000 AS INTEGER)),
    ...
) STRICT;

-- ❌ 禁止：秒级默认值
DEFAULT (strftime('%s', 'now'))
```

### 3.2 新表无 DEFAULT

大多数表**不需要** DEFAULT 时间戳值（代码层在 PO 创建时已生成）。DEFAULT 仅用于 `tools`、`agent_tools` 等极少需要 SQL 层自动填充的关联表。

### 3.3 迁移脚本规范

新增或修改时间戳 DEFAULT 时，必须：

1. 创建新的迁移脚本（`migrations/YYYYMMDDHHMMSS_xxx.sql`）
2. 数据迁移时乘以 1000 转换
3. 同步修改 `20260420000000_initial.sql` 中的原始 DEFAULT 值

---

## 四、代码层规范

### 4.1 Model 层（PO 创建）

```rust
// ✅ 正确
pub struct ExamplePo {
    pub created_at: i64,
    pub updated_at: i64,
}

impl ExamplePo {
    pub fn new(...) -> Self {
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            created_at: now,
            updated_at: now,
            ...
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = common::constants::utils::current_timestamp_ms();
    }
}
```

### 4.2 DAO 层

```rust
// ✅ 正确：DAO 层可直接用 chrono
let now = chrono::Utc::now().timestamp_millis();
sqlx::query("INSERT INTO ... (created_at, updated_at, ...) VALUES ($1, $2, ...)")
    .bind(now)
    .bind(now)
    .execute(&mut *self.pool)
    .await?;
```

### 4.3 Handler 层

```rust
// ✅ 正确
use common::constants::utils;
po.updated_at = utils::current_timestamp_ms();

// ❌ 禁止：本地实现
// fn current_timestamp() -> i64 { ... }
```

### 4.4 时间运算

```rust
// ✅ 正确：毫秒级运算
let expires_at = common::constants::utils::current_timestamp_ms() + 3600 * 1000; // 1 小时后
let next_run = last_run + interval_seconds * 1000;

// ❌ 禁止：混淆秒和毫秒
let expires_at = current_timestamp() + 3600; // 秒 + 秒 = 秒（错在变量名/上下文误导）
```

---

## 五、事件层规范

所有事件模型的时间字段**已统一为毫秒级**：

| 事件 | 时间字段 | 单位 |
|------|----------|------|
| `TaskStatusEvent` | `timestamp` | 毫秒 |
| `ThinkRoundEvent` | `timestamp` | 毫秒 |
| `ToolExecuteEvent` | `timestamp` | 毫秒 |
| `AgentLoopEvent` | `timestamp` | 毫秒 |
| `AgentStateEvent` | `timestamp` | 毫秒 |
| `MessageEvent` | `timestamp` | 毫秒 |
| `CronTriggerEvent` | `timestamp` | 毫秒 |

事件层使用 `common::constants::utils::current_timestamp_ms()` 生成。

---

## 六、前端规范

### 6.1 格式化函数

前端 `frontend/src/utils/` 中所有时间格式化函数**已统一按毫秒级处理**：

```typescript
// 前端已统一：毫秒级时间戳直接传给 Date()
const date = new Date(timestamp); // timestamp 为毫秒级
```

### 6.2 新增格式化

新增格式化函数时，默认输入为毫秒级。若需要兼容旧数据（秒级），可使用：

```typescript
const toMilliseconds = (ts: number) => ts < 1e12 ? ts * 1000 : ts;
```

---

## 七、检查表

### 新增时间戳字段检查清单

- [ ] PO 字段类型为 `i64`
- [ ] 注释明确标注「毫秒级 Unix 时间戳」
- [ ] 使用 `common::constants::utils::current_timestamp_ms()` 生成
- [ ] 若需 DEFAULT，使用 `CAST(strftime('%s', 'now') * 1000 AS INTEGER)`
- [ ] 前端格式化函数验证通过
- [ ] 测试数据使用毫秒级

### 代码审查检查清单

- [ ] 是否有本地 `current_timestamp()` 实现？→ 删除
- [ ] 是否使用 `Utc::now().timestamp()`（秒级）？→ 改为 `timestamp_millis()`
- [ ] 是否有 `SystemTime::now()` 原始调用？→ 改为公共工具
- [ ] 时间运算是否单位正确？（秒转毫秒需 × 1000）
- [ ] Cron/Interval 运算是否正确转换单位？
