# 组织初始化与用户认证模块设计文档

## 模块概述

本模块实现了系统初始化功能，支持首次启动从零创建第一个组织和超级管理员用户，同时提供完整的组织管理和用户管理 HTTP API。

## 架构设计

### 整体分层架构（对齐项目现有模式）

```
handler → domain → dal → dao → sqlite
```

| 层级 | 职责 |
|------|------|
| **handler** | HTTP 接口层，按功能分组：`organization/organization/` 组织管理接口、`organization/user/` 用户管理接口 |
| **domain** | 业务逻辑层，定义 `OrganizationDomain` trait，包含 `OrganizationManage` 和 `UserManage` 两个子 trait |
| **dal** | 数据访问层，封装数据库访问 |
| **dao** | 数据访问对象层，sqlite 具体实现 |

### 目录结构

#### handler 层（按功能二次分组）

```
src/handlers/organization/
├── initialize_system.rs        # 系统初始化（顶层独立）
├── organization/              # 组织管理分组
│   ├── mod.rs
│   ├── delete_organization.rs  # 删除组织
│   ├── get_organization.rs     # 获取组织信息
│   ├── list_organizations.rs   # 获取组织列表
│   └── update_organization.rs  # 更新组织
├── user/                      # 用户管理分组
│   ├── mod.rs
│   ├── create_user.rs          # 创建新用户
│   ├── delete_user.rs          # 删除用户
│   ├── get_user_by_username.rs  # 根据用户名查询用户（登录用）
│   ├── list_users_by_organization.rs  # 根据组织查询用户列表
│   └── update_user.rs         # 更新用户信息
└── mod.rs                     # 顶层导出
```

#### domain 层

```
src/service/domain/organization/
├── mod.rs      # 顶层 trait 定义 + 单例 + init
├── org.rs       # 组织管理 trait 实现
└── user.rs     # 用户管理 trait 实现
```

#### dal 层

```
src/service/dal/
└── organization.rs  # OrganizationDal 实现
```

#### dao 层

```
src/service/dao/organization/
├── mod.rs        # trait 定义
└── sqlite.rs    # sqlite 实现

src/service/dao/user/
├── mod.rs    # trait 定义
└── sqlite.rs  # sqlite 实现
```

## 功能清单

### 系统初始化流程

1. 服务启动
2. 调用 `domain.organization_manage().check_initialized()` 检查是否已有组织
3. 如果没有组织 → 前端自动跳转到初始化向导页面
4. 用户填写：组织名称、描述、超级管理员用户名、bcrypt 加密后的密码、显示名称、邮箱
5. 后端调用 `initialize_system()` → 自动：
   - 生成组织 ID
   - 创建组织记录
   - 生成用户 ID
   - 创建超级管理员用户 → 关联 organization_id，角色 = SuperAdmin
   - 返回 `(organization_id, user_id)` 给前端
6. 初始化完成 → 前端跳转登录页面

### HTTP API 接口

| 方法 | 路径 | 功能 | Handler 文件 |
|------|------|------|------|
| POST | `/api/organization/initialize` | 系统初始化 | `initialize_system.rs` |
| GET | `/api/organization/{org_id}` | 获取组织信息 | `organization/get_organization.rs` |
| GET | `/api/organization/list` | 获取组织列表 | `organization/list_organizations.rs` |
| PUT | `/api/organization/update` | 更新组织信息 | `organization/update_organization.rs` |
| DELETE | `/api/organization/{org_id}` | 删除组织 | `organization/delete_organization.rs` |
| POST | `/api/organization/user` | 创建新用户 | `user/create_user.rs` |
| GET | `/api/organization/user/{username}` | 根据用户名查询用户（登录）| `user/get_user_by_username.rs` |
| GET | `/api/organization/{org_id}/users` | 获取组织下用户列表 | `user/list_users_by_organization.rs` |
| PUT | `/api/organization/user/update` | 更新用户信息 | `user/update_user.rs` |
| DELETE | `/api/organization/user/{user_id}` | 删除用户 | `user/delete_user.rs` |

## 数据模型

### OrganizationPo (组织持久化对象)

```rust
pub struct OrganizationPo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: i32,  // 0-未删除, 1-已删除
}
```

### UserPo (用户持久化对象)

```rust
pub struct UserPo {
    pub id: String,
    pub organization_id: String,
    pub username: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: i32,  // 0-SuperAdmin, 1-OrgAdmin, 2-OrgMember
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: i32,
}
```

### UserRole 枚举

```rust
pub enum UserRole {
    SuperAdmin = 0,
    OrgAdmin = 1,
    OrgMember = 2,
}
```

## 日志模块设计

### 输出配置

| 输出位置 | 说明 | 默认路径 |
|----------|------|----------|
| 控制台 | 始终输出，方便开发调试 | - |
| 文件 | 按日期自动滚动，持久化日志 | `/data/logs` |

### 日志特性

- ✅ 自动创建日志目录（不存在时创建）
- ✅ 按日期自动轮换，每天新建一个日志文件
- ✅ 非阻塞写入，不阻塞主线程
- ✅ 同时输出到控制台和日志文件
- ✅ 可配置日志根路径（修改 `LOG_ROOT` 常量即可）
- ✅ 文件名格式：`ai_orz.log.YYYY-MM-DD`

### 日志目录结构

```
/data/logs/
├── ai_orz.log.2026-04-08
├── ai_orz.log.2026-04-07
├── ai_orz.log.2026-04-06
└── ...
```

## 开发日志

### 开发日期
**2026-04-08**

### 完成功能

- [x] 数据库表创建 `organizations` + `users`
- [x] 模型定义 `OrganizationPo` + `UserPo` + `UserRole`
- [x] DAO 层实现
- [x] DAL 层实现
- [x] Domain 层实现（对齐项目现有结构）
- [x] Handler 层实现（按功能二次分组对齐项目现有结构）
- [x] 日志模块分离测试代码到独立文件 `logging_test.rs`
- [x] 添加 `tracing-appender` 依赖实现按日期滚动日志
- [x] 默认日志路径配置为 `/data/logs`，可配置
- [x] 自动创建日志目录
- [x] 编译成功，所有测试通过

### 验证结果

```
running 30 tests
test result: ok. 30 passed; 0 failed; 99 warnings emitted;
```

**✅ 全部测试通过，编译成功，功能完成**

## 作者
开发: AI Orz 开发团队
