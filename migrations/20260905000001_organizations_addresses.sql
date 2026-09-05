-- P7 内外网可达性：组织自报联邦地址全集（三层模型之①层）
--
-- organizations.addresses 存对端自报的地址列表（JSON 数组，元素
-- {url, scope}，scope = "private" | "public"），由目录同步/建联维护，
-- 如实记录、只读参考——可达性裁决由出站探测（解析器）完成，
-- 实际在用地址仍以 organization_links.endpoint 为准（③层）。
--
-- 裸 SQL 读写、不进 OrganizationPo（先例：config 列），避免 PO/查询全量改动。
-- organization_links 不加 override 列（YAGNI：本端人工修正候选的需求出现再加）。

ALTER TABLE organizations ADD COLUMN addresses TEXT NOT NULL DEFAULT '[]';
