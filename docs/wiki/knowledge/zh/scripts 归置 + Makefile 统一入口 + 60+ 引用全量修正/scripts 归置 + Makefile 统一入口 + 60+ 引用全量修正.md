---
kind: wiki_knowledge_card
name: scripts/ 归置 + Makefile 统一入口 + 60+ 引用全量修正
category: 构建发布启动脚本
scope:
  - "Makefile"
  - "scripts/start.sh"
  - "scripts/build.sh"
  - "scripts/build_frontend.sh"
  - "scripts/test.sh"
  - "scripts/tools/**"
  - "docs/wiki/**/*.md"
  - ".github/workflows/*.yml"
source_files:
  - Makefile#L1-L200
  - scripts/start.sh#L1-L120
  - scripts/build.sh#L1-L140
  - scripts/build_frontend.sh#L1-L120
  - scripts/test.sh#L1-L100
  - scripts/tools/docs_lint.sh#L1-L60
  - scripts/tools/docs_migrate.sh#L1-L60
  - scripts/tools/coverage.sh#L1-L80
  - .github/workflows/ci.yml#L1-L200
  - .github/workflows/docs.yml#L1-L150
  - docs/archive/design-archive/build_and_deployment_workflow_design.md
  - docs/wiki/zh/content/基础设施/持续集成与发布工作流.md
  - docs/wiki/knowledge/zh/构建流水线：cargo 构建 + dioxus 前端打包 + docker 镜像 + GitHub Actions CI/构建流水线：cargo 构建 + dioxus 前端打包 + docker 镜像 + GitHub Actions CI.md
---

# scripts/ 归置 + Makefile 统一入口 + 60+ 引用全量修正

## §1 整体方案

7395ce18（合并 415feca1/b38999ea 两次提交）落地构建脚本的**目录规范 + 入口统一 + 全仓引用修正**三件套：

