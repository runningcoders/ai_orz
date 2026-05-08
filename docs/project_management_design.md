# 项目管理系统设计文档

## 简介

ai_orz 项目管理系统采用统一的 Project Domain 架构，整合项目（Project）、任务（Task）、产物（Artifact）三个核心子模块，支持任务 DAG 依赖管理、产物版本控制、项目进度实时计算等功能。

**设计原则**：严格单向分层、单一职责、实体优先参数传递、避免 N+1 查询。

---

## 架构设计

### 模块结构

```
service/
├── dao/                      # 数据访问层
│   ├── project/              # 项目 DAO（已完成）
│   ├── task/                 # 任务 DAO（已完成）
│   └── artifact/             # 产物 DAO（待改造）
├── dal/                      # 数据业务层
│   ├── project.rs            # ProjectDal（待实现）
│   ├── task.rs               # TaskDal（待实现）
│   └── artifact.rs           # ArtifactDal（待实现）
└── domain/
    └── project/              # 统一的 Project Domain
        ├── mod.rs            # Domain 入口 + Trait 定义
        ├── project.rs        # 项目业务逻辑
        ├── task.rs           # 任务业务逻辑
        ├── artifact.rs       # 产物业务逻辑
        └── project_test.rs   # 统一测试
```

### 实体持有关系

**纯单向持有链**，无反向依赖，无独立统计结构体：

```
Project (1)
└── tasks: Vec<Task>      // 1:N 持有所有任务

Task (2)
└── artifacts: Vec<Artifact>  // 1:N 持有所有产物

Artifact (3)
└── 不反向持有任何（单向链即可）
```

**实时计算方法**：所有统计数据通过方法实时计算，不需要额外的 `ProjectStats` 结构体：

```rust
impl Project {
    pub fn total_tasks(&self) -> u64;
    pub fn completed_tasks(&self) -> u64;
    pub fn progress_percent(&self) -> f64;
    pub fn total_artifacts(&self) -> u64;
    // ... 其他统计方法
}
```

---

## DAL 接口设计原则

### 核心规则：写操作接收实体，读操作接收 ID/参数

```rust
// ✅ 写操作：接收实体（避免重复查询）
async fn create(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;
async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;

// ✅ 读操作：接收 ID/参数
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>, AppError>;
async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<ProjectPo>, AppError>;
```

### 设计优势

| 优势 | 说明 |
|------|------|
| 避免 N+1 查询 | 上层已查询到的实体直接传递，不需要 DAL 内部再查一次 |
| 事务一致性 | 实体状态由上层控制，更新时就是上层看到的状态 |
| 性能优化 | 减少不必要的数据库往返 |
| 灵活性 | 上层可以对实体做多次修改后一次性更新 |

---

## 数据库设计

### 1. projects 表（无需修改）

参考 `docs/project_design.md`，PO 模型与 DAO 已完成。

### 2. tasks 表（无需修改）

参考 `docs/task_design.md`，PO 模型与 DAO 已完成。

### 3. artifacts 表（需要改造）

**变更前**：
```sql
CREATE TABLE artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    task_id TEXT NOT NULL,           -- 只关联任务
    -- ... 其他字段
) STRICT;
```

**变更后**：
```sql
CREATE TABLE artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL,        -- ✅ 新增：必选，关联项目
    task_id TEXT,                    -- ✅ 改造：可选，NULL 表示项目级产物
    -- ... 其他字段不变
) STRICT;
```

**含义**：
- `task_id = NOT NULL` → 任务级产物
- `task_id = NULL` → 项目级产物（不属于任何具体任务）

---

## 核心业务规则

### 任务依赖 DAG（有向无环图）

```rust
impl TaskManage {
    /// 添加任务依赖（自动检测环形）
    async fn add_dependency(
        &self,
        ctx: RequestContext,
        task_id: &str,
        dependency_task_id: &str
    ) -> Result<()> {
        // 1. DFS 深度优先检测环形依赖
        self.check_circular_dependency(ctx, task_id, dependency_task_id).await?;
        
        // 2. 添加依赖关系
        // ...
    }
    
    /// 环形依赖检测
    async fn check_circular_dependency(
        &self,
        ctx: RequestContext,
        start_task_id: &str,
        target_task_id: &str
    ) -> Result<()>;
}
```

