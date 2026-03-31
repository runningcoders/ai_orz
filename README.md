# ai_orz

AI 代理执行框架

## 架构

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

## 快速开始

```bash
cargo run
```

服务启动后监听 `0.0.0.0:3000`

## API 接口

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/agents` | 创建 Agent |
| GET | `/api/v1/agents` | 列出所有 Agent |
| GET | `/api/v1/agents/:id` | 获取单个 Agent |
| PUT | `/api/v1/agents/:id` | 更新 Agent |
| DELETE | `/api/v1/agents/:id` | 删除 Agent |

##  License

MIT
