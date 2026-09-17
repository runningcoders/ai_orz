---
kind: wiki_knowledge_card
name: 底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（InMemory/Hnsw 默认 → LanceDB 大规模）
category: 基础设施 / 存储架构决策
scope:
  - "migrations/**/*.sql"
  - "src/pkg/storage/**/*.rs"
  - "src/pkg/stats/**/*.rs"
  - "src/service/dao/**/sqlite.rs"
  - "src/service/dal/**/*.rs"
source_files:
  - src/pkg/storage/mod.rs#L33-L95（Storage::new 中 SQLite SqlitePool + 4 种向量后端按 VectorStoreType 配置选择 + DuckDB Stats 初始化）
  - src/pkg/storage/vector.rs#L20-L88（VectorStore trait：8 个 async 方法，统一向量抽象，支持可插拔切换）
  - src/pkg/storage/lance.rs#L30-L190（LanceVectorStore 结构 + LanceDB Connection + 表名 sanitize + 维度自愈）
  - src/pkg/storage/fts5.rs（FTS5 关键词搜索工具：escape_fts5_keyword + FTS5 规范）
  - src/pkg/stats/mod.rs（Stats 模块：DuckDB 五维统计表打开 + 建表 + 周期落盘）
  - src/pkg/stats/collector.rs（RuntimeStatsCollector：内存滑动窗口 + 多维度聚合）
  - src/service/dal/task.rs（TaskDal.search：三位一体混合搜索模式）
  - src/service/dao/memory/vector.rs（Memory DAO 向量索引调用链）
  - docs/wiki/zh/content/基础设施/存储系统/存储系统.md
  - docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md
  - docs/wiki/knowledge/zh/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离.md
  - docs/wiki/knowledge/zh/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md
  - docs/wiki/knowledge/zh/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md
  - docs/wiki/knowledge/zh/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md
---

## §1 概述与定位

AI Orz 存储层采用 **嵌入式多引擎组合** 策略，而非单一大数据库覆盖所有需求：SQLite 承载全部业务数据（ACID 事务 + FTS5 关键词搜索）、DuckDB 承载多维统计分析（OLAP 列式聚合）、向量存储通过 `VectorStore` trait 可插拔切换（默认 InMemory/Hnsw 零依赖，大规模升级到 LanceDB）。触发读取场景：新增 DAO 选引擎、评估存储性能瓶颈、判断是否需要从 InMemory/Hnsw 升级到 LanceDB、讨论"为什么我们不选 PostgreSQL/ClickHouse/单一大向量库"。

## §2 关键文件表

| 文件 | 职责 | 关联 Wiki 长文 |
|------|------|---------------|
| `src/pkg/storage/mod.rs` | Storage 统一门面：SqlitePool + VectorStore + DuckDB Stats 三引擎装配 | docs/wiki/zh/content/基础设施/存储系统/存储系统.md |
| `src/pkg/storage/vector.rs` | VectorStore trait 定义 + SqliteVssStore 实现 | （见下方关联 RAG 卡） |
| `src/pkg/storage/lance.rs` | LanceVectorStore：LanceDB 磁盘 ANN + 维度自愈 + payload 平铺列 |  |
| `src/pkg/storage/hnsw.rs` | HnswStore：纯 Rust HNSW 高性能近似最近邻索引 |  |
| `src/pkg/storage/mem_vector.rs` | InMemoryVectorStore：纯 Rust 内存实现，零系统依赖（开发/测试默认） |  |
| `src/pkg/storage/fts5.rs` | FTS5 关键词搜索：escape_fts5_keyword + BM25 排序 |  |
| `src/pkg/stats/mod.rs` | DuckDB Stats：五维统计表 + 周期落盘 + RuntimeStatsCollector 内存滑动窗口 | docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md |
| `migrations/*.sql` | SQLite STRICT 迁移文件：业务表定义 |  |

### 关联 RAG 知识卡（实现层，本卡为架构决策层）

- SQLite 工程规范 → docs/wiki/knowledge/zh/SQLite + SQLx 0.8 存储工程规范.../（编码规范）
- DuckDB 统计实现 → docs/wiki/knowledge/zh/DuckDB 多维统计双层互补.../（实现细节）
- 三位一体混合搜索 → docs/wiki/knowledge/zh/三位一体混合搜索.../（搜索流程）
- VectorStore trait → docs/wiki/knowledge/zh/向量存储抽象 VectorStore.../（向量 API）

## §3 三引擎选型依据

### SQLite OLTP（业务库）

**选 SQLite 的核心理由**：AI Orz 的业务特征是"单进程嵌入式 + 高频小事务 + 点查为王"，完全匹配 SQLite 的设计目标（"单条语句尽快完成"）。具体支撑点：

1. **ACID 成熟度**：20+ 年生产验证，WAL 模式支持单写者 + 无限并发读，事务极轻
2. **STRICT 表模式 + sqlx 0.8 类型安全**：枚举用 `i32` 映射，`RETURNING` 支持，离线 `.sqlx` 构建保证编译期类型推断
3. **FTS5 内置**：关键词搜索零外部依赖，BM25 排序亚毫秒级响应，与业务表同库 JOIN
4. **运维零成本**：单文件存储，备份就是拷文件，嵌入式部署（前端 WASM 也能跑）

**不用 PostgreSQL 的原因**：项目定位是嵌入式 + 单进程应用，PostgreSQL 需要独立 server 进程、需要运维、多连接管理增加复杂度，且单文件分发场景（Dioxus 前端 WASM + 桌面端）无法嵌入。

### DuckDB OLAP（统计分析）

**选 DuckDB 的核心理由**：统计需求是"全表聚合 + 多维度 GROUP BY + 窗口函数"，完全匹配 DuckDB 的设计目标（"整批数据尽快算完"）。

