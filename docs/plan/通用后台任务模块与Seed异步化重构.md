# 通用后台任务模块与Seed异步化重构

> 🎯 **本文档定位**：重构规划 + 落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 新增后台异步任务时，回看"BackgroundTask trait 契约 + 装饰模式向后兼容"两处
> - 若需了解 registry 注册执行或 progress 装饰响应细节，直接跳转对应代码文件（见 §涉及文件）
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范（trait 位置约定）
> - [向量存储架构设计](../design/) — 向量重建流程（BackgroundTask 典型收编对象）

---

## 一、重构目标（为什么做）

系统初始化和向量重建各写了一套后台任务（INIT_TASKS static、RebuildTask 字段），状态不统一、前端无法复用进度组件，seed 导出/恢复是同步阻塞 HTTP 请求（大快照超时）。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 初始化 / 向量重建 / Seed 导出-恢复-应用默认 3 类异步任务各自维护状态，5 处重复代码 | 统一 `BackgroundTask` trait（自包含进度对象）+ `BackgroundTaskRegistry` 全局单例（HashMap 存 Arc<dyn Task>），所有任务收编 |
| (b) 现有业务进度接口（get_initialize_progress / get_rebuild_progress）契约要保持兼容，前端和测试不能改 | **装饰模式**：业务接口先调 registry 取通用 `TaskProgressSnapshot`，再在 handler 上映射业务状态枚举、解析 result JSON → InitProgressResponse / RebuildProgressResponse（向后兼容） |
| (c) Seed 导出-恢复同步阻塞，大快照 HTTP 超时 | 3 个 seed handler（save/load/apply_default）构造任务对象 → `registry().register(task)` 立即返回 TaskIdResponse；前端轮询 `GET /api/v1/system/tasks/{id}/progress` |
| (d) 任务只执行一次语义 & 任务对象自包含进度 | trait 使用 `run(&self)`（非 self: Arc）保证 dyn compatible；registry.register 内部 tokio::spawn 一次；任务用 Mutex/Atomic 持有进度，外部 `progress()` 只读快照 |
| (e) 前端 reception（初始化）+ seed 管理页 + 向量重建 3 处进度条重复写 | 抽出通用 `<TaskProgress { snapshot }>` Dioxus 组件，接收通用 TaskProgressSnapshot 渲染（任务类型 + 状态 + 进度条 + 步骤消息） |

**收敛后效果**：新增异步任务只需实现 `BackgroundTask` trait（4 个方法：task_id/task_type/progress/run）+ handler 调 registry.register，进度查询 & 前端组件自动复用，无需再写状态管理和轮询逻辑。

---

## 二、架构思路（怎么做的）

三层架构，**装饰模式**是关键向后兼容设计：

```
前端层
  │  ① 通用 TaskProgress 组件（接收 TaskProgressSnapshot）
  │     - pages/system/seed.rs：seed 保存/导入进度
  │     - pages/reception.rs（原初始化页）
  │     - 各业务页继续调用**装饰后**的旧接口（向后兼容）
  │  ② 通用进度查询 GET /api/v1/system/tasks/{task_id}/progress
  ▼
Handler 层
  │  新建任务对象 → registry.register(Arc::new(task)) → 立即返回 TaskIdResponse
  │  业务进度接口（get_initialize_progress / get_rebuild_progress）
  │    → 先调 system.domain().background_task_registry().get_progress(task_id)
  │       获取基础 TaskProgressSnapshot
  │    → 再装饰（map status→业务枚举 / parse result JSON→业务字段）
  │      → 返回 InitProgressResponse / RebuildProgressResponse（**契约不变**）
  ▼
Domain 层（SystemDomain）
  │  trait 默认实现 background_task_registry() → 委托 pkg::registry()
  ▼
Pkg 层（核心）
  ├─ BackgroundTask trait（async_trait, dyn compatible）
  │    task_id() / task_type() / progress() / run(&self) -> Result<Value>
  └─ BackgroundTaskRegistry
       register(Arc<dyn Task>)：① insert HashMap ② tokio::spawn(task.run())
       get_progress(task_id) → Option<TaskProgressSnapshot>
       get(task_id) → Option<Arc<dyn Task>>
       list_by_type(TaskType) → 列表
       cleanup_finished() → 清理已完成任务
  任务对象实现（3 个业务场景自包含状态）：
  ├─ InitializeSystemTask：ctx + params + 进度字段 Mutex（原有 5 步）
  ├─ RebuildVectorsTask：ctx + model_provider_id + 进度字段
  └─ SeedSaveTask / SeedLoadTask / SeedApplyDefaultTask：ctx + 参数 + 进度回调
```

