# AI Orz 项目文档

## 项目简介

AI 代理执行框架，全栈 Rust + Dioxus 实现。

- 后端：Rust + Axum + SQLite
- 前端：Dioxus 0.7 WebAssembly
- 构建：cargo workspace + dioxus-cli

## 目录结构

```
ai_orz/
├── src/                    # 后端源码
│   ├── models/             # 实体定义
│   ├── pkg/                # 工具和基础设施
│   ├── service/            # 业务分层 (DAO/DAL/Domain)
│   ├── handlers/           # HTTP 接口
│   └── ...
├── frontend/              # Dioxus 前端源码
│   ├── src/
│   │   └── main.rs        # 入口 + 健康检查页面
│   ├── Cargo.toml
│   └── index.html
├── dist/                  # 生产构建输出（前端静态文件）
├── docs/                  # 文档
├── target/                # 编译输出
├── build-full.sh          # 全量构建脚本
├── start-dev.sh          # 开发启动脚本
└── README.md
```

## 环境配置

### 环境变量

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVER_HOST` | `0.0.0.0` | 服务监听地址 |
| `SERVER_PORT` | `3000` | 服务监听端口 |
| `DATABASE_URL` | `data/ai_orz.db` | SQLite 数据库文件路径 |

### 端口说明

| 服务 | 默认端口 | 场景 |
|------|----------|------|
| 后端 API + 静态文件 | 3000 | 生产环境 / 开发环境后端 |
| dioxus-cli 开发服务器 | 8080 | 开发环境前端热重载 |

## 快速开始

### 开发模式

```bash
./start-dev.sh
```

- 后端 API: http://localhost:3000
- 前端开发: http://localhost:8080 (自动热重载)

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

访问 http://localhost:3000 即可看到前端页面。

## 技术架构

### 后端分层

```
Handler (HTTP 接口)
    ↓
Domain (业务逻辑)
    ↓
DAL (数据访问层)
    ↓
DAO (存储操作层)
    ↓
SQLite 数据库
```

遵循命名规范和 Context 传递规范，详见 [NAMING_CONVENTION.md](./NAMING_CONVENTION.md)。

### 前端架构

- Dioxus 0.7 响应式框架
- WebAssembly 编译
- 组件化开发
- 支持热重载开发

## 开发指南

### 新增后端接口

1. 在 `models/` 定义实体
2. 在 `service/dao/` 写 DAO 接口和实现
3. 在 `service/dal/` 写数据访问层
4. 在 `service/domain/` 写业务逻辑
5. 在 `handlers/` 写 HTTP 接口
6. 在 `src/router.rs` 注册路由

### 新增前端页面

1. 在 `frontend/src/` 新建组件
2. 在 `main.rs` 引入渲染
3. `dx serve` 自动热重载

## 部署

### 单二进制部署

1. 在开发机器执行 `./build-full.sh`
2. 复制 `target/release/ai_orz` 和 `dist/` 到服务器
3. 设置环境变量（如果需要自定义端口/数据库）
4. 运行 `./ai_orz`
5. 访问 `http://your-server:3000`

### 开发部署

本地开发直接用 `./start-dev.sh` 即可。

## 依赖清单

### 编译依赖

- Rust 1.70+
- cargo
- dioxus-cli v0.7+
- wasm-bindgen-cli
- openssl-dev (macOS: brew install openssl@3)
- pkgconf (macOS: brew install pkgconf)

### 运行依赖

- 无，单二进制静态链接，不需要额外运行时依赖

## 许可证

MIT
