# AI Orz 开发常用命令汇总
# 用法：make <命令>（make 或 make help 查看全部）
# 所有命令与 .github/workflows/rust.yml CI 门禁严格对齐，本地过了 CI 就过

# 每条命令执行前自动补充标准 PATH（覆盖受限 shell 环境，rustup 在 ~/.cargo/bin）
export PATH := $(HOME)/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$(PATH)

.DEFAULT_GOAL := help
.PHONY: help fmt fmt-check clippy clippy-fe docs-lint docs-migrate test test-be test-fe ci coverage e2e dev build build-fe prod serve run clean clean-slim clean-proc hooks

# git hooks 目录指向仓库内 .githooks/（pre-push 自动跑 fmt-check，跳过用 git push --no-verify）
hooks:
	git config core.hooksPath .githooks

help: ## 显示本帮助
	@echo "AI Orz 开发命令（详细说明见文件头注释）："
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

# ===== 格式化 =====

fmt: ## 全仓格式化（根 workspace 单命令覆盖全部 5 个 crate）
	cargo fmt --all

fmt-check: ## 格式检查（CI fmt job 口径）
	cargo fmt --all -- --check

# ===== 静态检查 =====

clippy: ## 后端 clippy，-D warnings（CI lint job 口径，需 protoc）
	cargo clippy --all-targets -- -D warnings

clippy-fe: ## 前端 wasm32 clippy（CI frontend job 口径）
	cd frontend && cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings

docs-lint: ## 文档链接规范门禁：file:// 伪协议/绝对路径/冒号行号（AGENTS §2.1.2）
	cargo run -p ai-orz-tools --bin docs_lint

docs-migrate: ## 文档链接批量迁移，默认 dry-run；写盘加 APPLY=1
	@if [ "$(APPLY)" = "1" ]; then \
		echo "== APPLY 模式：写盘 =="; \
		cargo run -p ai-orz-tools --bin docs_migrate -- --apply; \
	else \
		echo "== dry-run 模式（预览不写盘；确认后 make docs-migrate APPLY=1）=="; \
		cargo run -p ai-orz-tools --bin docs_migrate; \
	fi

# ===== 测试 =====

test: test-be test-fe ## 全量测试（后端 + 前端）

test-be: ## 后端测试：单元 + 集成（CI backend job 口径）
	cargo test --lib
	cargo test --test '*'

test-fe: ## 前端测试（CI frontend job 口径）
	cd frontend && cargo test

# ===== 聚合门禁 =====

ci: fmt-check clippy clippy-fe docs-lint test ## 本地模拟 CI 全部门禁（不含 coverage）

# 覆盖率（需 cargo-llvm-cov；main 口径 45，PR 口径 38 可 FAIL_UNDER=38）
coverage: ## 覆盖率门禁，FAIL_UNDER 默认 45
	cargo llvm-cov --workspace --tests --no-clean --no-fail-fast \
		--ignore-filename-regex "(tests/common/|/cargo/registry/|/rustc/|build.rs|target/)"
	cargo llvm-cov report \
		--ignore-filename-regex "(tests/common/|/cargo/registry/|/rustc/|build.rs|target/)" \
		--fail-under-lines $(FAIL_UNDER)

# ===== 进程治理 =====

clean-proc: ## 清理残留死进程（后端/dx/端口占用；start.sh 启动前也会自动执行）
	./scripts/cleanup.sh

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

# ===== 运行 / 编译（路由到 scripts/ 下脚本，逻辑只有一处）=====

dev: ## 开发模式：后端 cargo run + 前端 dx serve 双服务
	./scripts/start.sh dev

serve: ## 仅启动前端开发服务器（路由 scripts/start.sh frontend）
	./scripts/start.sh frontend

run: ## 仅启动后端开发服务器（路由 scripts/start.sh backend）
	./scripts/start.sh backend

build: ## 全量 release 编译：前端 dist/ + 后端二进制（= CI release 口径）
	./scripts/start.sh build

build-fe: ## 仅编译前端 release 并复制产物到 dist/（路由 scripts/build_frontend.sh）
	./scripts/build_frontend.sh

prod: ## 生产模式：编译 release 并运行生产二进制（0.0.0.0:3000）
	./scripts/start.sh prod

e2e: ## Playwright E2E（仅本地，已移出 CI）
	cd e2e && npx playwright test