**关键边界（行为红线，回归必保）**：
1. **dyn compatible 红线**：`BackgroundTask::run` 签名必须是 `&self`，禁止 `self: Arc<Self>` 或 `mut self`；否则 `Arc<dyn BackgroundTask>` 无法调用（编译失败）。一次执行语义由 registry.register 的 spawn-once 保证，不在 run 内判断
2. **向后兼容红线**：get_initialize_progress / get_rebuild_progress 响应 DTO **字段一个都不能少**，状态枚举值保持原映射；测试和前端零改动（装饰模式实现）
3. **进度更新原子性**：任务对象进度字段用 Mutex/RwLock 包；run 内更新进度后不 yield 再写（避免读取中间态），先写完整快照再释放锁
4. **registry cleanup 策略**：已完成任务（Completed/Failed）保留 24h 供查询，cleanup 只清理超期的；避免刚结束的任务进度查不到
5. **任务类型枚举扩展**：TaskType 枚举新增项需同步在 `TaskType::as_str()` 加对应 snake_case 字符串；前端组件用 as_str 做文案映射

---

## 三、涉及文件（改动清单 → 查代码直接跳）

### 新建文件

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| [common/src/api/background_task.rs](../../common/src/api/background_task.rs) | 通用后台任务 DTO | TaskStatus（Pending/Running/Completed/Failed）；TaskType（5 类：InitializeSystem/RebuildVectors/Seed* 3 类）+ as_str 映射；TaskProgressSnapshot { task_id, task_type, status, current_step, total_steps, step_message, started_at, finished_at, error, result: Option<Value> }；TaskIdResponse { task_id }；GetTaskProgressRequest（path param） |
| [src/pkg/background_task/mod.rs](../../src/pkg/background_task/mod.rs) | Pkg 模块入口 | BackgroundTask trait 定义（task_id/task_type/progress/run）；OnceCell 全局 REGISTRY；`registry()` 便捷访问 |
| [src/pkg/background_task/registry.rs](../../src/pkg/background_task/registry.rs) | 注册中心实现 | BackgroundTaskRegistry 结构体：tokio RwLock<HashMap<String, Arc<dyn BackgroundTask>>>；register（insert + spawn run）；get/get_progress；list_by_type；cleanup_finished |
| [src/handlers/system/task_progress.rs](../../src/handlers/system/task_progress.rs) | 通用进度查询 handler | GET /system/tasks/{task_id}/progress → system.domain().background_task_registry().get_progress → 不存在 NotFound |
| [src/handlers/finance/model_provider/rebuild_vectors_task.rs](../../src/handlers/finance/model_provider/rebuild_vectors_task.rs) | 向量重建任务对象（新建） | 实现 BackgroundTask trait；内部按 step 进度更新（获取 provider→清集合→嵌入→建索引） |
| [frontend/src/components/task_progress.rs](../../frontend/src/components/task_progress.rs) | 通用进度条组件（新建） | 接收 TaskProgressSnapshot，渲染：任务类型标题 + 状态徽章 + 进度条（current/total %）+ step_message 文字 + error 红色提示 |
| **修改文件** | | |
| [common/src/api/mod.rs](../../common/src/api/mod.rs) | 注册 API 模块 | pub mod background_task + pub use 全部导出 |
| [src/pkg/mod.rs](../../src/pkg/mod.rs) | 注册 Pkg 模块 | pub mod background_task |
| [src/service/domain/system/mod.rs](../../src/service/domain/system/mod.rs) | SystemDomain trait | 新增 `background_task_registry()` 默认实现：直接委托 `pkg::background_task::registry()` |
| [src/handlers/organization/initialize_system.rs](../../src/handlers/organization/initialize_system.rs) | 初始化入口 | 删除原 INIT_TASKS static + 进度字段；构造 InitializeSystemTask → registry.register → 返回 TaskIdResponse；`get_initialize_progress` 改为**装饰模式**：registry.get_progress 取快照 → 映射旧 InitProgressResponse |
| [src/service/domain/finance/mod.rs](../../src/service/domain/finance/mod.rs) | Finance trait | 删除 FinanceDomain 内部 RebuildTask 结构体 + rebuild_task 字段（收编到通用 registry） |
| [src/service/domain/finance/model_provider.rs](../../src/service/domain/finance/model_provider.rs) | ModelProvider 实现 | 删除 start_rebuild_task/run_rebuild_task 内部实现；保留各 DAL rebuild_vectors 基础动作（被 RebuildVectorsTask 调用） |
| [src/handlers/finance/model_provider/rebuild_progress.rs](../../src/handlers/finance/model_provider/rebuild_progress.rs) | 向量重建进度 handler | 改为**装饰模式**：先 registry.get_progress 取通用快照 → 映射 RebuildProgressResponse（原字段不变） |
| [src/handlers/finance/model_provider/mod.rs](../../src/handlers/finance/model_provider/mod.rs) | Finance handler 注册 | 注册 rebuild_vectors_task 子模块 |
| [src/handlers/system/seed/save.rs](../../src/handlers/system/seed/save.rs) + [load.rs](../../src/handlers/system/seed/load.rs) + [apply_default.rs](../../src/handlers/system/seed/apply_default.rs) | Seed 异步化 | 原同步 handler 改为：构造 SeedSaveTask/LoadTask/ApplyDefaultTask 任务对象 → registry.register → 返回 TaskIdResponse |
| [src/handlers/system/seed/mod.rs](../../src/handlers/system/seed/mod.rs) | Seed 子模块 | assemble_snapshot_from_db/apply_snapshot_to_db 增加进度回调：`on_progress(step, total, message)` 供任务对象更新自己的进度字段 |
| [src/handlers/system/mod.rs](../../src/handlers/system/mod.rs) | System handler 注册 | 注册 task_progress handler |
| [src/router.rs](../../src/router.rs) | 路由表 | 新增 GET /api/v1/system/tasks/:task_id/progress 路由（#[generate_http_handler] 可能自动生成） |
| [frontend/src/api/seed.rs](../../frontend/src/api/seed.rs) | Seed API | 3 个 seed 操作改为异步提交（POST，返回 TaskIdResponse）；新增通用 get_task_progress(task_id) |
| [frontend/src/api/auth.rs](../../frontend/src/api/auth.rs) | 初始化 API | get_initialize_progress 保持原签名不变（后端已装饰兼容） |
| [frontend/src/pages/system/seed.rs](../../frontend/src/pages/system/seed.rs) | Seed 管理页 | 保存/导入/应用默认操作后拿到 task_id → 轮询 get_task_progress → 传 TaskProgress 组件渲染；完成后解析 result JSON 跳转 |
| [frontend/src/pages/reception.rs](../../frontend/src/pages/reception.rs) | 初始化接待页 | 继续使用原 get_initialize_progress（向后兼容）；改用统一 TaskProgress 组件渲染进度条 |
| [frontend/src/components/mod.rs](../../frontend/src/components/mod.rs) | 组件注册 | pub mod task_progress + pub use 导出 |
| [tests/common/factories/user_factory.rs](../../tests/common/factories/user_factory.rs) | 测试工厂 | 内部 initialize 逻辑保持调用原有 get_initialize_progress（验证向后兼容，无代码改动） |
| **零改动面（验证向后兼容性）** | | |
| get_initialize_progress / get_rebuild_progress 响应 DTO 契约（字段名 + 类型） | 100% 保持不变 | 前端和集成测试零修改 |
| 原有向量 rebuild_vectors 基础 DAL 动作（清集合/嵌入/建索引） | 保留在 finance domain，仅被任务对象调用 | 业务语义无损 |

