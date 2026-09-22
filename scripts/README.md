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
│   └── service.sh      服务进程治理：部署根/数据目录/静态目录、二进制定位、PID、端口等待、优雅停止
├── run.sh              【实现】开发态启动：dev / backend / frontend
├── prod.sh             【实现】生产态生命周期：build / install / start / stop / status / logs / restart
├── build_frontend.sh   【实现】前端 dx build --release → dist/（Dockerfile 也复用它）
├── cleanup.sh          【实现】残留进程与端口占用清理（--dry-run 只列不杀）；
│                       后端走 `service.sh::graceful_stop`（与 make stop 同一条链路）
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
| 全量 release 构建 | `make build` | `build` | `prod.sh build`（内含 `build_frontend.sh`）：只编译，产物留在仓库内 `target/release/ai_orz` + `dist/` |
| 搬运到部署根 | `make install` | `install` | `prod.sh install`：把 build 产物搬到 `<部署根>/bin` 与 `<部署根>/dist`（`make prod` 已内含） |
| 仅前端构建 | `make build-fe` | `build-fe` | `build_frontend.sh` |
| 构建 + 搬运 + 后台启动 | `make prod` | `prod` | `prod.sh build` + `prod.sh install` + `cleanup.sh` + `prod.sh start` |
| 停止 / 重启 | `make stop` / `make restart` | `stop` / `restart` | `prod.sh` |
| 状态 / 日志 | `make status` / `make logs` | `status` / `logs` | `prod.sh` |
| 残留进程清理 | `make clean-proc` | `clean [--dry-run]` | `cleanup.sh` |
| 依赖预检 | `make doctor` | `doctor [模式] [--fix]` | `check_deps.sh` |
| 代码门禁 | `make lint` / `make ci` | `check lint` / `check ci` | `check.sh` |
| 发布打包 | `make package` | `package [版本]` | `package.sh`（消费 `prod.sh build` 产物，**不**搬运到部署根） |
| 数据迁移 | — | `migrate [--apply]` | `migrate_tool_call_trace.sh` |

## 四、两条硬规则

1. **新功能先在某个 `【实现】` 脚本里落地，再在 `ai_orz.sh` + Makefile 各加一行转发**；
   不要在入口里写实现逻辑，否则很快又变成「同一功能两份」。
2. **通用片段下沉到 `lib/`**：颜色、路径推导、PATH 补齐 → `common.sh`；
   端口等待、PID 文件、优雅停止 → `service.sh`。发现第三处重复时，先想能不能进 lib。
3. **停后端只有一条链路**：`service.sh::graceful_stop`（事件驱动等终态日志 / 进程退出，
   超时才 `-9`）。`prod.sh stop`、`prod.sh start` 的幂等重启、`cleanup.sh` 全部走它。
   ❌ 禁止对后端二进制写「`kill` + 固定 `sleep N` + `kill -9`」——优雅退出含 10s HTTP drain
   窗口（SSE 长连接必然吃满）+ 渠道停服 + AOP 排空 + DuckDB flush，1s 就 `-9` 必丢统计落盘。

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

## 六、部署根：生产实例的落点

生产实例（`make prod`）的「数据 / 二进制 / 前端产物 / PID / run.log」统一以**部署根**为根，默认值按环境分叉：

| 环境 | 部署根默认值 | 数据目录 | 二进制 | 前端产物 | PID / run.log |
|------|-------------|----------|--------|----------|----------------|
| 仓库 checkout | `$HOME/.ai_orz` | `<部署根>/data` | `<部署根>/bin/ai_orz`（`make install` 搬运） | `<部署根>/dist`（同上搬运） | `<部署根>/` 下 |
| 发布包 | 解压目录自身 | `<包根>/.ai_orz` | `<包根>/ai_orz` | `<包根>/dist`（打包时即放入） | `<包根>/.ai_orz/`（历史布局不变） |

**为什么这么做**：生产数据不再长在 git 工作树里 —— `git clean -xdf`、切分支、删仓库都不影响它；
同一台机器从任意 checkout 调用 `make prod`，都指向同一个生产实例。
二进制安装到部署根是为了和 `target/` 解耦：`cargo clean` / 重建 target 不会让生产实例下次重启时找不到二进制。
前端产物同理，而且更迫切 —— `dist/` 本身就在 `.gitignore` 里，一次 `git clean -xdf` 就会连它一起删掉。