1. **列式存储 + 向量化执行**：1000 万行 GROUP BY 聚合，DuckDB 比 SQLite 快 10~200 倍
2. **零运维嵌入**：与 SQLite 同级别轻量，直接打开/创建统计文件，无需 server
3. **与 SQLite 并存无冲突**：DuckDB MVCC + 列式，SQLite ACID + 行式，双文件各自闭环，互不干扰
4. **record_event! 宏自动表推断**：AOP 事件中心投递 → StatsCollector 消费 → DuckDB 自动按维度表落盘

**不用 ClickHouse 的原因**：ClickHouse 是独立 server 集群，不适合嵌入式场景。不用让 SQLite 兼做统计的原因：SQLite 行式存储在聚合时要读整行数据，IO 浪费严重。

### 向量可插拔（4 后端）

**设计哲学**：向量数据量波动最大（测试场景零依赖、开发场景几万条、生产长期记忆可能涨到几十万+），所以**不锁定单个后端**，用 `VectorStore` trait 统一抽象，按 `VectorStoreType` 配置切换。

| 后端 | 适用场景 | 优势 | 限制 |
|------|---------|------|------|
| **InMemoryVectorStore** | 开发/测试默认，≤1 万向量 | 零系统依赖、纯 Rust、单进程最快 | 内存内，重启丢失 |
| **HnswStore** | 小规模生产（1~10 万向量） | 纯 Rust ANN 索引、高性能、零系统依赖 | 内存约束，超 RAM 则退化 |
| **SqliteVssStore** | 向量想和业务数据同文件时 | 与 SQLite 共享连接池、SQL JOIN 方便 | 需要系统级 VSS 扩展依赖 |
| **LanceVectorStore** | 大规模生产（>10 万向量 + 频繁增删） | 磁盘 ANN（IVF-PQ/HNSW）、可超 RAM、MVCC 版本化 | 额外 LanceDB 依赖 |

**默认路径（零依赖上线）**：`InMemoryVectorStore` → 向量规模涨 → 自动/手动切换 `HnswStore`
**大规模升级路径（生产级）**：配置 `VectorStoreType::LanceDb` → `LanceVectorStore` 接管磁盘持久化

### FTS5 + 向量 三位一体混合搜索

搜索不做"向量 or 关键词"二选一，而是 **6 DAO 统一 3 步模式**：向量语义搜索 → FTS5 关键词搜索 → 分数归一化加权合并 + 向量失败降级。

- FTS5 优势：**精确场景**（错误码、API 名、技术术语）向量搜不准，BM25 补精度
- 向量优势：**模糊场景**（同义、释义、跨语言）FTS5 搜不到，向量补召回
- 两者同库可 JOIN：SQLite FTS5 表和向量元数据表同文件，合并排序一条 SQL 搞定

## §4 硬约束（禁止违反的架构红线）

| # | 约束 | 理由 |
|---|------|------|
| 1 | **业务数据一律写 SQLite，禁止绕 SqlitePool 直接写文件** | 事务一致性 + migration 机制 + sqlx 类型安全，绕开会导致数据损坏或类型推断失败 |
| 2 | **统计数据一律写 DuckDB Stats 模块，禁止让 SQLite DAO 承担聚合统计** | SQLite 行式存储聚合性能差 10~200 倍，且 DuckDB 有独立文件不会锁死业务连接 |
| 3 | **向量后端切换必须通过 `VectorStore` trait，禁止 DAO 层感知具体实现** | 可插拔切换是核心设计，DAO 只调 trait 方法，具体后端由 Storage::new 按配置装配 |
| 4 | **禁止让 DuckDB 承载业务事务，禁止让 SQLite 承载大规模向量** | 引擎职责严格分离：SQLite=OLTP+关键词，DuckDB=OLAP，向量=可插拔专用引擎 |
| 5 | **LanceDB 切换触发条件**：向量规模 >10 万 **且** 存在频繁增删（记忆沉淀 / re-embedding）；规模 <10 万优先 HnswStore（零依赖） | LanceDB 磁盘 ANN 解决超 RAM 问题，但引入额外依赖；Hnsw 在内存内更快 |
| 6 | **FTS5 关键词与向量搜索必须并存**：禁止删除 FTS5 只留向量（精度场景丢失），禁止删除向量只留 FTS5（召回场景丢失） | 三位一体混合搜索是已验证的最优方案，单侧降级会导致搜索质量显著下降 |
| 7 | **`VectorStoreType::InMemory` 必须作为开发/测试默认**：任何新环境首次启动、CI 集成测试，必须 InMemory 后端（零系统依赖，可快速起停） | 开发体验 + CI 稳定性，引入 LanceDB/Hnsw 只在生产配置中指定 |
| 8 | **LanceDB 表名必须 sanitize**：collection 名含冒号（如 `memory:short_term`）需过滤为合法表名（`memory_short_term`） | LanceDB 0.26 表名只接受字母数字/下划线/连字符/点，非法名会 unwrap panic |
| 9 | **LanceDB 只读/删除操作路径禁止隐式建表**：`get` / `delete` / `clear_collection` / `update_payload` / `search` 对不存在的 collection 必须 no-op，禁止以 `dimensions=0` 建残废表 | 残废表会导致后续正常维度 upsert 被 LanceDB "Append with different schema" 拒绝（已在实现中用 `open_existing_table` 防住） |
| 10 | **DuckDB Stats 全局单例必须由 `Storage::new` 内部初始化**：禁止在业务代码中手动打开 DuckDB 文件或手动调用 `init_global_stats` | 三引擎装配统一在 Storage 门面内完成，避免重复初始化或连接泄漏 |
