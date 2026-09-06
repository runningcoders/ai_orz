//! 微信 iLink 集成 handlers（finance domain：身份凭证资产 + 扫码登录）
//!
//! 路由统一挂 `/api/v1/finance/identity/wechat/`：
//! 扫码登录（二维码获取 / 状态长轮询），confirmed 时凭据自动落库（整组轮换语义）。
//! 凭据由扫码产生，无手填表单；删除/改名类 CRUD 待渠道页需要时补齐。

pub mod get_login_qrcode;
pub mod get_status;
pub mod login_status;
