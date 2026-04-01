# 架构说明

## 整体架构

**全栈 Rust 单仓库项目**

```
 ┌───────────────────────────────────────────────────────────────────┐
 │                        浏览器 / 客户端                                │
 └───────────────────────────────────────────────────────────────────┘
                              ↓ HTTP
 ┌───────────────────────────────────────────────────────────────────┐
 │                 Axum 后端 (Rust)                                   │
 │  - REST API                                                      │
 │  - 静态文件服务 (dist 目录)                                       │
 │  - SQLite 存储                                                   │
 └───────────────────────────────────────────────────────────────────┘
                              ↓
 ┌───────────────────────────────────────────────────────────────────┐
 │                 Dioxus 前端 (WebAssembly)                          │
 │  - 组件化开发                                                    │
 │  - 响应式状态管理                                                │
 │  - 浏览器运行                                                    │
 └───────────────────────────────────────────────────────────────────┘
```

## 后端分层架构

### 分层

| 层 | 位置 | 职责 |
|---|------|------|
| **Handler** | `src/handlers/` | HTTP 接口处理，参数解析，返回响应 |
| **Domain** | `src/service/domain/` | 业务逻辑实现 |
| **DAL** | `src/service/dal/` | 数据访问层，组合 DAO 构建业务对象 |
| **DAO** | `src/service/dao/` | 底层存储操作，SQL 执行 |
| **Models** | `src/models/` | 实体定义 (PO, BO) |
| **Pkg** | `src/pkg/` | 基础设施工具 (日志、常量、存储) |

### 设计原则

1. **清晰分层** - 每层只负责自己的职责，不越界
2. **依赖方向** - Handler → Domain → DAL → DAO → Storage，单向依赖
3. **Context 传递** - 所有公共方法第一个参数是 `RequestContext`，保存用户信息和日志上下文
4. **命名一致** - 遵循 [NAMING_CONVENTION.md](./NAMING_CONVENTION.md)

### 初始化流程

```rust
// 按顺序初始化，保证依赖正确
dao::init_all();      // 初始化所有 DAO
dal::init_all();      // 依赖 DAO
domain::init_all();    // 依赖 DAL
```

## 前端架构

### 技术栈

- **框架**: Dioxus 0.7 (Web)
- **编译目标**: `wasm32-unknown-unknown`
- **构建工具**: dioxus-cli (`dx`)
- **开发模式**: `dx serve` 热重载
- **生产构建**: `dx build --release` 输出优化 WASM

### 项目结构

```
frontend/
├── src/
│   └── main.rs         # 入口组件，当前包含健康检查页面
├── Cargo.toml         # 依赖配置
├── index.html         # HTML 入口模板
└── target/           # 编译输出
```

## 配置管理

### 环境变量

所有配置通过环境变量注入，不硬编码：

```bash
# 示例
export SERVER_HOST=0.0.0.0
export SERVER_PORT=3000
export DATABASE_URL=./data/ai_orz.db
./target/release/ai_orz
```

### 默认值

不设置环境变量时使用合理的默认值，方便开发。

## 静态文件服务

生产模式下，后端自动提供 `dist/` 目录下的前端静态文件：

- `GET /` → `dist/index.html`
- `GET /wasm/*.wasm` → 编译好的 WebAssembly
- `GET /api/*` → 后端 API
- `GET /health` → 健康检查

## 构建流程

### 开发构建

```bash
./start-dev.sh
# 1. 启动后端 cargo run
# 2. 启动前端 dx serve (热重载)
```

### 生产构建

```bash
./build-full.sh
# 1. dx build --release 编译前端
# 2. 复制产物到 dist/
# 3. cargo build --release 编译后端
```

## 技术选型理由

### 为什么选 Rust + Dioxus?

1. **全栈 Rust** - 前后端同一种语言，减少上下文切换
2 **- 内存安全** - Rust 内存安全，无 GC，适合长期运行的服务
3. **单二进制部署** - 编译一个二进制文件就能运行，包含前端
4. **WASM 前端** - 一次类型，前端后端共享类型定义（未来可扩展）
5. **热重载开发** - dx serve 支持热重载，开发体验好

### 为什么选 Axum?

- 官方维护，活跃开发
- 异步清晰，API 友好
- 生态完善，支持 static file service

### 为什么选 SQLite?

- 单文件数据库，不需要额外服务
- 适合中小型项目，部署简单
- 足够稳定，性能满足需求

## 扩展性

- 新增实体 → 按分层依次添加
- 新增页面 → 前端直接添加组件
- 更换存储 → 只需要换 DAO 实现
- 前后端分离部署 → 前端独立部署，后端只提供 API