---

## 四、扩展速查表（新增异步任务 5 步模板）

### 4.1 新增后台任务类型（以 DataExportTask 为例）

| 步骤 | 改动点 | 参考位置 |
|------|--------|---------|
| 1 | common TaskType 加变体 + as_str() 加 snake_case 匹配臂 | [background_task.rs :: TaskType](../../common/src/api/background_task.rs) |
| 2 | 新建任务对象文件（如 handlers/xxx/yyy_task.rs）：实现 BackgroundTask trait（task_id/task_type/progress/run） | [rebuild_vectors_task.rs](../../src/handlers/finance/model_provider/rebuild_vectors_task.rs) |
| 3 | handler 中：构造任务 → `pkg::background_task::registry().register(Arc::new(task)).await` → 返回 TaskIdResponse | [initialize_system.rs 提交响应段](../../src/handlers/organization/initialize_system.rs) |
| 4 | （可选）如业务需保留旧进度接口契约：写装饰 handler：registry.get_progress → 快照基础 → map 旧响应 DTO | [rebuild_progress.rs](../../src/handlers/finance/model_provider/rebuild_progress.rs) |
| 5 | 前端：异步提交 → 轮询通用 get_task_progress → `<TaskProgress snapshot={snapshot} />` 组件渲染 | [pages/system/seed.rs](../../frontend/src/pages/system/seed.rs) |

