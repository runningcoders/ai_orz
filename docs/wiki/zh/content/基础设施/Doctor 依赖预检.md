# Doctor 依赖预检

> 🎯 **定位**：项目工具链依赖预检体系——新服务器一键检测/自动安装工具链
> 状态：v1.0 (2026-08-19)
> 触发场景：新环境初始化、CI 环境准备、本地开发环境诊断
>
> 关联文档：
> - 上层权威文档：[AGENTS.md](../../AGENTS.md) — 依赖管理约定
> - 横向文档：[基础设施](../基础设施.md)
> - 设计文档：[timestamp_convention.md](../../../design/timestamp_convention.md)
> - RAG 知识卡：[Doctor 依赖预检](../../../knowledge/zh/Doctor%20依赖预检/Doctor%20依赖预检.md)

## 一、概述

### 1.1 功能简介

Doctor 依赖预检是 AI Orz 项目的工具链健康检查体系，以 `scripts/check_deps.sh` 为核心脚本，配合 `Makefile` 的 `doctor` 目标和 `scripts/start.sh` 的启动前预检，形成「检测 → 修复 → 验证」的闭环。

### 1.2 核心价值

- **新环境零配置启动**：新服务器 clone 仓库后执行 `make doctor` 即可完成全链依赖检测
- **CI 环境一致性**：CI 管道使用同一脚本确保依赖一致性
- **故障快速定位**：依赖缺失时给出结构化诊断信息而非裸错误

## 二、架构设计

### 2.1 组件结构

```
scripts/check_deps.sh          # 核心预检脚本（检测 + 自动安装）
    │
    ├── Rust 工具链检测 (rustc/cargo/llvm)
    ├── 系统依赖检测 (protoc/sqlite/openssl)
    ├── 前端依赖检测 (node/npm)
    ├── 可选依赖检测 (ffmpeg/imagemagick)
    └── 版本兼容性检查
    │
scripts/start.sh                # 启动前自动触发预检
Makefile                        # make doctor 入口
```

### 2.2 检测模式

| 模式 | 触发方式 | 行为 |
|------|---------|------|
| 手动检测 | `make doctor` | 执行全量检测，输出报告 |
| 启动前检测 | `scripts/start.sh` | 仅检测，不自动安装 |
| CI 检测 | CI 管道 | 严格模式，缺失即失败 |

## 三、依赖检测清单

### 3.1 Rust 工具链

| 依赖 | 最低版本 | 自动安装 |
|------|---------|---------|
| rustc | 1.80+ | rustup |
| cargo | 最新 | rustup |
| llvm | 16+ | brew/apt |

### 3.2 系统依赖

| 依赖 | 用途 | 自动安装 |
|------|------|---------|
| protoc | protobuf 编译 | brew/apt |
| sqlite3 | 数据库 | brew/apt |
| openssl | 加密 | brew/apt |

### 3.3 前端依赖

| 依赖 | 最低版本 | 自动安装 |
|------|---------|---------|
| node | 20+ | nvm/brew |
| npm | 10+ | 随 node |

## 四、使用指南

### 4.1 手动检测

```bash
# 完整检测（含自动安装）
make doctor

# 仅检测不安装
make doctor-check
```

### 4.2 启动时自动检测

```bash
# 启动脚本会自动触发预检
./scripts/start.sh
```

### 4.3 CI 集成

```yaml
# .github/workflows/rust.yml
- uses: actions/run-doctor
  with:
    strict: true
```

## 五、故障排查

### 5.1 常见问题

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| protoc 未找到 | 未安装 protobuf-compiler | `brew install protobuf` |
| sqlite3 未找到 | 系统未装 sqlite | `brew install sqlite` |
| node 版本过低 | 版本 < 20 | `nvm install 20` |
| cargo check 失败 | 依赖未安装 | 运行 `make doctor` |

### 5.2 调试

```bash
# 查看详细检测日志
make doctor VERBOSE=1

# 跳过自动安装
make doctor AUTO_INSTALL=false
```

## 六、扩展指南

### 6.1 添加新依赖

编辑 `scripts/check_deps.sh`：

```bash
check_dependency() {
    local name="$1"
    local version_cmd="$2"
    local min_version="$3"
    local install_cmd="$4"
    
    # ...
}
```

### 6.2 CI 集成

在 `.github/workflows/rust.yml` 中添加 doctor 步骤：

```yaml
- name: Doctor Check
  run: make doctor-check
```

## 七、最佳实践

1. **新环境必须先跑 doctor**：clone 仓库后第一步执行 `make doctor`
2. **CI 使用严格模式**：CI 环境禁止自动安装，缺失即失败
3. **版本兼容性检测**：doctor 会检测版本兼容性，不只是存在性
4. **定期更新检测清单**：新增依赖时同步更新 `check_deps.sh`

## 八、故障排查指南

### 8.1 依赖检测失败

| 错误信息 | 可能原因 | 建议操作 |
|---------|---------|---------|
| `rustc not found` | Rust 工具链未安装 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| `protoc not found` | protobuf 编译器未装 | `brew install protobuf` |
| `node too old` | Node.js 版本过低 | `nvm install 20 && nvm use 20` |
| `permission denied` | 缺少安装权限 | 使用 `sudo` 或修改安装方式 |

### 8.2 启动脚本集成问题

- 确认 `scripts/start.sh` 有执行权限
- 检查 `PATH` 环境变量是否包含工具链路径
- CI 环境中需显式设置 `RUSTUP_HOME` 和 `CARGO_HOME`

## 九、参考

- [脚本入口](../../../../scripts/check_deps.sh)
- [Makefile 目标](../../../../Makefile)
- [启动脚本](../../../../scripts/start.sh)
- [RAG 知识卡](../../../../knowledge/zh/Doctor%20依赖预检/Doctor%20依赖预检.md)

## 十、附录

### 10.1 版本历史

| 版本 | 日期 | 变更说明 |
|------|------|---------|
| v1.0 | 2026-08-19 | 初始版本，支持 Rust/系统/前端三大类依赖检测 |

### 10.2 相关文件

- `scripts/check_deps.sh` — 核心检测脚本
- `Makefile` — `doctor` / `doctor-check` 目标
- `scripts/start.sh` — 启动前预检集成
