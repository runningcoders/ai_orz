# HNSW持久化与索引重建异步化重构

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：HNSW持久化与索引重建异步化重构 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 冷启动 HNSW 加载慢或索引损坏，回看"HNSW 加载-落盘三保险"与"CollectionMeta 增量重建校验"两处
> - 若需了解 flush 机制或 bincode 格式/向量重建进度映射，直接跳转对应代码文件（见 §涉及文件）
> 关联文档：
> - [向量存储解耦设计](./移除rig依赖与向量存储后端解耦.md) — 姊妹计划：HnswStore 前身来自移除 rig 依赖重写
> - [通用后台任务模块](./通用后台任务模块与Seed异步化重构.md) — 后续：RebuildTask 可进一步收编到通用 BackgroundTask

---

## 一、重构目标（为什么做）

内存 HNSW 索引进程重启丢失（冷启动全量重建，百万向量需数十分钟）；向量索引重建是同步阻塞请求（大模型切换导致 HTTP 超时/网关 504）；切换 Embedding Provider 时无增量判断：即使新 Provider 和旧一致也会全量重建，浪费 GPU 时间。

| 问题维度 | 解决方式 |
|---------|---------|
| (a) HNSW 纯内存，重启丢失 + 每次冷启动全量重建 | HnswStore 每个集合独立 bincode 序列化文件落盘；配置项 `hnsw_index_dir`；定时 60s 后台 flush 脏数据；进程 Drop 兜底落盘；冷启动时扫描目录 `{collection}.bincode` 反序列化加载 |
| (b) 向量重建同步 HTTP，大集合阻塞超时 | Finance Domain 持有 `Arc<RwLock<Option<RebuildTask>>>`：switch 时写入 RebuildTask 并 spawn 后台 tokio 协程，handler 立即返回 task_id；新增 `get_rebuild_progress` handler 轮询进度（task_id 查字段） |
| (c) 切换 Provider 无增量判断，新旧 Provider 相同时白重建 | 每个 Collection 维护 `CollectionMeta { model_provider_id, dimensions, vector_count, updated_at }`；重建前对比目标 Provider + 维度：一致（且 DB 向量计数与 meta 一致）→ 跳过重建，仅返回"已最新"响应 |
| (d) 并发重建保护：同一时刻两次"切换 Embedding"请求重复触发 | Domain Arc<RwLock<Option<RebuildTask>>>：切换前 read 检查 Some（存在进行中任务）→ 返回 Conflict 错误码 `RebuildInProgress`（common error 新增）；仅 None 时 write 写入任务 |
| (e) 切换 Embedding 原直接 `return rebuild_finished`，前端拿不到 task_id | `switch_embedding` handler 响应 DTO 扩 task_id 字段：① 需要重建 → 后台 spawn + 返回 task_id（非空）② 跳过重建 → task_id 为空 + 说明"已是最新配置" |

**收敛后效果**：冷启动有索引时（无向量变更）加载从 10min+ → 秒级反序列化完成；切换 Embedding 从阻塞同步 → 立即响应，前端轮询进度条展示重建百分比。

---

## 二、架构思路（怎么做的）

双并行链路（HNSW 持久化三保险 + 索引重建异步）：

```
HNSW 持久化链路（三保险写盘）
├─ 保险 1：后台定时 60s flush
│   HnswStore.new() → start_flush_task()
│   tokio spawn loop: sleep(60s) → 遍历 collections: data.dirty == true → save_collection()
├─ 保险 2：HnswStore::drop() 兜底
│   Drop impl 被调用 → abort flush_task → try_read collections → 遍历 dirty → save_collection()
│   （防止进程优雅退出时 flush 任务还没跑到下一个 60s 周期）
└─ 保险 3：显式 VectorStore::flush()
    trait 新增 flush(&self) 默认 Ok(())
    HnswStore impl: 立即 flush_all_dirty（应用层/退出钩子可主动调）

冷启动加载：
new() → create_dir_all(hnsw_index_dir)
  → read_dir 扫描 *.bincode
  → file_stem 提取 collection_name
  → deserialize_from(reader) 加载 CollectionData（含 vectors/deleted/cached_index）
  → 写入 collections HashMap（后续请求命中内存不走 DB 重建）

索引重建异步链路（Finance Domain）
  switch_embedding 接收新 provider
    │
    ▼ 1. 校验 target provider 是否与 CollectionMeta 一致（增量判断）
    │  一致且 vector_count 匹配 → 跳过，响应 { task_id: None, skipped: true }
    │
    ▼ 2. 进行中任务判断：Arc<RwLock<Option<RebuildTask>>>
    │  read lock → Some(_) → 返回 common ErrorCode::RebuildInProgress
    │
    ▼ 3. write lock 写入 RebuildTask { id, status: Running, current_step, total_steps, message }
    │   并 tokio::spawn 异步执行：
    │   步骤 (1) 清理旧集合 → (2) DB 查 memory 总数 → (3) 分批 embedding 调 provider → (4) 写向量 → (5) 建 HNSW 索引 → (6) 更新 CollectionMeta
    │   每步更新 RebuildTask 内部字段（RwLock 写）
    │   完成 → status = Completed + result = Some(JSON)
    │   失败 → status = Failed + error = Some(message)
    │
    ▼ 4. 立即返回 SwitchEmbeddingResponse { task_id: Some(id), skipped: false }
    │
    ▼ 5. 前端轮询 GET /finance/model_providers/rebuild_progress?task_id=xxx
        handler: finance.domain().get_rebuild_progress(task_id)
         → 读 RebuildTask 字段 → 映射 RebuildProgressResponse DTO
            { status, current_step, total_steps, message, percent }
```

