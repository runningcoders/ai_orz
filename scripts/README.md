# scripts/ 脚本地图

> 目标：**一个功能只有一处实现**。所有入口（Makefile、git 钩子、CI、旧脚本名）都只做转发，
> 行为改动只改一个文件，不会出现在几处同步。

## 一、两条入口（任选其一，等价）

| 入口 | 用法 | 说明 |
|------|------|------|
| `make <命令>` | `make dev` / `make prod` / `make stop` | 根目录 Makefile，纯转发，推荐日常使用 |
| `./scripts/ai_orz.sh <命令>` | `./scripts/ai_orz.sh dev` | 脚本侧统一入口，同样是纯转发 |

`make xxx` 与 `./scripts/ai_orz.sh xxx` 一一对应；`ai_orz.sh` 再分发到下面的专职脚本。

## 二、目录结构

```
scripts/
├── ai_orz.sh           统一入口：参数解析 + 转发，不含任何实现
├── lib/                公共库（被仓库脚本与发布包共用，禁止直接执行）
│   ├── common.sh       颜色 / 路径推导 / PATH 补齐 / 输出助手
│   └── service.sh      服务进程治理：二进制定位、PID 文件、端口等待、优雅停止
├── run.sh              【实现】开发态启动：dev / backend / frontend
├── prod.sh             【实现】生产态生命周期：build / start / stop / status / logs / restart
├── build_frontend.sh   【实现】前端 dx build --release → dist/（Dockerfile 也复用它）
├── cleanup.sh          【实现】残留进程与端口占用清理（--dry-run 只列不杀）
├── check_deps.sh       【实现】依赖预检（[模式] [--fix]）
├── check.sh            【实现】代码门禁：fmt / clippy / clippy-fe / test / lint / ci / coverage
├── package.sh          【实现】发布物打包（CI release.yml 直接调用）
├── migrate_tool_call_trace.sh  一次性数据迁移工具（call_trace 目录拉平），独立运行
├── start.sh / stop.sh / status.sh / logs.sh / restart.sh / build.sh
│                       兼容别名 → ai_orz.sh（旧调用方式与文档引用保持可用）
└── release/            发布包模板：Makefile + README.md + script/（包内别名）
```

## 三、命令对照表

| 功能 | make | ai_orz.sh | 实现脚本 |
|------|------|-----------|----------|
| 开发全栈 | `make dev` | `dev` | `run.sh dev` |
| 仅后端 / 仅前端 | `make run` / `make serve` | `backend` / `frontend` | `run.sh` |
| 全量 release 构建 | `make build` | `build` | `prod.sh build`（内含 `build_frontend.sh`） |
| 仅前端构建 | `make build-fe` | `build-fe` | `build_frontend.sh` |
| 构建 + 后台启动 | `make prod` | `prod` | `prod.sh build` + `cleanup.sh` + `prod.sh start` |
| 停止 / 重启 | `make stop` / `make restart` | `stop` / `restart` | `prod.sh` |
| 状态 / 日志 | `make status` / `make logs` | `status` / `logs` | `prod.sh` |
| 残留进程清理 | `make clean-proc` | `clean [--dry-run]` | `cleanup.sh` |
| 依赖预检 | `make doctor` | `doctor [模式] [--fix]` | `check_deps.sh` |
| 代码门禁 | `make lint` / `make ci` | `check lint` / `check ci` | `check.sh` |
| 发布打包 | `make package` | `package [版本]` | `package.sh` |
| 数据迁移 | — | `migrate [--apply]` | `migrate_tool_call_trace.sh` |

## 四、两条硬规则

1. **新功能先在某个 `【实现】` 脚本里落地，再在 `ai_orz.sh` + Makefile 各加一行转发**；
   不要在入口里写实现逻辑，否则很快又变成「同一功能两份」。
2. **通用片段下沉到 `lib/`**：颜色、路径推导、PATH 补齐 → `common.sh`；
   端口等待、PID 文件、优雅停止 → `service.sh`。发现第三处重复时，先想能不能进 lib。

## 五、发布包如何复用这些脚本

`package.sh` 打包时会把 `scripts/prod.sh` 与 `scripts/lib/*.sh` 复制进发布包的 `script/`：

```
ai_orz-<版本>-<平台>/
├── ai_orz                 服务二进制
├── dist/                  前端静态资源
├── script/
│   ├── prod.sh            与仓库 scripts/prod.sh 同一份文件
│   ├── lib/               prod.sh 的依赖库
│   └── start.sh|stop.sh|restart.sh|status.sh|logs.sh   → prod.sh 的转发别名
├── Makefile               make start / stop / restart / status / logs
└── README.md
```

**仓库与发布包共用同一份生产服务实现**：`prod.sh` 通过 `lib/service.sh::in_repo_checkout()`
（根目录有无 `Cargo.toml`）区分环境——仓库里启动前需要构建、控制台日志交给后端文件层；
发布包里直接跑二进制、stdout 落 `run.log`。因此「怎么停服务」不会有两份漂移的实现。
