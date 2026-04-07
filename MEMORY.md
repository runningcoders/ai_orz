# MEMORY.md - 项目长期记忆

## 项目名称
**ai_orz** - AI Agent 平台后端项目

## 核心架构

### 实体关系层次

```
Agent (po + brain)
  └─► Brain (cortex)
       └─► Cortex (model_provider + dyn CortexTrait)
            ├─► ModelProvider (po)
            └─► dyn CortexTrait (prompt)
```

### 分层职责

| 层级 | 模块 | 职责 |
|------|------|------|
| **DAO 层** | `service/dao/cortex/CortexDao` | 创建 `CortexTrait` 实例，执行 `prompt` 获取回答 |
| **DAL 层** | `service/dal/cortex/CortexDal` | 创建完整 `Cortex` 实体，组装 `ModelProvider + CortexTrait` |
| **DAL 层** | `service/dal/agent/AgentDal` | Agent 业务逻辑，组装 `Agent + Brain` |
| **DAL 层** | `service/dal/model_provider/ModelProviderDal` | ModelProvider 业务逻辑，`wake_cortex` 测试连通性 |
| **Domain 层** | `service/domain/finance/model_provider` | 领域层处理，对应 Handler 层方法 |
| **Handler 层** | `handlers/finance/model_provider` | REST API 入口，每个方法单独文件 |

### 核心命名规范

- `Cortex` → 实体结构体，包含 `model_provider + Box<dyn CortexTrait>`
- `CortexTrait` → 推理 trait，提供 `prompt(&self, prompt: &str) -> Result<String>`
- `CortexDao` → 工厂 trait，创建 `CortexTrait` 实例，执行 `prompt`
- `CortexDal` → 业务层，组装完整 `Cortex` 实体

### 强约束规则

1. **所有 service 层（DAO/DAL/Domain）公共方法都必须传递 `ctx: RequestContext` 作为第一个参数**
   - 即使方法当前不使用，也必须传递，方便后续日志串联和扩展
   - 遵循依赖注入约定，满足 tracing 需求

2. **目录结构对齐**
   - `handlers/{business}/{module}/{method}.rs`
   - `hr/agent/` 和 `finance/model_provider/` 结构一致

3. **单元测试**
   - 每个业务模块都应有单元测试
   - 当前测试总数：**31**，全部通过

### 今日重构总结 (2026-04-07)

## 重构决策记录

1. **核心实体层次重构**
   - ✅ `Agent` 直接持有 `Option<Brain>`，动态装配，不持久化
   - ✅ `Brain` 直接持有 `Cortex`
   - ✅ `Cortex` 直接持有 `ModelProvider + Box<dyn CortexTrait>`
   - ✅ `ModelProvider` 只持有配置 (`po`)，不持有执行实例
   - ✅ 移除了 `Brain` 中重复的 `cortex` 字段，避免冗余

2. **分层职责清晰化**
   - ✅ DAO 创建 `CortexTrait`，DAL 创建 `Cortex` 实体
   - ✅ 所有调用收敛到 DAO 层
   - ✅ 简单模块保持单文件，不需要分文件夹

3. **命名修正**
   - ✅ `BrainDao` → `CortexDao`（Brain 是聚合根，CortexDao 只创建 Cortex）
   - ✅ `create_brain` → `create_cortex_trait`
   - ✅ `wake_brain` → `wake_cortex`（实际唤醒的是 Cortex，不是 Brain）
   - ✅ `Cortex` trait → `CortexTrait`（遵循 Rust 命名规范，Cortex 现在是实体结构体）
   - ✅ `wake_cortex` in CortexDao → `prompt`（不再有"唤醒"概念，就是执行 prompt）

4. **依赖注入**
   - ✅ `ModelProviderDal` 持有 `CortexDao`，构造函数注入
   - ✅ `AgentDal` 不再依赖 `CortexDao`，上层组装好 Brain 传入，只做赋值和更新
   - ✅ `Agent` 业务实体持 `brain` 可选字段，持久化不存储，只通过 DAL 动态装配

5. **强规范推行**
   - ✅ 所有 service 层方法都传递 `ctx: RequestContext` 参数
   - ✅ 这是强约定，即使当前不使用，也必须传递，方便未来扩展

6. **单元测试补充**
   - ✅ `service/dao/cortex/rig_test.rs` - 6 个测试，全部通过
   - ✅ `service/dal/cortex_test.rs` - 2 个测试，全部通过
   - ✅ `service/dao/model_provider/sqlite_test.rs` - 3 个测试，全部通过

## 最终验证结果 (2026-04-07)

```
running 31 tests
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

✅ 编译成功，所有测试通过，满足所有规范要求

## 开发者约定

- 每次大的重构后，更新此文件记录决策
- 保持命名一致性，遵循已有约定
- 方法粒度拆分，每个 handler 方法单独文件
- 提交前必须编译测试通过
