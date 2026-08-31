---
kind: RAG 原子知识卡
name: 组织权限与用户偏好：Organization 多级组织 + UserRole 并查集继承 + JWT Cookie+Bearer 双模式 + 偏好双源沉淀 + Agent入职流程
category: 业务模块 / 用户组织
scope:
  - "common/src/enums/user_role.rs"
  - "src/service/domain/organization/**"
  - "src/service/domain/hr/agent.rs (入职相关)"
  - "src/service/dal/organization.rs"
  - "src/service/dao/organization/**"
  - "src/service/dao/user/**"
  - "src/handlers/organization/**"
  - "src/pkg/jwt.rs"
  - "src/middleware/auth.rs"
  - "src/models/user.rs (preferences 字段)"
  - "docs/archive/design-archive/organization_design.md"
source_files:
  - common/src/enums/user_role.rs#L1-L60 (UserRole 枚举 + 并查集继承：Member=1 / Admin=2 / SuperAdmin=3；has_permission(need) 用 find_root；禁止 role >= 2 的数字大小比较)
  - src/service/domain/organization/mod.rs#L1-L40 (OrganizationDomain trait：OrganizationManage（CRUD）+ UserManage（CRUD+角色分配+偏好）两个子 trait；子模块 org.rs user.rs 分别 impl)
  - src/service/domain/organization/user.rs#L1-L150 (OrganizationDomain::UserManage：create_user（默认 Member）+ set_role（需要 admin 权限校验 has_permission Admin）+ get_preferences / update_preferences（偏好自报源）)
  - src/service/domain/hr/agent.rs#L250-L330 (HR::onboard_agent：创建 Agent 草稿 → 绑定默认技能包（5 套 TEMPLATE 内存快照里匹配 tag）→ 绑定身份凭证（可选）→ 绑定工具包 → 状态切 Active 的五步流程；每一步失败回滚上一步)
  - src/pkg/jwt.rs (encode_token / decode_token：HS256；claims 含 uid / org_id / role / exp；JWT Cookie + Authorization Bearer 双模式解析同一个 claims)
  - src/middleware/auth.rs#L1-L100 (Axum 鉴权中间件 extractor：优先从 Cookie(ai_orz_token)=JWT → 次从 Header Authorization: Bearer xxx；解析失败统一返回 401 Unauthorized ApiResponse<()>)
  - src/models/user.rs#L20-L80 (UserPo：preferences JSON 字段（自报偏好：theme/language/timezone/notification_channels）；root_user_id 归属过滤 + org_id 多级组织隔离)
  - src/handlers/organization/user/set_user_role.rs#L1-L40 (set_user_role Handler：先 ctx.uid() role.has_permission(Admin) → 再调 OrganizationDomain.set_role；禁止 Member/普通用户给自己提权)
  - docs/archive/design-archive/organization_design.md（§首次启动自举第一个 SuperAdmin §多级组织 root_org_id 级联 §OrganizationManage + UserManage 子 trait）
  - docs/archive/design-archive/agent_onboarding_design.md（§入职五步流程 §技能/凭证/工具绑定顺序 §启动时 Agent 必须 Active）
  - docs/archive/design-archive/request_context_design.md（§ctx.uid/uname/role/org_id 来源 §跨层统一 ctx.clone()）
  - docs/archive/plan-archive/用户偏好双源设计.md（§自报 users.preferences + §推断 user_preference knowledge tag 双源合并 §冲突优先级）
  - docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md（§入职时凭证绑定第四步 §AES256-GCM 加密存储 reference）
  - docs/archive/plan-archive/调用者类型上下文.md（§caller_type=User/Agent/System 三枚举 注入 ctx §权限判断 caller_type + role 组合）
  - docs/wiki/zh/content/功能模块/用户与组织管理/用户与组织管理.md（用户组织管理全景：首次启动→创建组织→邀请成员→分配角色→配置偏好 5 步）
  - docs/wiki/zh/content/功能模块/用户与组织管理/组织管理.md（多级组织架构树 + 级联权限 + root_org_id 归属过滤）
  - docs/wiki/zh/content/功能模块/用户与组织管理/用户认证与授权.md（JWT Cookie + Bearer 双模式说明；401/403 错误语义）
  - docs/wiki/zh/content/功能模块/用户与组织管理/用户管理.md（用户列表 + 角色分配 + 偏好配置 UI）
  - docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/技能与工具绑定.md（入职流程五部曲第三步：装默认技能 + 第四步：绑工具 + 凭证绑定）
  - migrations/20260830000000_add_org_config.sql#L1-L8（organizations 表新增 config 列 TEXT NOT NULL DEFAULT '{}'，承载组织级可扩展配置的 JSON）
  - common/src/api/organization.rs#L7-L20（OrganizationConfig：enable_message_vector bool + #[serde(default)]；所有字段必须带默认值保证向后兼容）
  - common/src/api/organization.rs#L155-L238（OrganizationInfoResponse 新增 config 字段 + UpdateCurrentOrganizationRequest / UpdateOrganizationRequest 新增 config: Option<OrganizationConfig>）
  - src/service/dao/organization/sqlite.rs#L17-L24（ORG_CONFIG_CACHE 全局 LazyLock<Mutex<HashMap>> 缓存）
  - src/service/dao/organization/sqlite.rs#L111-L147（get_org_config 读穿 + set_org_config 写穿双模式实现）
  - src/service/dao/organization/sqlite.rs#L247-L268（read_org_config_from_db：SQL 裸查询 + 空/非法 JSON 回退 OrganizationConfig::default()）
  - src/service/dao/organization/mod.rs#L29-L39（OrganizationDao trait 新增 get_org_config / set_org_config）
  - src/service/dal/organization.rs#L55-L65 + #L123-L140（OrganizationDal trait + impl 新增 get_org_config / update_org_config 透传 DAO）
  - src/service/domain/organization/mod.rs#L119-L129（OrganizationManage trait 新增 get_org_config / update_org_config）
  - src/service/domain/organization/org.rs#L148-L165（OrganizationManage impl 配置读写透传 DAL）
  - src/service/dal/message.rs#L156-L174（消息落库时检查 org_config.enable_message_vector，默认 false 跳过向量索引构建）
  - 【平行卡 1】docs/wiki/knowledge/zh/身份凭证 AES-256-GCM 敏感字段加密 + 统一 CRUD Domain + Handler八文件迁移/身份凭证 AES-256-GCM 敏感字段加密.md（入职第四步：身份凭证绑定 → Finance::Credential.create → AES256-GCM 加密存储）
  - 【平行卡 2】docs/wiki/knowledge/zh/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md（入职第二步：install_skill_pack tag="memory" 拉 Published 技能 → 复制 Draft）
