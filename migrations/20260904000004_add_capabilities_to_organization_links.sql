-- 连接级能力白名单（跨组织业务调用方案 P3）
--
-- organization_links 增加能力白名单列：本节点开放给这条连接的能力清单
-- （JSON 字符串数组）。存量连接迁移后默认开放 a2a_task（跨组织 Agent 委派
-- 的第一闭环能力）；Agent 级白名单延后到真需要时（YAGNI）。
-- 断联（revoke）不删除记录，capabilities 随连接保留，续联可复用。
ALTER TABLE organization_links ADD COLUMN capabilities TEXT NOT NULL DEFAULT '["a2a_task"]';
