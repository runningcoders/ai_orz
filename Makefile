# AI Orz 开发常用命令汇总
# 用法：make <命令>（make 或 make help 查看全部）
#
# 设计原则：Makefile 只做「命令名 → 脚本」的转发，不内联任何实现
#   - 运行 / 构建 / 发布 → scripts/ai_orz.sh（统一入口，再分发到 run.sh / prod.sh / ...）
#   - 门禁（fmt/clippy/test/ci）→ scripts/check.sh（与 .githooks 同口径，不会漂移）
# 这样做的好处：同一功能只有一处实现，改行为只需改脚本，不用在 Makefile、钩子、CI 三处同步。
# 日常自测：make lint（纯静态检查，前后端全量，不跑测试）
# 提交/推送前：make ci（= lint + 全量测试；与 pre-push 钩子同口径）

# 每条命令执行前自动补充标准 PATH（覆盖受限 shell 环境，rustup 在 ~/.cargo/bin）
export PATH := $(HOME)/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$(PATH)

# 覆盖率红线（CI：push main = 45，PR = 38；本地默认 45）
FAIL_UNDER ?= 45

.DEFAULT_GOAL := help
.PHONY: help fmt fmt-check clippy clippy-fe docs-lint docs-migrate lint test test-be test-fe ci coverage e2e \
        dev serve run build build-fe prod prod-stop stop prod-status status prod-log logs restart \
        clean-proc clean clean-slim doctor package hooks

# git hooks 目录指向仓库内 .githooks/
#   - pre-commit：fmt-check（cargo fmt --all -- --check，秒级）
#   - pre-push ：fmt-check + clippy（workspace 口径）+ clippy-fe（前端 wasm32 真编译门禁）+ dx check
# 两者都转发到 scripts/check.sh，与 CI 同口径；跳过某次：git commit/push --no-verify
hooks:
	git config core.hooksPath .githooks

help: ## 显示本帮助
	@echo "AI Orz 开发命令（详细说明见文件头注释）："
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

# ===== 格式化 =====

fmt: ## 全仓格式化（根 workspace 单命令覆盖全部 5 个 crate）
	./scripts/check.sh fmt

fmt-check: ## 格式检查（CI fmt job 口径）
	./scripts/check.sh fmt-check

# ===== 静态检查 =====

clippy: ## clippy -D warnings（CI lint job 口径，需 protoc）
	./scripts/check.sh clippy

clippy-fe: ## 前端 wasm32 clippy（CI frontend job 口径）
	./scripts/check.sh clippy-fe

docs-lint: ## 文档链接规范门禁：file:// 伪协议/绝对路径/冒号行号（AGENTS §2.1.2）
	./scripts/check.sh docs-lint

docs-migrate: ## 文档链接批量迁移，默认 dry-run；写盘加 APPLY=1
	APPLY=$(APPLY) ./scripts/check.sh docs-migrate

# ===== 测试 =====

test: ## 全量测试（后端 + 前端）
	./scripts/check.sh test

test-be: ## 后端与共享 crate 测试：单元 + 集成（CI backend job 口径）
	./scripts/check.sh test-be

test-fe: ## 前端测试（CI frontend job 口径）
	./scripts/check.sh test-fe

# ===== 聚合门禁 =====

lint: ## 全部静态检查（前后端）：fmt + clippy + clippy-fe + docs-lint
	./scripts/check.sh lint

ci: ## 本地模拟 CI 全部门禁（= lint + 全量测试，不含 coverage）
	./scripts/check.sh ci

coverage: ## 覆盖率门禁，FAIL_UNDER 默认 45（PR 口径 38：make coverage FAIL_UNDER=38）
	FAIL_UNDER=$(FAIL_UNDER) ./scripts/check.sh coverage

# ===== 运行 / 构建（全部路由到 scripts/ai_orz.sh，实现只有一处）=====

dev: ## 开发模式：后端 cargo run + 前端 dx serve 双服务
	./scripts/ai_orz.sh dev

serve: ## 仅启动前端开发服务器（dx serve，http://localhost:8080）
	./scripts/ai_orz.sh frontend

run: ## 仅启动后端开发服务器（cargo run，http://localhost:3000）
	./scripts/ai_orz.sh backend

build: ## 全量 release 编译：前端 dist/ + 后端二进制（= CI release 口径）
	./scripts/ai_orz.sh build

build-fe: ## 仅编译前端 release 并复制产物到 dist/
	./scripts/ai_orz.sh build-fe

prod: ## 生产模式：构建 + 后台运行 release 二进制（0.0.0.0:3000，连跑两次 = 幂等重启）
	./scripts/ai_orz.sh prod

stop: ## 停止后台生产服务（仅 release 二进制，不影响开发态进程）
	./scripts/ai_orz.sh stop

prod-stop: ## 停止后台生产服务（stop 的兼容别名）
	$(MAKE) stop

restart: ## 重启后台生产服务（优雅停止后启动，不重新构建）
	./scripts/ai_orz.sh restart

status: ## 查看后台生产服务状态（PID / 运行时长 / 资源占用 / 监听端口）
	./scripts/ai_orz.sh status

prod-status: ## 查看生产服务状态（status 的兼容别名）
	$(MAKE) status

logs: ## 实时跟踪生产日志（tail -F，自动跟随按日滚动）
	./scripts/ai_orz.sh logs

prod-log: ## 实时跟踪生产日志（logs 的兼容别名）
	$(MAKE) logs

# ===== 治理 =====

clean-proc: ## 清理残留死进程（后端/dx/端口占用；dev 启动前也会自动执行）
	./scripts/ai_orz.sh clean

doctor: ## 依赖预检：MODE 指定模式（默认 dev），FIX=1 自动安装可自动项
	./scripts/ai_orz.sh doctor $(MODE) $(if $(FIX),--fix)

# ===== 发布 =====

package: ## 编译并打包正式发布物（tar.gz：二进制 + dist/ + 运维脚本 + Makefile + README，可指定 VERSION）
	./scripts/ai_orz.sh package $(VERSION)

# ===== 磁盘治理 =====

clean-slim: ## 瘦身 target：清增量缓存+陈旧快照，保留依赖缓存（下次编译仅几十秒）
	@echo "== 当前 target 体积 =="
	@du -sh target frontend/target 2>/dev/null || true
	@echo ""
	@echo "== 清理增量编译缓存（膨胀主因，可安全删除）=="
	rm -rf target/debug/incremental target/wasm32-unknown-unknown/debug/incremental \
		target/wasm32-unknown-unknown/release/incremental 2>/dev/null || true
	find target -name "*.fingerprint" -type d -name "incremental" -exec rm -rf {} + 2>/dev/null || true
	@echo "== 清理 30 天未访问的陈旧 deps 快照（带哈希后缀的旧版本）=="
	find target/debug/deps target/wasm32-unknown-unknown -name "*-[0-9a-f]\{16\}.*" -atime +30 -delete 2>/dev/null || true
	@echo ""
	@echo "== 清理后体积 =="
	@du -sh target frontend/target 2>/dev/null || true

clean: ## 全量清理 target（下次编译为完整冷构建，慎用）
	cargo clean
	cd frontend && cargo clean

# ===== E2E =====

e2e: ## Playwright E2E（仅本地，已移出 CI）
	./scripts/check.sh e2e
