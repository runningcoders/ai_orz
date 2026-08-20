---
kind: wiki_knowledge_card
name: Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵
category: Finance 模型提供商 / 向量索引重建
scope:
  - "src/service/domain/finance/model_provider.rs"
  - "src/handlers/finance/model_provider/create_model_provider.rs"
  - "src/handlers/finance/model_provider/update_model_provider.rs"
  - "src/handlers/finance/model_provider/switch_embedding.rs"
  - "src/handlers/finance/model_provider/rebuild_vectors_task.rs"
  - "src/handlers/organization/initialize_system.rs"
  - "src/models/model_provider.rs"
  - "common/src/enums/agent.rs"
  - "common/src/api/model_provider.rs"
  - "common/src/api/organization.rs"
  - "frontend/src/pages/finance/model_providers.rs"
  - "frontend/src/pages/reception.rs"
  - "frontend/src/api/mod.rs"
source_files:

  - common/src/enums/agent.rs#L118-L135 (ModelProviderStatus 枚举：Deleted=0 软删除 / Normal=1 启用 / Disabled=2 未启用)

  - common/src/api/model_provider.rs#L9-L80 (CreateModelProviderResponse 含 status/rebuild_task_id；UpdateModelProviderResponse 含 rebuild_task_id)
  - 'common/src/api/organization.rs#L20-L65 (InitializeSystemRequest.chat_model: Option + serde(default)；chat_provider_id: Option<String>)'

  - src/models/model_provider.rs#L70-L197 (ModelProvider Po + Entity；vector_collection = "embeddings"；status 默认 Normal)

  - src/service/domain/finance/model_provider.rs#L1-L200 (create_model_provider Embedding 降级逻辑；update_model_provider 409 switch_required；switch_embedding 软删旧+启用新；max_context_length/recommended_context_length 上下文长度配置)

  - src/handlers/finance/model_provider/create_model_provider.rs#L1-L90 (回读落库状态；首个 Normal Embedding 创建注册 RebuildVectorsTask；响应含 status + rebuild_task_id + 上下文长度；Config 含 max_context_length/recommended_context_length)

  - src/handlers/finance/model_provider/update_model_provider.rs#L1-L145 (was_enabled_embedding 用更新前值判断；embedding_config_changed 仅 Normal Embedding 配置变化触发重建；Disabled 编辑不重建；上下文长度 partial update 三态逻辑)

  - src/handlers/finance/model_provider/switch_embedding.rs#L1-L55 (切换：软删旧 provider + 启用新 + 全量重建)

  - src/handlers/finance/model_provider/rebuild_vectors_task.rs#L1-L200 (RebuildVectorsTask：7 类实体顺序重建向量索引)

  - src/handlers/organization/initialize_system.rs#L1-L300 (初始化动态步骤数：3 + chat + embedding；条件化创建 provider；边界校验)

  - frontend/src/pages/finance/model_providers.rs#L1-L600 (创建 Modal capability 选择；禁用 status=2；删除 Embedding 警示文案；上下文长度字段表单)

  - frontend/src/pages/reception.rs#L1-L700 (初始化 2 步表单；对话/向量模型可选 + 跳过后果提示；上下文长度输入)

  - frontend/src/api/mod.rs (API 客户端统一入口；model_provider 模块请求转发)

  - tests/integration/model_provider_embedding_test.rs (Embedding 生命周期 6+ 用例：创建/禁用/切换/重建/上下文长度)

  - docs/plan/系统初始化模型配置策略调整.md

  - docs/wiki/zh/content/功能模块/模型提供商管理.md

  - docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/模型提供商管理.md

  - docs/wiki/zh/content/API 参考/RESTful API/财务管理模块 API/模型提供商管理 API.md

  - docs/wiki/zh/content/功能模块/用户与组织管理/系统初始化.md

  - 【平行卡】docs/wiki/knowledge/zh/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md

  - 【测试覆盖引用】docs/wiki/knowledge/zh/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E.md---

# Embedding Provider 生命周期与重建触发条件

## §1 整体方案

Embedding Provider 生命周期采用「**创建不阻塞 + 启用时切换**」策略，配合向量重建任务的条件触发，实现灵活的多模型管理：

- **状态枚举** `ModelProviderStatus`：`Deleted=0`(软删除) / `Normal=1`(启用) / `Disabled=2`(未启用，创建时降级)
- **创建策略**：允许任意数量的 Embedding Provider 创建，但同一时刻仅一个 Normal(启用)；已有启用者时新创建自动降级为 Disabled
- **启用切换**：将 Disabled 切换为 Normal 时，通过 409 `embedding_provider_switch_required` 守卫强制走 `switch_embedding` 流程（软删旧+启用新+全量重建）
- **重建触发条件矩阵**：创建/更新 Embedding 时按生效状态条件注册 `RebuildVectorsTask`，避免无谓的全量重建

