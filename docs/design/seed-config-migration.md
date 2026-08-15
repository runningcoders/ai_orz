# Seed 配置迁移中心

> 🎯 **本文档定位**：业务实体配置导出/导入/diff 系统设计——仅配置层不含运行时数据、4 种导入策略、敏感字段占位与字段级 diff
> 状态：v1.0（2026-08-15 整理）
> 查阅场景：新增 Seed 导入实体类型、排查导入冲突与幂等、理解配置迁移与全量备份边界时打开；具体导入导出实现看 src/service/domain/seed/
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构（Seed 是纯工具箱 Domain）
> - [attachment_storage.md](./attachment_storage.md) — 产物与附件统一存储设计（导出包的文件承载）

## 概述

业务实体配置的导出/导入/diff 系统，区别于全量备份：
- 仅含配置层（Org/User/Provider/Agent/Skill 定义）
- 不含运行时数据（消息、任务、stats、日志、向量索引）
- 敏感字段（password/api_key）使用占位符，导入时由管理员填写
- 支持 4 种导入策略 + 字段级 diff

## 架构原则

**核心原则**：seed 模块是"纯工具箱"，不持有任何 DAL 引用，不调用其他 domain。

| 层级 | 职责 |
|------|------|
| Domain | 只处理自己的业务逻辑，不调用其他 domain |
| Handler/Consumer | 跨 domain 编排，调用各 domain 完成 CRUD |
| Seed 子模块 | 提供数据视图（snapshot 结构 + diff 算法 + 文件存储），不调用任何 domain |

### 为什么这样设计？

1. **架构原则一致性**：项目硬约束"DAL layer must not call other DALs; business orchestration must be moved to consumer layer"在 domain 层面同样适用——domain 不应调用其他 domain
2. **可测试性**：seed 的纯函数（diff、validate、resolve）可独立单元测试，无需 DB
3. **职责清晰**：seed 关注"数据视图"，handler 关注"编排执行"
4. **复用性**：seed 的算法可被任何 handler 复用（不限于 system domain）

## 模块结构

```
src/service/domain/system/seed/        # 纯工具箱子模块
├── defs.rs                            # SeedSnapshot + XxxDef + ImportStrategy + SeedDiff 结构
├── diff.rs                            # diff_snapshots + validate_sensitive_fields + resolve_password/api_key 纯函数
├── store.rs                           # 文件系统 CRUD（CRUD + 路径穿越防护）
├── default.rs + default.json          # 编译期内置默认模板
└── seed_test.rs                       # 纯函数单元测试

src/handlers/system/seed/              # HTTP Handler + 跨 domain 编排
├── mod.rs                             # assemble_snapshot_from_db + apply_snapshot_to_db 编排函数
├── list.rs                            # GET /seed/list
├── get_file.rs                        # GET /seed/file/{name}
├── save.rs                            # POST /seed/save
├── load.rs                            # POST /seed/load/{name}
├── delete_file.rs                     # DELETE /seed/file/{name}
├── diff.rs                            # POST /seed/diff/{name}
├── diff_files.rs                      # POST /seed/diff-files
├── get_default.rs                     # GET /seed/default
└── apply_default.rs                   # POST /seed/apply-default
```

## 使用场景

1. **开箱即用初始化**：`POST /seed/apply-default` 一键应用默认模板
2. **配置版本管理**：`POST /seed/save` 导出 → git 提交 → diff 跟踪
3. **跨环境迁移**：导出 → 切换环境 → 导入（RegenerateIds 策略）
4. **配置回滚**：导入旧版本快照（PreserveIds 策略，运行时数据保留）

## 占位符语义

| 占位符 | 含义 | 解析方式 |
|--------|------|---------|
| `PENDING_INPUT` | 导入时强制要求管理员填写 | handler 从 `sensitive_values` map 取值 |
| `INHERIT_CURRENT` | 保留 DB 当前值（回滚场景） | handler 调用 domain 拉 DB 当前值传入 `resolve_*` 纯函数 |
| `RANDOM_GENERATE` | 随机生成并显示一次 | seed::diff::resolve_password 内部生成 |

## API 列表

| 方法 | 路径 | 描述 | 权限 |
|------|------|------|------|
| GET | `/seed/list` | 列出所有快照 | Admin+ |
| GET | `/seed/file/{name}` | 读取快照内容 | Admin+ |
| POST | `/seed/save` | 导出当前配置 | SuperAdmin |
| POST | `/seed/load/{name}` | 加载快照 | SuperAdmin |
| DELETE | `/seed/file/{name}` | 删除快照 | SuperAdmin |
| POST | `/seed/diff/{name}` | 文件 vs DB diff | Admin+ |
| POST | `/seed/diff-files` | 两文件 diff | Admin+ |
| GET | `/seed/default` | 获取默认模板 | Admin+ |
| POST | `/seed/apply-default` | 应用默认模板 | SuperAdmin |

## 相关架构改进

本计划同时修复了 `OrganizationDomain::initialize_system` 的架构违规：
- **之前**：organization domain 直接调用 `crate::service::dal::model_provider::dal()`，跨过 finance domain
- **之后**：organization domain 只提供 `create_org_and_owner`（仅创建 org+user），handler 编排 organization + finance domain 完成 provider 创建