**DAG 约束**：
- ❌ 禁止自依赖（A 不能依赖 A）
- ❌ 禁止环形依赖（A→B→C→A）
- ✅ 支持多对多依赖（一个任务依赖多个，多个任务依赖同一个）

### 项目归档与激活

**完整级联操作**：

```rust
impl ProjectManage {
    /// 归档项目
    async fn archive_project(&self, ctx: RequestContext, project_id: &str) -> Result<()> {
        // 1. 更新项目状态 → Archived
        // 2. 级联归档所有下属任务 → Archived
        // 3. 级联标记所有下属产物
        // 4. 记录操作日志
    }
    
    /// 从归档中恢复
    async fn unarchive_project(&self, ctx: RequestContext, project_id: &str) -> Result<()>;
}
```

### 产物版本管理

- 版本号由 Agent 上传时自行指定（如 "v1", "v2", "draft-20240508"）
- 系统不做强校验，Agent 自行组织版本策略

### 产物存储路径

**简化后的路径规则**：
```
/data/artifacts/projects/{project_id}/{artifact_id}
```

**设计优势**：
- ✅ 100% 避免文件名冲突
- ✅ 路径最简单，层级最少
- ✅ 根据 artifact_id 直接定位文件，不需要查询数据库
- ✅ 文件扩展名、原始文件名等信息存储在 `file_meta` JSON 中

---

## DAL 接口完整签名

### ProjectDal

```rust
#[async_trait]
pub trait ProjectDal: Send + Sync {
    // 写操作
    async fn create(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;
    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;
    
    // 读操作
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>, AppError>;
    async fn find_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<Vec<ProjectPo>, AppError>;
    async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<ProjectPo>, AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
}
```

### TaskDal

```rust
#[async_trait]
pub trait TaskDal: Send + Sync {
    // 写操作
    async fn create(&self, ctx: RequestContext, task: &TaskPo) -> Result<(), AppError>;
    async fn update(&self, ctx: RequestContext, task: &TaskPo) -> Result<(), AppError>;
    
    // 读操作
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>, AppError>;
    async fn find_by_project_id(&self, ctx: RequestContext, project_id: &str) -> Result<Vec<TaskPo>, AppError>;
    async fn find_by_agent_id(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<TaskPo>, AppError>;
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<TaskPo>, AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
    
    // 批量操作
    async fn batch_update_status(
        &self,
        ctx: RequestContext,
        task_ids: &[String],
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<(), AppError>;
}
```

### ArtifactDal

```rust
#[async_trait]
pub trait ArtifactDal: Send + Sync {
    // 写操作
    async fn create(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<(), AppError>;
    async fn update(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<(), AppError>;
    
    // 读操作
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ArtifactPo>, AppError>;
    async fn find_by_project_id(&self, ctx: RequestContext, project_id: &str) -> Result<Vec<ArtifactPo>, AppError>;
    async fn find_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<ArtifactPo>, AppError>;
    async fn query(&self, ctx: RequestContext, query: ArtifactQuery) -> Result<Vec<ArtifactPo>, AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
}
```

---

## 开发阶段规划

### 阶段 0：DAO 层改造（当前阶段）
1. [ ] ArtifactPo 模型改造（新增 `project_id`，`task_id` 改为 Option）
2. [ ] 编写数据库迁移文件
3. [ ] Artifact DAO 接口改造与实现
4. [ ] Artifact DAO 单元测试

### 阶段 1：DAL 层实现
1. [ ] ProjectDal 实现 + 单元测试
2. [ ] TaskDal 实现 + 单元测试
3. [ ] ArtifactDal 实现 + 单元测试

### 阶段 2：Domain 层实现
1. [ ] Project 子模块（项目 CRUD + 进度计算）
2. [ ] Task 子模块（任务 CRUD + 状态流转 + DAG 环形检测）
3. [ ] Artifact 子模块（产物上传/下载/审核）
4. [ ] 集成测试（任务完成 → 项目进度自动更新）

### 阶段 3：Handler 层实现
1. [ ] Project 相关 Handler
2. [ ] Task 相关 Handler
3. [ ] Artifact 相关 Handler
4. [ ] 路由注册

---

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-05-09 | 创建设计文档，完成架构对齐与技术方案确认 | 王挺 |

---

## 参考文档

- [Project 模块设计](./project_design.md)
- [Task 模块设计](./task_design.md)
- [分层架构规范](./architecture.md)
- [测试规范](./testing_guidelines.md)
