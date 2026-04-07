# ai_orz

AI 代理执行框架 - Full-stack Rust + Dioxus

## 技术栈

- **后端**: Rust + Axum + SQLite + [rig-core](https://docs.rs/rig-core/latest/rig/) (LLM 调用框架)
- **前端**: Dioxus 0.7 (WebAssembly)
- **构建**: dioxus-cli + cargo workspace

## 项目结构

```
ai_orz/
├── src/                # 后端源码
├── frontend/            # Dioxus 前端源码
├── dist/               # 编译好的前端静态文件（生产构建输出）
├── docs/               # 详细文档
│   └── ARCHITECTURE.md # 完整架构说明（最新）
├── build-full.sh        # 全量构建脚本（后端 + 前端）
├── start-dev.sh         # 一键启动开发环境（后端 + dx serve 热重载）
└── README.md
```

## 核心概念（最终版）

| 概念 | 层次 | 职责 |
|------|------|------|
| **Agent** | 模型层 | 顶级智能体，持有装配好的 Brain |
| **Brain** | 模型层 | 聚合根，持有完整的 Cortex 思考模块 |
| **Cortex** 🧠 | 模型层 | 思考组合，持有 ModelProvider 配置 + 具体推理实现 |
| **CortexTrait** | 模型层 | 推理接口 trait，不同提供商不同实现 |
| **ModelProvider** | 模型层 | 模型配置，保存持久化信息，可以被多个 Agent 复用 |
| **CortexDao** | DAO 层 | 工厂 DAO，统一创建 `CortexTrait` + 执行 `prompt` |
| **CortexDal** | DAL 层 | DAL 业务层，组装完整 `Cortex` 实体 |
| **HR (Human Resources)** | 领域层 | 人力资源领域，统一管理 AI 智能体和人类员工 |
| **Finance (财务管理)** | 领域层 | 财务管理领域，统一管理 LLM 模型提供商（付费资源） |

## 最终实体层次关系 🎯

```
Agent (po + brain: Option<Brain>)
  └─► Brain (cortex: Cortex)
       └─► Cortex (model_provider: ModelProvider, cortex: Box<dyn CortexTrait + Send + Sync>)
            ├─► ModelProvider (po: ModelProviderPo)
            └─► dyn CortexTrait (async fn prompt(&self, prompt: &str) -> Result<String>)
```

更多详细架构说明请查看 [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)

## 设计原则

1. **严格分层** → `handlers → domain → dal → dao → models`，不允许跨层级调用
2. **强约定** → 所有 service 层（DAO/DAL/Domain）公共方法都必须传递 `ctx: RequestContext` ✅
3. **高内聚低耦合** → 领域模块拆分清晰，trait 定义在 mod.rs，实现在子文件
4. **统一编码规范** → 所有 DAO 使用 `OnceLock<Arc<dyn Trait>>` 单例模式
5. **完整命名** → 子功能 trait 方法完整命名：`create_agent` 而不是 `create`
6. **Handler 拆分** → 业务分组 + 方法粒度拆分，每个方法一个单独文件 ✅
7. **单元测试** → 每个业务模块都应该有单元测试，当前 31/31 全部通过 ✅

## LLM 调用流程（最新版）

```rust
// Handler 层调用方式
use crate::service::domain::finance::domain;

// 1. Handler 先查询 Model Provider（已经找到实例）
let provider = domain().model_provider_manage().get_model_provider(ctx, id)?
    .ok_or_else(|| AppError::NotFound(...))?;

// 2. 调用 domain 直接唤醒执行
let result = domain().model_provider_manage().wake_cortex(ctx, &provider, prompt)?;
```

## 支持的模型提供商

| 提供商 | 支持 |
|--------|------|
| OpenAI 官方 | ✅ |
| DeepSeek | ✅ |
| 阿里云通义千问 | ✅ |
| 字节跳动豆包 | ✅ |
| Ollama 本地 | ✅ |
| 自定义 OpenAI 兼容接口 | ✅ |

## 配置

项目配置通过环境变量读取：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVER_HOST` | `0.0.0.0` | 服务监听地址 |
| `SERVER_PORT` | `3000` | 服务监听端口 |
| `DATABASE_URL` | `data/ai_orz.db` | SQLite 数据库路径 |

## 快速开始

### 开发模式

```bash
./start-dev.sh
```

- 后端服务: http://localhost:3000
- 前端开发服务器 (热重载): http://localhost:8080

### 生产构建

```bash
./build-full.sh
```

输出：
- 后端二进制: `target/release/ai_orz`
- 前端静态文件: `dist/`

### 运行生产版本

```bash
./target/release/ai_orz
```

服务启动后监听 `0.0.0.0:${SERVER_PORT:-3000}`，前端静态文件从 `dist/` 目录提供。

## API 接口

### HR (人力资源) - Agent 管理

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/hr/agents` | 创建 Agent |
| GET | `/api/v1/hr/agents` | 列出所有 Agent |
| GET | `/api/v1/hr/agents/{id}` | 获取单个 Agent |
| PUT | `/api/v1/hr/agents/{id}` | 更新 Agent |
| DELETE | `/api/v1/hr/agents/{id}` | 删除 Agent |

### Finance (财务管理) - Model Provider 管理

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/finance/model-providers` | 创建模型提供商 |
| GET | `/api/v1/finance/model-providers` | 列出所有模型提供商 |
| GET | `/api/v1/finance/model-providers/{id}` | 获取单个模型 |
| PUT | `/api/v1/finance/model-providers/{id}` | 更新模型 |
| DELETE | `/api/v1/finance/model-providers/{id}` | 删除模型 |
| POST | `/api/v1/finance/model-providers/{id}/test` | 测试连通性 |
| POST | `/api/v1/finance/model-providers/{id}/call` | 调用模型生成文本 |

### 健康检查

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |

## 前端架构

```
frontend/src/
├── main.rs              # 入口，App 组件
├── api/                # API 调用模块
│   ├── health.rs       # 健康检查 API
│   ├── agent.rs        # Agent 管理 API
│   └── model_provider.rs # Model Provider 管理 API
└── components/         # UI 组件
    ├── navbar.rs        # 顶部导航栏
    ├── reception.rs     # 前台接待欢迎页
    ├── agent_management.rs # Agent 管理页
    └── model_provider_management.rs # Model Provider 管理页
```

前端已经实现：
- ✅ 顶部导航栏
  - 前台接待
  - 人力资源 → Agent 管理
  - **财务管理 → 模型管理** 👈 新增
- ✅ 前台接待欢迎页
- ✅ Agent 管理列表 + 创建弹窗 + 删除功能
- ✅ **Model Provider 管理列表 + 创建弹窗 + 删除功能 + 创建后自动测试连通性** 👈 新增

## 前端开发

项目使用 Dioxus 0.7 Web 框架开发前端：

- **开发**: `cd frontend && dx serve` 支持热重载
- **构建**: `dx build --release` 输出生成版本
- **打包**: `./build-full.sh` 自动编译前后端并打包到 `dist/`

## 端口说明

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| 后端 API | 3000 | 提供 REST API 和静态文件服务 |
| 前端开发服务器 | 8080 | dx serve 热重载开发服务器 |

## 单元测试

运行所有单元测试：

```bash
cargo test
```

当前状态：**31 个测试全部通过 ✅**

## License

MIT
