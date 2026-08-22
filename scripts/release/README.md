# AI Orz `__VERSION__` (`__TARGET__`)

开箱即用的多 Agent 协作平台（后端 Rust + 前端 Web）。

本包 = 单个二进制 `ai_orz` + 前端静态资源 `dist/` + 运维脚本 `script/` + 命令入口 `Makefile`。可放在任意目录运行，所有数据保存在包目录下的 `.ai_orz/` 里。

## 快速开始

```bash
make start        # 后台启动（推荐）
# 或前台启动: ./script/start.sh -f
```

浏览器访问 http://localhost:3000（远程服务器用 IP，需放行端口）。

```bash
make stop         # 停止服务
make restart      # 重启服务
make status       # 查看运行状态
make logs         # 实时跟踪运行日志
```

## 常用命令（Makefile）

| 命令 | 说明 |
|------|------|
| `make start` | 后台启动服务，日志写入 `.ai_orz/run.log` |
| `make stop` | 停止服务（优雅停止，超时强杀） |
| `make restart` | 重启服务 |
| `make status` | 查看运行状态与监听端口 |
| `make logs` | 实时查看运行日志（`tail -f`） |

脚本也可直接调用：`./script/start.sh`（`-f` 前台）/ `./script/stop.sh` 等。

## 首次使用（系统初始化）

首次打开页面会进入初始化引导，按提示完成：

- 创建超级管理员账号（默认用户名 `admin`，密码由你设置）
- 配置模型提供商 API Key（对话模型；向量模型可选）

完成后用 `admin` 登录即可开始使用。

## 配置文件

首次启动自动生成 `.ai_orz/ai_orz.toml`，可编辑后重启生效。**首次启动时，若设置了 `AI_ORZ_LISTEN_ADDR` / `JWT_SECRET` / `JWT_EXPIRY_HOURS` / `FRONTEND_DIST_DIR` / `SECRET_KEY` 等环境变量，其值会固化进本配置文件**（后续即使环境变量消失也保持生效）；之后每次运行环境变量仍优先，但不再改写文件。常用项：

| 配置 | 说明 |
|------|------|
| `server.listen_addr` | 监听地址与端口（默认 `0.0.0.0:3000`） |
| `server.timezone` | 时区，cron 等时间计算使用（默认 `Asia/Shanghai`） |
| `frontend.dist_dir` | 前端静态目录（默认 `dist`） |
| `logging.enable_file_log` | 是否写文件日志（默认 true） |
| `jwt.secret` | JWT 签名密钥（生产务必修改） |
| `security.secret_key` | 敏感数据加密密钥（生产务必修改） |

## 环境变量（可选，优先级高于配置文件）

| 变量 | 说明 |
|------|------|
| `AI_ORZ_LISTEN_ADDR` | 覆盖监听地址（`server.listen_addr`），如 `AI_ORZ_LISTEN_ADDR=0.0.0.0:8080`；首次初始化固化进配置 |
| `SECRET_KEY` | 覆盖敏感数据加密密钥（`security.secret_key`）；首次初始化固化进配置 |
| `AI_ORZ_BASE_PATH` | 覆盖数据目录（默认 `.ai_orz/`） |
| `JWT_SECRET` | 覆盖 JWT 签名密钥；首次初始化固化进配置 |
| `JWT_EXPIRY_HOURS` | 覆盖 JWT 过期时间（默认 168） |
| `FRONTEND_DIST_DIR` | 覆盖前端静态目录 |

## 数据与备份

所有数据都在 `.ai_orz/` 目录下：`ai_orz.toml`（配置）、`ai_orz.db`（SQLite 主数据库）、`stats.duckdb`（统计数据库）、`logs/`（运行日志），以及附件、Agent 记忆、技能、工具追踪日志等。

**备份 = 备份整个 `.ai_orz/` 目录；迁移 = 整目录拷贝到新环境后启动即可。**

## 安全提醒（生产环境必读）

默认密钥仅供开发，上线前务必修改 `.ai_orz/ai_orz.toml` 中的 `security.secret_key` 与 `jwt.secret`，或改用环境变量 `JWT_SECRET` 注入，避免明文落盘。
