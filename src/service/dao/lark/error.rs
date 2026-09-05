//! 飞书 DAO 错误类型
//!
//! 飞书 OpenAPI 调用相关错误定义，转换为 common::error::Error 后向上传递。

use common::error::{Error, err};
use serde::Deserialize;

/// 飞书 OpenAPI 错误响应体（业务层错误码）
#[derive(Debug, Clone, Deserialize)]
pub struct LarkApiError {
    /// 飞书业务错误码（0 表示成功）
    pub code: i32,
    /// 错误信息
    pub msg: String,
}

/// 飞书 OpenAPI 响应通用包装
#[derive(Debug, Clone, Deserialize)]
pub struct LarkResponse<T> {
    pub code: i32,
    pub msg: String,
    #[serde(default)]
    pub data: Option<T>,
}

impl<T> LarkResponse<T> {
    /// 将飞书响应转换为 ai_orz Result
    ///
    /// - code = 0：返回 data
    /// - code != 0：返回 ThirdPartyError
    pub fn into_result(self, op: &str) -> Result<T, Error>
    where
        T: Default,
    {
        if self.code == 0 {
            Ok(self.data.unwrap_or_default())
        } else {
            Err(err!(
                ThirdPartyError,
                "lark {} failed: code={} msg={}",
                op,
                self.code,
                self.msg
            ))
        }
    }

    /// code != 0 时返回错误（data 一定存在场景）
    pub fn check(self, op: &str) -> Result<T, Error> {
        if self.code == 0 {
            self.data
                .ok_or_else(|| err!(ThirdPartyError, "lark {} returned empty data", op))
        } else {
            Err(err!(
                ThirdPartyError,
                "lark {} failed: code={} msg={}",
                op,
                self.code,
                self.msg
            ))
        }
    }
}

/// 飞书 WebSocket 协议错误
#[derive(Debug)]
pub enum LarkWsError {
    /// 获取连接地址失败
    EndpointFetch(String),
    /// WebSocket 连接失败
    Connect(String),
    /// 协议错误（心跳/帧格式）
    Protocol(String),
    /// 解析事件失败
    EventParse(String),
}

impl std::fmt::Display for LarkWsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EndpointFetch(m) => write!(f, "lark ws endpoint fetch failed: {}", m),
            Self::Connect(m) => write!(f, "lark ws connect failed: {}", m),
            Self::Protocol(m) => write!(f, "lark ws protocol error: {}", m),
            Self::EventParse(m) => write!(f, "lark ws event parse failed: {}", m),
        }
    }
}

impl std::error::Error for LarkWsError {}

impl From<LarkWsError> for Error {
    fn from(e: LarkWsError) -> Self {
        err!(ThirdPartyError, "lark ws error: {}", e)
    }
}

/// 将 reqwest 错误转换为 ai_orz Error
pub fn from_reqwest(op: &str, e: reqwest::Error) -> Error {
    err!(ThirdPartyError, "lark {} http error: {}", op, e)
}

/// 将 serde_json 错误转换为 ai_orz Error
pub fn from_serde(op: &str, e: serde_json::Error) -> Error {
    err!(ThirdPartyError, "lark {} parse error: {}", op, e)
}

/// 凭证未配置错误便捷构造
pub fn missing_credentials() -> Error {
    err!(
        ConfigMissing,
        "lark credentials not configured (app_id/app_secret empty)"
    )
}

/// 校验配置完整性
pub fn validate_config(app_id: &str, app_secret: &str) -> Result<(), Error> {
    if app_id.is_empty() || app_secret.is_empty() {
        return Err(missing_credentials());
    }
    Ok(())
}
