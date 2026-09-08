# AI Orz 开发常用命令汇总
# 用法：make <命令>（make 或 make help 查看全部）
# 所有命令与 .github/workflows/rust.yml CI 门禁严格对齐，本地过了 CI 就过
# 日常自测：make lint（纯静态检查，前后端全量，不跑测试）
# 提交/推送前：make ci（= lint + 全量测试；与 pre-push 钩子同口径）

# 每条命令执行前自动补充标准 PATH（覆盖受限 shell 环境，rustup 在 ~/.cargo/bin）
export PATH := $(HOME)/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$(PATH)

.DEFAULT_GOAL := help
.PHONY: help fmt fmt-check clippy clippy-fe docs-lint docs-migrate lint test test-be test-fe ci coverage e2e dev build build-fe prod package serve run clean clean-slim clean-proc doctor hooks

# git hooks 目录指向仓库内 .githooks/
#   - pre-commit：fmt-check（cargo fmt --all -- --check，秒级）
#   - pre-push ：fmt-check + clippy（workspace 口径）+ clippy-fe（前端 wasm32 真编译门禁）
# 跳过某次：git commit/push --no-verify
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

# --workspace 不可省：根 Cargo.toml 带 [package]，cargo 默认只选 default member
#（= 根包 ai_orz），common / tools / ai-orz-macros 的 lint 会被静默跳过。
# frontend 由 clippy-fe 以 wasm32 口径全量覆盖（实际运行目标），此处排除以免 native 重复编译。
clippy: ## clippy -D warnings（CI lint job 口径，需 protoc）
	cargo clippy --workspace --exclude frontend --all-targets -- -D warnings

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

# 同 clippy：必须 --workspace，否则 common / tools / ai-orz-macros 的测试根本不会被执行
#（验证过：裸 `cargo test --lib` 只跑 ai_orz 一个二进制，common 的 200+ 单测全被跳过）。
# frontend 由 test-fe 单独跑。
test-be: ## 后端与共享 crate 测试：单元 + 集成（CI backend job 口径）
	cargo test --workspace --exclude frontend --lib
	cargo test --workspace --exclude frontend --test '*'

test-fe: ## 前端测试（CI frontend job 口径）
	cd frontend && cargo test

# ===== 聚合门禁 =====

# 纯静态检查（不跑测试，快于 ci）：前后端全量覆盖
#   fmt-check → workspace 全 crate 格式；clippy → ai_orz + common + tools + macros；
#   clippy-fe → frontend（wasm32 口径，含完整类型检查）；docs-lint → 文档链接规范
lint: fmt-check clippy clippy-fe docs-lint ## 全部静态检查（前后端）：fmt + clippy + clippy-fe + docs-lint

ci: lint test ## 本地模拟 CI 全部门禁（= lint + 全量测试，不含 coverage）

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

# ===== 依赖治理 =====

doctor: ## 依赖预检：MODE 指定模式（默认 dev），FIX=1 自动安装可自动项
	./scripts/check_deps.sh $(MODE) $(if $(FIX),--fix)

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

package: ## 编译并打包正式发布物（tar.gz：二进制 + dist/ + start.sh 启动脚本 + README，可指定 VERSION）
	./scripts/package.sh $(VERSION)

e2e: ## Playwright E2E（仅本地，已移出 CI）
	cd tests/e2e && npx playwright test