1. **scripts/ 归置**：之前散落在仓库根目录 `start.sh` / `build-frontend.sh` / `setup-dev.sh` / `coverage.sh` / `tools/*.py`（根目录 12 个 shell 脚本 + docs 根目录 3 个、ai-orz-macros/ 下 2 个独立脚本乱七八糟）→ 全部**搬迁到 scripts/ 目录下**，子目录结构：`scripts/start.sh` / `scripts/build.sh` / `scripts/build_frontend.sh` / `scripts/test.sh` / `scripts/dev/`（开发工具：setup-dev.sh、install-deps.sh）/ `scripts/tools/`（辅助工具：docs_lint.sh / docs_migrate.sh / coverage.sh / sqlite-backup.sh）。根目录只留 `Makefile` / `Cargo.toml` / `.env.example` / `README.md` 一级文件，脚本全部收敛进 scripts/。
2. **Makefile 统一入口**：仓库根 `Makefile` 定义所有开发/构建常用命令为统一 make target（`make run` / `make build` / `make build-frontend` / `make test` / `make test-backend` / `make test-frontend` / `make fmt` / `make clippy` / `make coverage` / `make ci-docs-lint` / `make docker-build` 等 24 个 target）；每个 target 内部只做一件事 = **调用对应 scripts/*.sh 脚本**（如 `make run: @bash scripts/start.sh`），Makefile 本身不写复杂命令逻辑（长流程易读、易脚本化）。
3. **60+ 引用全量修正**：搬迁+统一入口后，全仓 60+ 处对旧脚本路径/命令的引用批量修正：① 前端 README 中 `./build-frontend.sh` → `make build-frontend`；② CI workflow（ci.yml / docs.yml）中 bash 调用：`bash scripts/build_frontend.sh`（路径修正）、`bash docs_lint.sh` → `bash scripts/tools/docs_lint.sh`、`make docs-lint` → `make ci-docs-lint` 名称对齐；③ 353 篇 Wiki 长文中引用启动命令（「本地开发请运行 `./start.sh`」→「请运行 `make run`」或 `bash scripts/start.sh`）④ 8 大板块设计文档中 shell 命令统一替换；⑤ .env.example / docker-compose.yml 中 `command: ./start.sh` → `command: ["bash", "scripts/start.sh"]`。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [Makefile](Makefile) | 【7395ce18 核心 1】统一开发/构建命令入口（根目录唯一命令入口）| 24 个 make target：① 运行类（run run-backend run-frontend-dev）② 构建类（build build-backend build-frontend docker-build docker-push）③ 测试类（test test-backend test-frontend test-integration coverage）④ 质量类（fmt clippy ci-docs-lint ci-docs-migrate-dry ci-clippy-full）⑤ 工具类（db-migrate db-backup setup-dev clean）；每个 target recipe 简单 = @bash scripts/xxx.sh 或短命令（如 fmt = cargo fmt --all）；`.PHONY` 声明全部 24 个 target（防止磁盘上同名文件干扰）|
| [scripts/start.sh](scripts/start.sh) | 【7395ce18 核心 2】统一启动脚本（取代之前根目录 start.sh）| 功能：① 检查 AI_ORZ_HOME env（没设置用 default ~/.ai_orz/）② `cargo build --release` 如果 `backend/target/release/ai-orz-server` 不存在或 flag --rebuild 传了 ③ 读取 `.env`（没 .env 就从 .env.example 复制一份模板+提示用户填）④ 启动后端 `backend/target/release/ai-orz-server` + 前端静态文件服务（如果 build-frontend 产物在 frontend/dist/ 存在就 serve，否则只启动 REST API 5000 端口）；兼容开发/生产模式：DEV=1 环境变量走 `cargo run` 而不是 release bin。|
| [scripts/build.sh](scripts/build.sh) | 后端 + 前端同时构建的总入口（Makefile make build 调用它）| 内部：`bash scripts/build_backend.sh`（cargo build --release --workspace）→ `bash scripts/build_frontend.sh`（dioxus build + npm install / tailwind v4 编译）→ 结束后输出产物大小摘要：backend binary size + frontend dist gzip size，便于 CI 监控回归；超时：前端 build 20 分钟硬超时（防止 npm install 挂死 CI）。|
| [scripts/build_frontend.sh](scripts/build_frontend.sh) | 前端独立构建脚本 | ① cd frontend/ ② build.rs 触发前先 `npm install`（Tailwind CSS v4 requires、daisyUI v5）③ `dioxus build --release`（Dioxus CLI，生成 frontend/dist/index.html + .wasm + .js）④ 拷贝 dist 到 backend/static/（后端 serve 静态文件的目录）；NODE_ENV=production 时构建产物启用 wasm-opt（binaryen）优化 size，DEV 时跳 optimize 省时间。|
| [scripts/test.sh](scripts/test.sh) | 全仓测试统一入口（make test 调用）| 按模块拆分：`cargo test --workspace --exclude frontend --exclude ai-orz-macros-internal-test`（后端 984 个测试）→ `cd frontend && dioxus test`（前端 82 个）→ 覆盖率门槛校验：`bash scripts/tools/coverage.sh --check-thresholds`（PR 分支 ≥38%，main 分支 ≥45%，不达标 exit 1）；--quick 模式跳过 coverage + e2e，本地开发迭代快。|
| [scripts/tools/docs_lint.sh](scripts/tools/docs_lint.sh) | docs_lint 调用封装 | `cargo run -p docs_lint -- --root $REPO_ROOT --rules all`；自动找到 REPO_ROOT（相对脚本位置 ../..），CI 中相对路径和本地调用结果一致；--json 输出便于 CI 收集。|
| [.github/workflows/ci.yml](.github/workflows/ci.yml) | GitHub Actions CI 主流程 | 全量 build + test + clippy；【7395ce18 核心修正】CI 中脚本路径全部对齐 scripts/：`- run: bash scripts/test.sh`（旧 `- run: bash test.sh` 会 file not found）；`- run: make clippy`（CI 也通过 Makefile 调用，与本地命令一致，防止 CI 命令和本地命令漂移）。|
| [.github/workflows/docs.yml](.github/workflows/docs.yml) | Docs CI 门禁 | `make ci-docs-lint`（调用 docs_lint.sh）+ docs_migrate --dry-run（确认迁移后 diff=0，防止 PR 引入遗留旧格式没 migrate）。|
| 【Level4 总卡】构建流水线：cargo 构建 + dioxus 前端打包 + docker 镜像 + GitHub Actions CI | 本卡是总卡的 scripts/ 归置细粒度拆卡 | [构建流水线总卡](docs/wiki/knowledge/zh/构建流水线：cargo%20构建%20+%20dioxus%20前端打包%20+%20docker%20镜像%20+%20GitHub%20Actions%20CI/构建流水线：cargo%20构建%20+%20dioxus%20前端打包%20+%20docker%20镜像%20+%20GitHub%20Actions%20CI.md) |
| 【Wiki 长文】持续集成与发布工作流.md | 系统化上下文 + §8 Troubleshooting | [持续集成与发布工作流](docs/wiki/zh/content/基础设施/持续集成与发布工作流.md) |
| 【① Design】build_and_deployment_workflow_design.md | 构建部署设计决策 | [docs/archive/design-archive/build_and_deployment_workflow_design.md](docs/archive/design-archive/build_and_deployment_workflow_design.md) |

## §3 架构约定

本卡为 [构建流水线总卡](docs/wiki/knowledge/zh/构建流水线：cargo%20构建%20+%20dioxus%20前端打包%20+%20docker%20镜像%20+%20GitHub%20Actions%20CI/构建流水线：cargo%20构建%20+%20dioxus%20前端打包%20+%20docker%20镜像%20+%20GitHub%20Actions%20CI.md) 描述的**构建流水线体系**中**脚本目录规范 + Makefile 统一入口**模块的细粒度独立召回卡；按 AGENTS §2.1.3 Level 4 保留。

1. **Makefile = 薄包装层（不承载业务逻辑）**：Makefile 每个 target 的 recipe ≤ 3 行 shell，复杂的构建逻辑（env 检查、fallback、超时、多步流水线）**必须全部抽到 scripts/*.sh 中实现**。原因：Makefile 的函数、条件语法极其晦涩（ifneq/foreach 等），20+ 行长 recipe 可读性差；shell 脚本易读易调试（bash -x scripts/start.sh 能单步）；未来换构建工具（Earthfile / just 命令 runner）时只需改 Makefile 一层薄 target，scripts/*.sh 全复用。
2. **脚本引用路径 = 相对脚本自身位置（$0），禁止 PWD 依赖**：所有 scripts/*.sh 开头必写 `REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`（$0 取脚本自身所在目录 → 向上 1 层到 repo 根）——从此路径拼 `$REPO_ROOT/scripts/tools/docs_lint.sh` 等其他脚本或 Cargo.toml；**禁止**用 `$(pwd)/scripts/`（如果用户 cd 到子目录再 `bash ../scripts/start.sh`，PWD 是子目录导致路径全错）。
3. **所有脚本 Bash Strict Mode（非兼容 POSIX）**：scripts/*.sh 首 2 行强制 `#!/usr/bin/env bash` + `set -euo pipefail`（-e 命令非 0 立即退出 / -u 未定义变量报错 / -o pipefail 管道中任一失败才算失败）。防止 `cd some_not_exist_dir && rm -rf *`（无 -e 会 cd 失败继续 rm 根目录！）；例外：scripts/tools/*.sh 中 `grep "pattern" file || true`（grep 没找到 match 返回 non-zero，但不是错误，明确 || true 白名单放行）。
4. **CI 命令 & 本地命令统一入口 Makefile 对称**：CI workflow 中 `run: make ci-docs-lint`、`run: make clippy`、`run: make test` → 本地开发者相同命令 `make ci-docs-lint` / `make clippy` / `make test` 100% 相同结果。禁止 CI 中 `run: cargo test --workspace --exclude xxx ...`（超长参数手工写）而本地开发者习惯 `make test` —— 两者参数不一致 = CI fail 但本地过，或反过来；Makefile target = 命令唯一事实源（AGENTS §2.1.1 路径/命令单一事实源原则对齐）。
5. **脚本目录 3 层子结构 = 固定约束，不再扁平**：`scripts/` 根 = 顶层 5 个高频命令（start/build/test/build_frontend/dev_entrypoint）、`scripts/dev/` = 低频 setup 类（setup-dev.sh install-deps.sh install-wasm-target.sh）、`scripts/tools/` = 辅助/门禁/运维类（docs_lint.sh docs_migrate.sh coverage.sh sqlite-backup.sh db-migrate.sh）。未来新增脚本先决定属于哪一层，再放对目录——禁止回到根目录扔 `*.sh`（违反 scripts/ 归置的初衷）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 Makefile target 内 recipe 长于 3 行（注释/空行不计，但有效 shell 代码 ≤ 3 行）**：CI grep 检查；超过 3 行的有效逻辑强制抽到 scripts/*.sh 内，Makefile 只调 shell。
2. ❌ **禁止仓库根目录除 Makefile / Cargo.toml / 配置文件（.env.example / .gitignore / README.md / AGENTS.md / Dockerfile / docker-compose.yml / rust-toolchain.toml / rustfmt.toml / clippy.toml）外有任何 .sh / .py 可执行脚本**：`ls -1 $REPO_ROOT/*.sh $REPO_ROOT/*.py 2>/dev/null | wc -l` 必须 = 0。任何功能脚本必须在 `scripts/` 下 3 层子目录之一。
3. ✅ **强制 60+ 引用修正后 0 遗留测试（全仓 grep）**：全仓 grep「旧脚本路径」模式矩阵：① `grep -r "\./start\.sh" docs/ frontend/ README.md` → 0 命中（应该 `bash scripts/start.sh` 或 `make run`）② `grep -r "bash build-frontend\.sh"` → 0（应 `make build-frontend` 或 scripts/build_frontend.sh）③ `grep -r "root-level scripts\|/setup-dev\.sh"` 非 scripts/ 目录下 → 0 命中。3 类全 0 才算修正到位。
4. ✅ **强制 Bash Strict Mode 矩阵 12 条全过**：12 条覆盖：① `set -euo pipefail` 后遇到未定义变量 `echo $NOT_DEFINED` → exit non-zero ② `cd non_exist && rm ...` 不执行 rm（因 cd fail -e 立即退出）③ `grep no_match file | sort`（grep no_match non-zero → pipefail = non-zero exit）④ 例外 `|| true` 正确放行。12 条全命中预期行为。
5. ✅ **强制 Makefile 24 target 功能冒烟测试（make -n 干跑）**：`for t in run build build-frontend test coverage fmt clippy ci-docs-lint ... 全 24 个` → `make -n $t`（--dry-run 只打印，不执行）→ exit code = 0（无语法错 / `*** missing separator` 等 Makefile 格式错）；且输出中包含对应 `scripts/*.sh` 的调用字符串（证明 target 真指向正确脚本，不是空 target）。
6. ❌ **禁止脚本中硬编码本机绝对路径**：任何 scripts/*.sh 中禁止出现 `/Users/aman/...`（本用户机器路径）、`/home/ci-runner/...` 等绝对路径——必须是 $HOME 或 $REPO_ROOT + 相对。`grep "/Users/aman" scripts/ -r` = 0 命中；CI 环境路径 CI_HOME 用 env var 注入。
7. ✅ **四类互引闭环**：本卡 source_files[] 含 Wiki 长文 1 篇（持续集成与发布工作流）+ Level 4 构建流水线总卡 + 1 Design（真实文件）；持续集成与发布工作流长文 cite 段回链本卡 + 总卡 + Design。
8. ✅ **REPO_ROOT 路径自愈验证**：模拟 3 种 PWD 场景调用脚本：① cd repo_root → `bash scripts/start.sh --help`（正常）② cd repo_root/scripts → `bash start.sh --help`（正常）③ cd /tmp → `bash /abs/path/to/scripts/start.sh --help`（正常）。3 种都能正确找到 REPO_ROOT（$0 自愈）且 `echo $REPO_ROOT` 输出 = 真实仓库根。