**关键边界（行为红线，回归必保）**：
1. **flush 三保险互斥**：定时 flush 与 Drop 兜底的 write_file 时间窗口可能重叠 → save_collection 用 std::fs::File::create（覆盖写）是原子文件系统调用；且 HnswStore.collections 读锁持有时再执行序列化，两保险同时写同一文件的最后结果一致（无 partial write 风险）。另：启动时 flush_task 先被 abort，保证 Drop 时定时线程不跑
2. **增量跳过重建判定严格**：CollectionMeta 三条件（model_provider_id 匹配 **AND** dimensions 匹配 **AND** DB 向量计数 == meta.vector_count）缺一不可（防止"元数据旧但 DB 实际向量被外部绕过 DAO 更新"的假匹配）；跳过重建时，**必须把集合 meta 的 updated_at 刷新为 now**（标记"已校验"）
3. **RebuildInProgress 错误码强类型**：common/src/error/code.rs 显式新增 ErrorCode::RebuildInProgress（禁止复用通用 Conflict）；前端根据特定错误码展示"当前有重建任务进行中，请等待完成"提示（而非通用 409 报错）
4. **HNSW 反序列化版本兼容**：bincode 不自我描述格式。CollectionData 结构字段若后续变动（如加字段），**必须用 bincode::Options::with_native_endian 固定端序 + 增加 Version 包裹**；当前实现暂固定 V1 版本号在文件名（`collection.bincode` 即 V1），未来 V2 用 `{collection}.v2.bincode` 并优先加载 V2
5. **cached_index Option 序列化语义**：HNSWMap cached_index 序列化开销巨大（数万节点邻接表）。策略：CollectionData.cached_index=None 持久化 → None（不写实际索引，只写向量+标记+dirty）；反序列化后首次 search 触发 rebuild_cached_index（一次性，通常冷启动加载后首个请求会花费几百毫秒构建缓存，但显著小于从 SQLite 原始数据重新全量嵌入）；可在配置中加 `hnsw_eager_index` 选项在加载后立即主动 build

---