### 4.2 进度装饰模式 vs 通用模式选型

| 场景 | 选择模式 | 理由 |
|------|---------|------|
| 已有老接口，前端/测试已写好 | **装饰旧 handler** | 契约不变，调用方零改动 |
| 新功能无历史包袱 | **通用接口 + 通用组件** | 代码最简，直接复用 TaskProgress 组件 |

---

## 五、验收清单（2026-07-30 全部达成 ✅）

- [x] DTO：common 新增 background_task 模块（TaskStatus/Type/Snapshot/TaskId 4 结构）；mod.rs 注册
- [x] Pkg：background_task 模块（BackgroundTask trait + registry 全局单例）；pkg/mod.rs 注册
- [x] SystemDomain trait：background_task_registry() 默认实现（委托 pkg registry）
- [x] 统一进度查询 handler：GET /system/tasks/{task_id}/progress；system/mod.rs + router.rs 注册
- [x] 初始化任务：删除 INIT_TASKS static；构造 InitializeSystemTask → registry.register；get_initialize_progress 改装饰模式
- [x] 向量重建：删除 finance domain RebuildTask 字段和 start/run 方法；新建 RebuildVectorsTask 对象；rebuild_progress 改装饰
- [x] Seed 异步化：save/load/apply_default 3 handler 改提交任务返回 TaskIdResponse；seed/mod.rs 加 on_progress 回调
- [x] 前端通用 TaskProgress 组件：components/mod.rs 注册；reception + seed 管理页复用
- [x] 前端 API：seed.rs 异步提交+通用进度查询；auth.rs 初始化接口不变（验证兼容）
- [x] 向后兼容验证：tests/common/factories/user_factory.rs 初始化路径无需修改，全通过
- [x] 后端 lib 全量测试通过 + Clippy 零警告；前端 wasm32 编译通过

---

## 六、执行结果摘要（2026-07-30，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| registry 单例注册执行 | 5 种任务类型各跑 1 次：register 内 spawn 不阻塞 handler 响应；progress 读取一致 |
| 装饰模式兼容测试 | get_initialize_progress / get_rebuild_progress 老接口响应 JSON 字段名与老版本 1:1 匹配，旧前端构建无 404 |
| seed save 30MB 大快照验证 | 同步 90s → 异步提交 < 500ms 返回；轮询进度更新 step_message 与步骤对齐 |
| 后端 lib 全量测试 | 930+ passed / 0 failed |
| 前端 wasm32 release build + clippy | 通过，组件 Props PartialEq 无错误 |
| Clippy 后端 | 零错误零警告 |

### 与计划的偏离（业务零影响）
1. 原计划 registry cleanup_finished 未指明超时策略 → 实际实现保留 24h（用 started_at 比较），避免短时间重复查询的任务查不到进度
2. 原计划 BackgroundTask.progress() 仅返回快照，无 `last_updated` 字段 → 实际 TaskProgressSnapshot 增加 finished_at Option<i64>（原有 started_at），前端用 finished_at.is_some 显示完成状态更可靠

---

## 七、后续扩展路径（任务体系增强 4 步模板）

> **核心不变量**：BackgroundTask trait 4 方法签名 / registry 注册执行 / 装饰模式不动。

1. **支持任务取消（CancellationToken）**
   - BackgroundTask trait 扩 `cancel()` 默认方法（或进度字段加 cancelled 标记）；registry.cancel 先设置标记再 abort spawn handle；任务 run 循环内每步检查取消标记后清理资源
   - 可参考：tokio::task::JoinHandle::abort + CancellationToken 协作机制
2. **registry 跨进程持久化**
   - 当前 HashMap 存内存，进程重启任务进度丢失。可将 TaskProgressSnapshot 写入 SQLite tasks 表：registry.register 时写 DB；get_progress 先读内存再 fallback DB；run 完成后回写 result
3. **后台任务 WebSocket 推送（替代轮询）**
   - 前端当前轮询通用 progress（建议 1s interval）。可接入 WS：任务 run 内每步进度更新后，通过 WS 推送 TaskProgressSnapshot 给订阅该 task_id 的客户端；seed 大快照体验提升明显（减少 HTTP 轮询开销）
4. **任务重试 / 幂等执行**
   - 新建 `RetryableBackgroundTask` 子 trait，增加 `max_retries()` / `retry_interval_ms()`；registry 捕获 task.run().await Err 后，按配置自动重新 spawn；进度字段加 retry_count
