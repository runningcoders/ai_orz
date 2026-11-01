<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 01-组织与权限

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 41 篇：A 设计 1 · B 落地方案 4 · C 知识 36。

## A 设计（design）

- [用户身份凭证独立表设计（user_credentials）](<A-设计/user_credentials_design.md>) — `现行`

## B 落地方案（plan）

- [用户身份凭证独立表落地](<B-落地方案/用户身份凭证独立表落地.md>) — `已完成`
- [组织组网与去中心化联邦方案（评审稿）](<B-落地方案/组织组网与去中心化联邦方案.md>) — `草案`
- [联邦鉴权升级方案（静态 token → 组织公钥签名）](<B-落地方案/联邦鉴权升级方案.md>) — `进行中`
- [跨组织业务调用方案（Phase 2 主体）](<B-落地方案/跨组织业务调用方案.md>) — `已完成`

## C 知识（wiki）

- [Agent 管理 API](<C-知识/Agent 管理 API.md>)
- [GitHub 集成](<C-知识/GitHub 集成.md>)
- [HR模块处理器](<C-知识/HR模块处理器.md>)
- [IdentityCredentials 身份凭证扩展：WechatIlink 扫码授权 + 凭证整组轮换 + 与通用 CRUD 框架的集成点](<C-知识/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理.md>)
- [Organization模块处理器](<C-知识/Organization模块处理器.md>)
- [Skill处理器](<C-知识/Skill处理器.md>)
- [User模块处理器](<C-知识/User模块处理器.md>)
- [人力资源模块 API](<C-知识/人力资源模块 API.md>)
- [当前组织信息处理器](<C-知识/当前组织信息处理器.md>)
- [技能包管理 API](<C-知识/技能包管理 API.md>)
- [用户与组织管理](<C-知识/用户与组织管理.md>)
- [用户管理](<C-知识/用户管理.md>)
- [用户管理处理器](<C-知识/用户管理处理器.md>)
- [用户管理模块 API](<C-知识/用户管理模块 API.md>)
- [用户认证与授权](<C-知识/用户认证与授权.md>)
- [系统初始化处理器](<C-知识/系统初始化处理器.md>)
- [组织权限与用户偏好：Organization 多级组织 + UserRole 并查集继承 + JWT Cookie+Bearer 双模式 + 偏好双源沉淀 + Agent入职流程](<C-知识/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md>)
- [组织管理](<C-知识/组织管理.md>)
- [组织管理处理器](<C-知识/组织管理处理器.md>)
- [组织管理模块 API](<C-知识/组织管理模块 API.md>)
- [组织组网与联邦](<C-知识/组织组网与联邦.md>)
- [组织领域](<C-知识/组织领域.md>)
- [聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成](<C-知识/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成.md>)
- [联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断](<C-知识/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断.md>)
- [联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接](<C-知识/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接.md>)
- [认证与授权 API](<C-知识/认证与授权 API.md>)
- [认证与授权](<C-知识/认证与授权.md>)
- [认证处理器](<C-知识/认证处理器.md>)
- [记忆管理 API](<C-知识/记忆管理 API.md>)
- [跨组织业务调用鉴权](<C-知识/跨组织业务调用鉴权.md>)
- [跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌](<C-知识/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities.md>)
- [身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用](<C-知识/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用.md>)
- [身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式](<C-知识/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式.md>)
- [身份凭证与授权流程](<C-知识/身份凭证与授权流程.md>)
- [身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立](<C-知识/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md>)
- [身份凭证统一链路（总卡）](<C-知识/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md>)