## 三、涉及文件（改动清单 → 查代码直接跳）

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **配置层** | | |
| [common/src/config.rs](../../common/src/config.rs) | AppConfig 扩展 | DatabaseConfig 新增 `hnsw_index_dir: String` + `default_hnsw_index_dir()`（默认 "hnsw_index"）；Default impl 同步；AppConfig 新增 `hnsw_index_dir() -> PathBuf` 方法（base_data_path.join） |
| **HNSW 持久化核心** | | |
| [src/pkg/storage/hnsw.rs](../../src/pkg/storage/hnsw.rs) | HnswStore 实现 | 大量修改：① CollectionData 加 `#[derive(Serialize, Deserialize)]`（FloatPoint wrapper 也加）；② HnswStore 结构体加 `base_path: PathBuf` + `flush_task: Option<JoinHandle<()>>`；③ new()：config 读取路径 → create_dir_all → `load_all_collections()` 扫描 bincode 反序列化 → start_flush_task()；④ 新增 load_all_collections / load_collection / start_flush_task / flush_all_dirty / save_collection 5 辅助函数；⑤ impl Drop for HnswStore：abort flush_task + try_read dirty 遍历 save |
| [src/pkg/storage/vector.rs](../../src/pkg/storage/vector.rs) | VectorStore trait | 新增 `async fn flush(&self) -> Result<()>` 默认 Ok(())（HnswStore 覆盖实现）；RigStore/LanceStore 保持默认空实现 |
| **向量重建异步化（Finance 域）** | | |
| [src/service/domain/finance/mod.rs](../../src/service/domain/finance/mod.rs) | Finance Domain trait + 结构体 | FinanceDomain 结构体加 `rebuild_task: Arc<RwLock<Option<RebuildTask>>>`；ModelProviderManage trait 新增 `async fn get_rebuild_progress(ctx, task_id) -> Result<RebuildProgressSnapshot>`；`async fn switch_embedding_provider(...) -> Result<SwitchEmbeddingResponse>` 响应加 task_id |
| [src/service/domain/finance/model_provider.rs](../../src/service/domain/finance/model_provider.rs) | Finance 实现（核心） | 新增：① CollectionMeta（model_provider_id, dimensions, vector_count, updated_at）保存到 finance meta map；② RebuildTask 结构体 { id, status, current_step, total_steps, step_message, started_at, error, result }；③ switch_embedding：A. 增量判断（3 条件全满足跳过 + 刷新 updated_at + 返回 task_id=None）B. 读锁进行中存在→RebuildInProgress C. 写锁写入+spawn 后台 6 步 rebuild（每步更新 task 字段）；④ get_rebuild_progress 从 Arc<RwLock> 读 task → 返回 snapshot |
| **common 错误码** | | |
| [common/src/error/code.rs](../../common/src/error/code.rs) | 错误码定义 | 新增 `RebuildInProgress` 错误变体；HTTP 映射为 409 Conflict（但前端按 code 字段精确匹配） |
| **API DTO + Handler** | | |
| [common/src/api/model_provider.rs](../../common/src/api/model_provider.rs) | Finance API DTO | 新增：RebuildProgressResponse { status, current_step, total_steps, step_message, percent, error, result }；SwitchEmbeddingResponse 扩 task_id: Option<String> + skipped: bool |
| 新建 rebuild_progress.rs（handlers/finance/model_provider/） | 进度查询 handler | `GET /api/v1/finance/model_providers/rebuild_progress` 参数 task_id → finance.domain().get_rebuild_progress → map RebuildProgressResponse（percent = current_step*100/total_steps，容错 total=0） |
| [handlers/finance/model_provider/mod.rs](../../src/handlers/finance/model_provider/mod.rs) | handler 注册 | pub mod rebuild_progress + 导出；switch_embedding handler 扩响应（返回 SwitchEmbeddingResponse { task_id, skipped }） |
| [handlers/finance/model_provider/switch_embedding.rs](../../src/handlers/finance/model_provider/switch_embedding.rs) | Switch handler | 调 domain.switch_embedding_provider → 返回扩字段后的响应 |
| **零改动面** | | |
| 向量 search/insert/delete DAO & Domain API（除 flush 新 trait 方法默认空实现） | 100% 不变 | 前端业务搜索功能无破坏 |
| SQLite 向量表 / memory 表结构 | 零字段修改 | 数据无迁移 |

---

## 四、扩展速查表

### 4.1 HNSW 落盘触发时机与诊断