---

## §1 概述

**本卡角色**：组织权限与用户偏好的知识卡。覆盖 OrganizationDomain 的组织 + 用户双 trait、UserRole 三级并查集（Member→Admin→SuperAdmin 继承规则，has_permission 统一入口）、JWT Cookie+Bearer 双模式鉴权中间件、用户偏好双源沉淀（users.preferences 自报 + knowledge graph user_preference tag 推断，冲突自报优先）、Agent 入职五步流程（草稿→装技能→绑凭证→绑工具→Active）。**定位：新增角色、排查用户 403 权限不足、调试 Agent 入职卡住某一步、理解偏好冲突优先级时读。**

- **UserRole 并查集继承规则（禁止 role >=2 数字比较，必须用 match + find_root）**（common/src/enums/user_role.rs）：`Member(1)` 基础角色（创建自己的消息/任务）；`Admin(2)` 组织级管理员（邀请成员/分配角色/删除组织内资源，继承 Member 所有权限）；`SuperAdmin(3)` 系统级超管（跨组织管理/备份/系统初始化，继承 Admin 所有权限）。`role.has_permission(UserRole::Admin)` 的实现：`find_root(self) >= find_root(Admin)`，不是简单 `self as i32 >= need as i32`（未来增加 SubAdmin = 1.5 中间层级时旧写法全 break）。AGENTS.md §4.3 强制枚举类型安全。
- **JWT Cookie + Authorization Bearer 双模式**（pkg/jwt.rs + middleware/auth.rs）：鉴权中间件先查 Cookie: ai_orz_token（浏览器用户，防 XSS 用 `HttpOnly; Secure; SameSite=Lax`）→ 未发现再查 Header Authorization: Bearer <token>（API/脚本调用）；两者解析后得到同一个 Claims{ uid, uname, org_id, role, exp }。decode_token 时校验 exp（过期 5 分钟内可以容忍吗？= 不行，硬校验过期立即 401），校验 org_id 与请求的资源 org_id 是否一致（跨组织访问=403）。
- **用户偏好双源沉淀（冲突处理 3 条规则）**（用户偏好双源设计文档 §3）：**源 1（自报高优先）** = users.preferences JSON（用户在设置页手动改 theme、language、notification_channels，存 DB 不丢）；**源 2（推断低优先）** = 知识图谱里 user_preference tag 的节点（Agent 与用户交互时观察到"用户喜欢中文回复/喜欢上午 9 点处理邮件"，LongTerm 沉淀时创建 knowledge node）。合并逻辑 3 条：① 自报有且推断有，取自报值；② 自报无 + 推断有，取推断值并在前端 UI 旁边标「AI 推荐（可修改）」；③ 自报有=某值 + 推断冲突值=另一个 → 完全忽略推断（不写入自报，不弹通知，避免覆盖用户显式设置）。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| common/enums/user_role.rs UserRole 枚举 | 角色 + 继承规则 | Member=1 / Admin=2 / SuperAdmin=3；#[repr(i32)]+sqlx::Type；has_permission(need)；find_root() 并查集上溯；禁止直接比较 i32 数字 | `:L1-L60` |
| domain/organization/mod.rs OrganizationDomain | 域总 trait | 两个子 trait：OrganizationManage（创建/删除/查询组织，级联归属 root_org_id）+ UserManage（创建用户/分配角色/偏好读写）；子模块 org.rs user.rs 分别 impl | `:L1-L40` |
| domain/organization/user.rs UserManage impl | 用户管理 + 偏好 | create_user（默认 Member）；set_role（调用者必须 has_permission(Admin) 且不能给自己升 SuperAdmin）；get/update_preferences（读 users.preferences JSON，字段校验 theme 是 30+ 内置主题之一） | `:L1-L150` |
| domain/hr/agent.rs HR::onboard_agent | Agent 入职五步 | `1. create_agent_draft`（status=Draft）→ `2. install_default_skill_packs`（TOOL/MEMORY/COMMUNICATION 三套 tag）→ `3. bind_identity_credentials(option)`（有凭证时走 Finance::Credential.create，AES256-GCM 加密）→ `4. bind_default_tool_packs`（按技能包 tag 匹配工具包）→ `5. set_status(Active)`；失败按顺序回滚 | `:L250-L330` |
| pkg/jwt.rs JWT 编码解码 | 令牌工具 | encode_token(claims) → HS256 + secret 从配置；decode_token(token) → 解析 Claims + 校验 exp + iss 双字段；错误统一 JwtError → ApiError::Unauthorized | 见 jwt.rs |
| middleware/auth.rs 鉴权中间件 | 401/403 生成 | 优先 Cookie ai_orz_token → 次 Authorization Bearer → 两者都无 → 401；解析后 claims 注入 RequestContext(uid,uname,org_id,role)；org_id 与请求资源不匹配 → 403（例：用户组织 A 访问 /organizations/B/users） | `:L1-L100` |
| models/user.rs UserPo | 用户持久化 | id/name/hash_password/email/phone/organization_id/role(UserRole)/preferences(JSON)/status(UserStatus)/created_at；preferences 结构：`{theme, language, timezone, notifications: {lark, email, sse}}` | `:L20-L80` |
| handlers/organization/user/set_user_role.rs 设置角色 Handler | 权限双校验 | ① ctx.role().has_permission(Admin)（调用者不能是 Member）→ ② target_user_id 不能是 ctx.uid()（不能给自己改角色）→ ③ 升 SuperAdmin 必须调用者自己是 SuperAdmin → 通过后才调 OrganizationDomain.set_role | `:L1-L40` |
| common/src/api/organization.rs OrganizationConfig DTO | 组织级可扩展配置 | enable_message_vector bool + #[serde(default)]；所有字段必须带默认值保证向后兼容；存于 organizations.config TEXT 列（JSON） | `:L7-L20` |
| common/src/api/organization.rs OrganizationInfoResponse + UpdateCurrentOrganizationRequest + UpdateOrganizationRequest | Handler 层 DTO 扩展 | 三个 DTO 同步新增 config 字段：OrganizationInfoResponse.config（返回）+ UpdateCurrentOrganizationRequest.config: Option + UpdateOrganizationRequest.config: Option（写入） | `:L155-L238` |
| service/dao/organization/sqlite.rs ORG_CONFIG_CACHE + get_org_config / set_org_config | DAO 层配置缓存 | ORG_CONFIG_CACHE 全局 LazyLock<Mutex<HashMap<org_id, OrganizationConfig>>>；get_org_config 读穿（命中缓存直接返回，未命中回退 DB 并回填）；set_org_config 写穿（DB 落盘后同步刷新缓存）；read_org_config_from_db 对非法 JSON 回退 OrganizationConfig::default() | `:L17-L24` + `:L111-L147` + `:L247-L268` |
| service/dal/organization.rs OrganizationDal | DAL 层配置透传 | trait + impl 新增 get_org_config / update_org_config 透传 DAO，保持 DAL 作为数据访问层的薄封装定位 | `:L55-L65` + `:L123-L140` |
| service/domain/organization/mod.rs + org.rs OrganizationManage | Domain 层配置能力 | trait 新增 get_org_config / update_org_config；impl 透传 DAL 调用，Domain 层不做业务逻辑，仅保持能力暴露 | `:L119-L129` + `:L148-L165` |
| service/dal/message.rs 消息落库 | 下游消费点 | 消息落库时检查 org_config.enable_message_vector（经 DAO 缓存读取），默认 false 跳过向量索引构建，FTS 搜索不受影响 | `:L156-L174` |
| migrations/20260830000000_add_org_config.sql | DB 迁移 | `ALTER TABLE organizations ADD COLUMN config TEXT NOT NULL DEFAULT '{}'`；后续新增组织级配置项只需追加 JSON 字段，无需改表结构 | `:L1-L8` |

