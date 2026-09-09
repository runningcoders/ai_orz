-- 联邦鉴权升级 S2：删除链路共享密钥，配对码支持 DID 钉住
-- SSOT: docs/plan/联邦鉴权升级方案.md §五 / §六 S2

-- 联邦同步链路不再使用共享密钥：出站用本端私钥签名、对端用本端公钥验签。
-- 建联时已交换 peer_did / peer_verification_key（20260909000001），两列随之删除。
ALTER TABLE organization_links DROP COLUMN access_token;
ALTER TABLE organization_links DROP COLUMN peer_token_hash;

-- 配对码签发时可选钉住预期对端 DID（关闭「首达者即身份」窗口的唯一手段，§2.1）：
-- 填了则在 verify_pairing_code 中比对，出示其他 DID 的对端被拒绝。
ALTER TABLE organization_pairing_codes ADD COLUMN expected_peer_did TEXT NULL;