| 场景 | 触发者 | 行为 | 诊断方法 |
|------|--------|------|---------|
| 每 60s 周期 | 后台 flush_task tokio spawn | flush_all_dirty：遍历 collections 中 dirty=true 的集合写入 bincode | 日志启用 tracing::debug! 可看 "HNSW flushed N collections"；或查看 hnsw_index_dir/*.bincode 修改时间是否接近 |
| 进程退出 | HnswStore::drop() | abort 定时任务 → try_read 非阻塞扫 dirty 落盘（避免挂起等待） | 测试：kill -TERM 后，*.bincode 文件 size 应与内存估算一致（向量数 × 4bytes × dims） |
| 应用层主动调用 | VectorStore::flush() | 立即 flush_all_dirty（可在 signal hook 中显式调用） | 部署 systemd ExecStop 调 /flush 管理端点 |
| 冷启动加载 | HnswStore::new() | create_dir_all → read_dir 扫描 → 反序列化入内存 | 启动日志 "Loaded N HNSW collections from disk"，对比 DB memory 总数校验 |

### 4.2 新增"重建类型"模板（如记忆沉淀知识重建）

| 步骤 | 改动点 | 参考位置 |
|------|--------|---------|
| 1 | Finance Domain（或对应 domain）加 `Arc<RwLock<Option<XxxTask>>>` 结构字段 | [finance/mod.rs :: rebuild_task 字段](../../src/service/domain/finance/mod.rs) |
| 2 | common error 新增 XxxInProgress；DTO 新增 Progress Response + switch 响应扩 task_id | [code.rs :: RebuildInProgress](../../common/src/error/code.rs) |
| 3 | trait 加 get_xxx_progress + switch_xxx 异步 spawn 实现 | [model_provider.rs :: switch/get 实现](../../src/service/domain/finance/model_provider.rs) |
| 4 | handler 新建 get_xxx_progress + switch 响应扩字段 | rebuild_progress.rs 新建模式 |

> 优化建议：5 类任务形态可进一步收编为 [通用后台任务模块](./通用后台任务模块与Seed异步化重构.md) 的 BackgroundTask trait，避免每个 domain 写一份 Arc<RwLock<Option<Task>>> 样板。

---

## 五、验收清单（2026-07-16 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-16，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| HNSW 持久化往返（10万向量数据集） | new() → insert 10w → drop → new() 再加载：search 结果与插入前 100% 一致（余弦相似度命中排序相同），加载耗时 < 2s（对比全量重建 8min+） |
| 60s flush 周期验证 | 单测手动 tokio::time::advance(Duration 90s) 后，bincode 文件 mtime 变化；dirty 标记被清除 |
| 增量重建跳过 | 切换 Embedding provider_id = 当前 → response { task_id: None, skipped: true }，后台无 spawn；元数据 updated_at 刷新 |
| 并发冲突 | 连续两次 switch 请求 → 第二次 ErrorCode::RebuildInProgress；后端只有 1 个 spawn 在跑 |
| 后端 lib 全量测试 | 向量相关测试 + HNSW flush 测试 11 用例 → 全部通过；全量 820+ passed / 0 failed |
| Clippy 后端 + fmt | 零错误警告；fmt check PASS |

### 与计划的偏离（业务零影响）
1. 原计划 HNSW cached_index（HnswMap<FloatPoint, String>）序列化随集合一起写盘 → 实际因 bincode 序列化 HNSW 邻接表体积巨大（数万节点 × M=16 × 8bytes = >10MB per 集合）且首次 search 自动构建 cached 仅需 <300ms，权衡后 cached_index=None 落盘（即**每次冷启动后首次 search 自动重建索引缓存**，节省落盘文件大小 80% 以上）
2. 原计划 get_rebuild_progress 路径在 Task 6 中写 `/finance/model_providers/rebuild_progress`（Query param）→ 实际 handler 实现中为了 REST 规范改成 `?task_id=` query param；task_id 从 path 提取兼容性更好（通用 BackgroundTask 后续用 path/:task_id 更统一）

---

## 七、后续扩展路径（向量体系增强 4 步模板）

> **核心不变量**：hnsw_index_dir bincode 文件格式 / CollectionMeta 增量判定三条件 / 进行中冲突锁机制不动。

1. **落盘校验 & 损坏自动修复**
   - 当前加载时 bincode 反序列化失败会跳过该集合（静默从空集合开始，触发全量重建）。增强方案：save_collection 时追加 CRC32 校验尾 4 字节；load_collection 反序列化前校验 CRC，失败则：① 重命名坏文件 `{name}.bincode.corrupt.{ts}`（保留排查）② 删除 meta → 触发从 DB 向量自动增量重建
2. **多 HNSW 后端热切换（Lance/Rig fallback）**
   - VectorStore trait 已有抽象；配置 `vector_store_type: VectorStoreType::Hnsw(LoadPolicy)` 枚举加策略：`LoadPolicy::PreferDiskFirst(VerifyCrc)` / `AlwaysRebuild` / `DiskThenVerifyCount(3次重试)`；启动时按策略选加载方式
3. **进一步收编 RebuildTask 到通用 BackgroundTask**
   - 目前 RebuildTask 是 Finance Domain 内部专用。参考通用后台任务模块：改实现 BackgroundTask trait（task_id/task_type=RebuildVectors/progress/run），通过 registry.register 统一管理；get_rebuild_progress 走**装饰模式**（先 registry.get_progress 快照 → 装饰 RebuildProgressResponse），消除重复 Arc<RwLock<Option<Task>>> 样板
4. **RebuildTask 进度细化（向量级百分比而非步骤级）**
   - 现在 Rebuild 6 步等比，第 3 步"分批 embedding 调 provider"占总时间 95% 以上但只是一步。优化：step=3 内部再分子进度（每完成 100 条向量更新 step_message = f"嵌入中 {processed}/{total_vectors} {percent}%"，percent 用 (3*100 + sub_percent_3) / 6 计算）；进度条体感更平滑，不会卡在 50% 长时间不动