本卡与「向量存储抽象 VectorStore + embed_entity」卡构成互补视角：该卡聚焦向量存储基础设施，本卡聚焦 Embedding Provider 业务生命周期与重建触发条件。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [common/src/enums/agent.rs](common/src/enums/agent.rs) | ModelProviderStatus 枚举 | `Deleted=0` / `Normal=1` / `Disabled=2`；`from_i32(2)` → `Disabled` |
| [common/src/api/model_provider.rs](common/src/api/model_provider.rs) | DTO 定义 | `CreateModelProviderResponse { status, rebuild_task_id }`；`UpdateModelProviderResponse { rebuild_task_id }` |
| [common/src/api/organization.rs](common/src/api/organization.rs) | 初始化请求 DTO | `chat_model: Option<ModelProviderInitConfig>` + `#[serde(default]`；`chat_provider_id: Option<String>` |
| [src/models/model_provider.rs](src/models/model_provider.rs) | PO + Entity | `ModelProviderPo` 含 `status: ModelProviderStatus`；`ModelProvider::new()` 默认 `Normal` |
| [src/service/domain/finance/model_provider.rs](src/service/domain/finance/model_provider.rs) | Domain 核心逻辑 | `create_model_provider`：Embedding 降级 Disabled；`update_model_provider`：409 switch_required；`switch_embedding_provider`：软删旧+启用新 |
| [src/handlers/finance/model_provider/create_model_provider.rs](src/handlers/finance/model_provider/create_model_provider.rs) | 创建 Handler | 回读落库状态；首个 Normal Embedding 注册重建；响应含 status + rebuild_task_id |
| [src/handlers/finance/model_provider/update_model_provider.rs](src/handlers/finance/model_provider/update_model_provider.rs) | 更新 Handler | `embedding_config_changed` 检测；仅 Normal Embedding 配置变化触发重建 |
| [src/handlers/finance/model_provider/switch_embedding.rs](src/handlers/finance/model_provider/switch_embedding.rs) | 切换 Handler | 软删旧 provider + 启用新 + 注册 RebuildVectorsTask |
| [src/handlers/finance/model_provider/rebuild_vectors_task.rs](src/handlers/finance/model_provider/rebuild_vectors_task.rs) | 重建后台任务 | 顺序重建 7 类实体（agent/memory/skill/task/project/message/tool）的向量索引 |
| [src/handlers/organization/initialize_system.rs](src/handlers/organization/initialize_system.rs) | 初始化 Handler | 动态步骤数 `3 + chat + embedding`；条件化创建 provider；入口边界校验 |
| [frontend/src/pages/finance/model_providers.rs](frontend/src/pages/finance/model_providers.rs) | 前端模型管理页 | 创建 Modal capability 选择(Agent/Embedding)；禁用按钮发 status=2；删除 Embedding 警示文案 |
| [frontend/src/pages/reception.rs](frontend/src/pages/reception.rs) | 前端初始化页 | 2 步表单：基础信息→模型配置；对话/向量模型可选+跳过后果提示 |

## §3 架构约定

### 3.1 Embedding 创建降级矩阵

| 场景 | 创建结果 | 是否注册重建 | 原因 |
|------|---------|-------------|------|
| 初始化时创建首个 Embedding | Normal(启用) | ✅ | 后补场景：存量实体无向量，需全量补建 |
| 初始化后创建首个 Embedding | Normal(启用) | ✅ | 同上 |
| 已有启用 Embedding 时再创建 | Disabled(未启用) | ❌ | 未生效；重建推迟到启用切换时 |

### 3.2 Embedding 更新重建触发矩阵

| 场景 | 是否注册重建 | 原因 |
|------|-------------|------|
| 编辑 Normal Embedding 的 model_name/api_key/base_url | ✅ | 向量空间变化，使用中的索引需重建 |
| 编辑 Disabled Embedding | ❌ | 未生效，启用时 switch 全量重建兜底 |

### 3.3 初始化动态步骤数

```total_steps = 3 + usize::from(chat_model.is_some()) + usize::from(embedding_model.is_some())```

- 基础 3 步：创建组织 + 同步内置工具 + 导入预置技能
- 对话模型：可选，未配置时跳过
- 向量模型：可选，未配置时跳过

### 3.4 前端表单步骤

| Step | 内容 | 校验规则 |
|------|------|---------|
| 1 | 基础信息（组织名/用户名/密码等） | 仅校验非空 |
| 2 | 模型配置（对话/向量可选 + 跳过提示） | 启用时校验必填字段 |

### 3.5 状态与操作语义分离

- **禁用** `status=2`(Disabled)：条目保留在列表，显示「未启用」badge，可再启用
- **删除** `status=0`(Deleted)：软删除，条目从列表消失，不可恢复
- **前端按钮对应**：「禁用」按钮发 `UpdateModelProviderStatusRequest { status: 2 }`；「删除」按钮走 `delete_model_provider` API

## §4 硬约束与回归红线

1. ❌ **禁止 Embedding 创建阻塞**：已有启用 Embedding 时新创建必须降级为 Disabled(2)，不得返回 409/错误；违反 = 用户无法配置备用模型。
2. ❌ **禁止 Disabled Embedding 编辑触发重建**：只有使用中(Normal) Embedding 的 `model_name`/`api_key`/`base_url` 变化才触发重建；违反 = 无谓的全量重建浪费资源。
3. ❌ **禁止在 Domain 层绕过 switch 直接启用 Embedding**：将 Disabled 改为 Normal 必须走 `update_model_provider` 的 409 `embedding_provider_switch_required` 守卫，强制前端弹确认 modal 告知用户后果。
4. ✅ **初始化步骤数必须动态计算**：`total_steps = 3 + chat + embedding`，不得硬编码 4/5；违反 = 前端进度条显示错误。
5. ✅ **初始化响应 chat_provider_id 必须为 Option**：未配置对话模型时响应中为 null，不得返回空字符串或随机 ID。
6. ✅ **回读创建结果状态**：`create_model_provider` 返回后必须 `get_model_provider` 回读，因为 Domain 层可能已将状态降级为 Disabled；直接用 `provider.po.status` 构造响应会返回错误的 Normal 状态。
7. ✅ **向量重建任务注册必须用 Arc::new + registry().register()**：参照 `switch_embedding.rs` 的注册方式，返回 `task_id` 供前端进度轮询。
8. ✅ **前端禁用按钮必须发 status=2**：不得复用 status=0(软删除)；违反 = 条目从列表消失，与「禁用」语义矛盾。