**章节来源**
- [user_role.rs:L1-L60](common/src/enums/user_role.rs#L1-L60)
- [domain/organization/user.rs:L1-L150](src/service/domain/organization/user.rs#L1-L150)
- [middleware/auth.rs:L1-L100](src/middleware/auth.rs#L1-L100)

---

## §3 首次启动自举 + Agent 入职时序

### 首次启动自举第一个 SuperAdmin（组织设计文档 §模块概述）

```
ai_orz serve → lib.rs::run() 初始化
  → pkg::init_all → service::init
  → service::init_base_data().await
       → organization::init_base_data()
          ├─ users 表 count=0？是=创建组织「默认组织」
          ├─ 创建用户 admin / 密码=环境变量 AI_ORZ_ADMIN_PASSWORD（没设置则生成随机密码并在 syslog 中打印 WARNING：「首次启动 SuperAdmin: admin / 密码=xxxxx，请立即在用户管理中修改」）
          └─ role=SuperAdmin + 写入 users + 组织成员关联表
          └─ 调用 HR::onboard_agent 创建默认前台 Agent「前台助手」（入职五步）
  → 前端登录页输入 admin + 密码 → JWT 签发 Cookie ai_orz_token → 进入系统
  → SuperAdmin 立即创建其他成员（Admin/Member）→ 邀请邮件链接（Token 1小时有效）
```

### Agent 入职五步（详细顺序）

```
HR 面板：填写 Agent 名 / 角色描述 / ModelProvider 选择 → 点「入职」
  ↓
  1. create_agent_draft
    → HR::create_agent → 插入 agents 表 status=Draft
    → 返回 agent_id

  2. install_default_skill_packs（tool_management, memory_cognition, communication 三套）
    → 对每个 tag 调用 HR::install_skill_pack(ctx, agent_id, tag)
       → SkillDal.query(Published, tags_contains[tag])
       → 复制 Draft 版本到 agent 私有目录
    → 全部安装成功 = 下一步；任一失败 = 回滚步骤 1（删除 agent_draft）

  3. bind_identity_credentials（如果飞书/企业微信/Lark 凭证已配置在 Finance 凭证池且勾选绑定）
    → Finance::CredentialBindings.create(agent_id, credential_id, scope="lark_p2p")
    → 失败 = 回滚步骤 2（uninstall 所有已装技能包）+ 删 agent_draft

  4. bind_default_tool_packs
    → 根据 installed_skill_packs 的 tag 匹配 tool_packs
       例：tag="memory" → tools 表 tags_contains["memory", "neural"] → 批量绑定 insert into agent_tool_bindings
    → 失败 = 回滚 3/2/1

  5. HR::set_agent_status(agent_id, Active)
    → agents.status 从 Draft(0) → Active(1)
    → AOP publish agent.onboarded 事件 → 欢迎消息发给创建者
    → 返回成功，可在 Agent 列表中看到「在线」
```

---

## §4 硬约束与回归红线（10 条）

1. **UserRole 权限判断永远通过 has_permission 不用数字**：代码中出现 `role as i32 >= 2` 或 `role == 0` 直接 fail（clippy lint 自定义规则开启）；正确写法：`ctx.role().has_permission(UserRole::Admin)`。单元测试必须覆盖 Member/Admin/SuperAdmin 三个组合调用 set_user_role。
2. **鉴权中间件 401 不泄露 JWT 解析失败的细节**：401 body `ApiResponse<()>` message 固定"未登录或会话过期"，绝不返回 "签名错误" / "JWT 过期 14 分钟"（防止攻击者区分合法 JWT 但过期 vs 伪造 JWT）。
3. **set_user_role 三锁：调用者≥Admin / 目标≠自己 / 升 SuperAdmin 调用者是 SuperAdmin**：三个条件任一违反 → 403 带 message "权限不足，无法修改角色"；详细原因写入 syslog（审计用）不返前端。
4. **Agent 入职五步原子回滚**：onboard_agent 是 async fn，步骤 N 失败 → 回滚 N-1、N-2…1；回滚顺序必须是步骤的逆（先删后建的资源，防止 FK 约束删不掉）。测试环境模拟步骤 3 失败 → 验证 agent_id 对应的 agents 记录已被删除（COUNT=0）。
5. **用户偏好自报值永不被推断值覆盖**：`UserPreferences::merge(self_preferences, inferred_preferences)` 代码里，只要 self_preferences 的字段不是 null → 跳过该字段的推断写入；推断值只能作为「推荐值」显示在前端 UI 旁边黄色「AI 推荐」小胶囊，需要用户点「采纳」才写 DB。
6. **JWT secret 不能编译期嵌进二进制**：JWT_SECRET 从环境变量 + 配置文件覆盖（common config 三层优先级：嵌入默认 < 配置文件 < 环境变量）；生产若检测到使用「嵌入默认 secret」立即 panic（防止 Docker 镜像里默认 secret 被公开）。
7. **SuperAdmin 跨组织访问必须显式加系统级 scope 标记**：ctx.caller_type=SuperAdmin 但访问 /organizations/B（非自己 org）时，必须在请求 URL 里带 `?as_system=true` 且前端 UI 显示红色「系统级操作，有审计日志」横幅；否则默认返回 403（防止 SuperAdmin 误操作其他组织的数据，有审计追踪）。
8. **OrganizationConfig 所有字段必须带 #[serde(default)] 默认值**：JSON 反序列化时，缺失字段 → 回退 Rust 默认值（bool=false、Option=None）；保证 DB 列 `config` 为 '{}' 或旧版本迁移过来缺失字段时仍能正常解析，绝不 panic；禁止新增无默认值字段。
9. **organizations.config 列 TEXT NOT NULL DEFAULT '{}' + 代码侧 JSON 回退**：DB 层默认值为空 JSON 对象 '{}'，代码侧 `read_org_config_from_db` 对空字符串、trim 后为空、或非法 JSON 一律回退 `OrganizationConfig::default()`；两层兜底保证向后兼容和容错。
10. **DAO 层 ORG_CONFIG_CACHE 采用读穿 + 写穿双模式，避免每条消息落库回查 DB**：消息落库（message DAL）和 Handler 读当前组织配置均走 DAO 缓存；缓存键为 org_id，值为解析后的整个 OrganizationConfig；set_org_config 先落盘再刷缓存（防止写 DB 失败但缓存已脏）；严禁任何业务层直接 SELECT organizations.config 绕过 DAO 缓存。