⚠️ **前端产物缺失的症状不是 404，而是白屏**：`src/router.rs` 的 SPA 回退把读不到的 `index.html`
当空字符串（`unwrap_or_default()`），于是所有无扩展名路径都返回 **200 + 空 body** ——
状态码一切正常、页面全白。排查时先 `curl -o /dev/null -w '%{size_download}'` 看首页字节数是否为 0。
### 三段分离：编译 / 搬运 / 启动

构建输出留在仓库 `dist/`（`build_frontend.sh` 的位置不变），**搬运到部署根是独立一步**（`make install`）——
「源码产物」与「部署产物」分开。同一份 `build` 产物因此有两类消费者：

| 路径 | 步骤 | 命令 |
|------|------|------|
| 部署 | `build` → `install` → `start` | `make build` / `make install` / `make prod`（= 三步 + 残留清理） |
| 打包 | `build` → 组装 tar.gz | `make package`（**不**碰本机部署根） |

- **`make build`**：只编译（`build_frontend.sh` + `cargo build --release`），产物留在仓库，不写部署根。
- **`make install`**：把 `target/release/ai_orz` 与 `dist/` 搬到部署根（幂等；覆盖运行中的二进制走 tmp+mv
  原子替换，见 `prod.sh::install_bin`）。仓库与发布包之外的路径都一致 —— 发布包解压后二进制本就在包根。
- **`start` 不自动搬运**：同机多 checkout 共享同一个部署根，若 `start` 自动从当前工作树搬运，会把
  「恰好在这个 checkout 编译过」的产物推上生产（可能反而是降级）。代价是 `make build && make restart`
  会跑旧产物，所以 `start` 检测到「部署根产物落后于仓库构建产物」时**明确告警**并提示 `make install`
  —— 既不静默，也不擅自搬运。
- **打包为什么不复用 `install`**：打包是纯离线动作。走 `install` 会让 CI 往 runner 的 `$HOME` 白写约 280MB
  产物，本机 `make package` 还会覆盖正在运行实例的 `bin/` 与 `dist/`。故 `package.sh` 只认
  `prod.sh build` 的产物（`target/release/ai_orz` + `dist/`），与部署根无关。

⚠️ 「落后」判定按 mtime 比较（二进制比二进制、`dist/index.html` 比 `dist/index.html`）；dx 每次构建都会
重写 `index.html`，该判定足够可靠。

**开发态刻意不走部署根**：`make dev` / `cargo run` / `cargo test` 继续落在仓库内 `.ai_orz`，
靠「相对路径 + 进程 CWD」天然隔离，各 checkout 与集成测试互不干扰。
⚠️ 因此 `make dev` 与 `make prod` 用的是**两份不同的数据**（历史上前者是共享同一份）：
想拿生产数据跑开发态，显式指过去 —— `AI_ORZ_BASE_PATH=~/.ai_orz/data make dev`。

### 环境变量（生产生命周期）

| 变量 | 作用 | 默认 |
|------|------|------|
| `AI_ORZ_DEPLOY_ROOT` | 覆盖部署根 | 见上表 |
| `AI_ORZ_BASE_PATH` | 覆盖数据目录（后端也读它，是全局唯一开关） | `<部署根>/data` |
| `AI_ORZ_BIN` | 覆盖服务二进制路径 | `<部署根>/bin/ai_orz` |
| `FRONTEND_DIST_DIR` | 覆盖前端静态目录（后端也读它） | `<部署根>/dist` |
| `AI_ORZ_LISTEN_ADDR` | 覆盖监听地址 | `0.0.0.0:3000` |
| `AI_ORZ_STOP_TIMEOUT` | 优雅停止超时秒数 | `30` |

⚠️ **不要把 `BASE_DATA_PATH`（后端常量 `.ai_orz`）的默认值改成全局 home**：
这个相对路径本身就是隔离机制。改成全局绝对路径后，同机所有实例（多个 checkout 的 dev、
不带 env 的 `cargo test`、生产实例）会共享同一份 SQLite / DuckDB / 向量库 ——
WAL 锁冲突、测试污染真实数据、bincode 元数据互相覆盖，且**全程静默**（无校验、无锁）。
要脱离仓库请走 `AI_ORZ_DEPLOY_ROOT`（只影响生产生命周期），**不要动默认值**。
