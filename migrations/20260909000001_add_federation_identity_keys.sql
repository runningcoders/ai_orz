-- 联邦身份密钥底座（S1，SSOT: docs/plan/联邦鉴权升级方案.md §五）
--
-- organizations 三列：
-- - did              组织 DID（did:key:z...，公钥编码进标识符；Local 组织自生成，
--                    Remote/Linked 影子组织随目录同步复制对端值）
-- - verification_key 联邦公钥（Ed25519 32B base64），随目录同步公开给对端
-- - signing_key      联邦私钥种子（32B base64），经 encrypt_channel_secret 加密落库；
--                    仅 Local 组织持有，影子组织恒 NULL
ALTER TABLE organizations ADD COLUMN did TEXT NULL;
ALTER TABLE organizations ADD COLUMN verification_key TEXT NULL;
ALTER TABLE organizations ADD COLUMN signing_key TEXT NULL;

-- organization_links 两列（S2 建联交换后填充；S1 先立列，不改任何鉴权行为）
-- - peer_did                对端组织 DID（入站验签时与 X-Federation-Key-Id 比对）
-- - peer_verification_key   对端联邦公钥（Ed25519 32B base64，入站验签依据）
ALTER TABLE organization_links ADD COLUMN peer_did TEXT NULL;
ALTER TABLE organization_links ADD COLUMN peer_verification_key TEXT NULL;
