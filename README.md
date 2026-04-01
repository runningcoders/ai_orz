# ai_orz

AI 代理执行框架 - Full-stack Rust + Dioxus

## 技术栈

- **后端**: Rust + Axum + SQLite
- **前端**: Dioxus 0.7 (WebAssembly)
- **构建**: dioxus-cli + cargo workspace

## 项目结构

```
ai_orz/
├── src/                # 后端源码
├── frontend/            # Dioxus 前端源码
├── dist/               # 编译好的前端静态文件（生产构建输出）
├── build-full.sh        # 全量构建脚本（后端 + 前端）
├── start-dev.sh         # 一键启动开发环境（后端 + dx serve 热重载）
└── README.md
```

## 架构（后端）

```
src/
├── models/              # 实体定义
│   ├── agent.rs         # AgentPo (持久化对象) + Agent (业务对象)
│   ├── organization.rs  # OrganizationPo 组织实体
│   └── task.rs          # Task 任务实体
├── pkg/                # 工具和基础设施
│   ├── constants.rs     # 常量定义
│   ├── logging.rs       # 日志模块（带 Context）
│   └── storage/        # 存储管理
│       ├── mod.rs       # 全局连接管理
│       └── sql.rs      # SQL 常量定义（SQLITE_CREATE_TABLE_*）
├── service/
│   ├── dao/            # DAO 层（存储操作）
│   │   └── agent/      # Agent DAO
│   │       ├── mod.rs    # 接口定义
│   │       ├── sqlite.rs # SQLite 实现
│   │       └── sqlite_test.rs # 单元测试
│   ├── dal/            # DAL 层（数据访问层）
│   │   ├── agent.rs     # Agent DAL
│   │   └── agent_test.rs  # 单元测试
│   └── domain/         # Domain 层（业务逻辑）
│       ├── agent.rs     # Agent Domain 业务逻辑
│       └── agent_test.rs # 单元测试
└── handlers/          # HTTP 接口
    ├── agent/          # Agent Handler
    │   ├── mod.rs       # Handler 方法
    │   └── dto.rs       # 请求/响应 DTO
    ├── mod.rs           # 导出 + 通用 ApiResponse
    └── health.rs       # 健康检查
```

## 设计原则

1. **分层清晰**
   - DAO：只负责底层存储操作，不关心业务
   - DAL：组合 DAO，构建业务对象，不关心业务逻辑
   - Domain：实现业务逻辑，不关心 HTTP 和存储
   - Handler：只负责 HTTP 接口处理，不关心业务逻辑

2. **命名规范**
   - 文件：`snake_case.rs`
   - 函数/变量：`snake_case`
   - 类型/结构体：`PascalCase`
   - 常量：`SQLITE_CREATE_TABLE_AGENTS`（全大写+下划线）

3. **测试分离**
   - 核心逻辑：`xxx.rs`
   - 单元测试：`xxx_test.rs`
   - 保持核心文件干净

4. **Context 传递**
   - 所有公共方法第一个参数都是 `ctx: RequestContext`
   - `created_by`/`modified_by` 从 `ctx.uid()` 获取
   - 保证链路完整性，便于日志追踪

5. **分层初始化**
   - `dao::init_all()` 初始化所有 DAO
   - `dal::init_all()` 初始化所有 DAL（依赖已初始化的 DAO）
   - `domain::init_all()` 初始化所有 Domain（依赖已初始化的 DAL）
   - `service::init()` 按顺序调用三层初始化
   - 每层只负责自己的实例，结构清晰，易于扩展

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

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/agents` | 创建 Agent |
| GET | `/api/v1/agents` | 列出所有 Agent |
| GET | `/api/v1/agents/:id` | 获取单个 Agent |
| PUT | `/api/v1/agents/:id` | 更新 Agent |
| DELETE | `/api/v1/agents/:id` | 删除 Agent |

## 前端开发

项目使用 Dioxus 0.7 Web 框架开发前端：

- **开发**: `cd frontend && dx serve` 支持热重载
- **构建**: `dx build --release` 输出生产版本
- **打包**: `./build-full.sh` 自动编译前后端并打包到 `dist/`

## 端口说明

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| 后端 API | 3000 | 提供 REST API 和静态文件服务 |
| 前端开发服务器 | 8080 | dx serve 热重载开发服务器 |

##  License

MIT
