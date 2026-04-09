//! 前端配置管理
//!
//! 配置优先级：
//! 1. localStorage 中用户保存的配置（最高优先级）
//! 2. 编译时嵌入的默认配置（从后端 ai_orz.toml 读取）

use serde::{Deserialize, Serialize};
use web_sys::Storage;

/// 前端可配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendConfig {
    /// 后端 API 基础地址
    /// 例如: http://localhost:3000, https://api.example.com
    pub api_base_url: String,
}

impl Default for FrontendConfig {
    fn default() -> Self {
        // 使用编译时嵌入的配置生成默认值
        let compiled_config = crate::get_config();

        // 从编译后的服务器配置生成 API 基础地址
        // listen_addr 格式: "0.0.0.0:3000" 或者 "localhost:3000"
        let listen_addr = compiled_config.server.listen_addr.clone();

        // 如果 listen_addr 不包含域名，默认用 http 协议
        let api_base_url = if listen_addr.starts_with("http://") || listen_addr.starts_with("https://") {
            listen_addr
        } else {
            format!("http://{}", listen_addr)
        };

        Self {
            api_base_url,
        }
    }
}

impl FrontendConfig {
    /// 从 localStorage 加载配置，没有则返回默认值
    pub fn load() -> Self {
        if let Some(storage) = get_local_storage() {
            match storage.get("ai_orz_config") {
                Ok(json_opt) => {
                    if let Some(json) = json_opt {
                        match serde_json::from_str(&json) {
                            Ok(config) => return config,
                            Err(_) => Self::default(),
                        }
                    } else {
                        Self::default()
                    }
                },
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    /// 保存配置到 localStorage
    pub fn save(&self) -> Result<(), String> {
        if let Some(storage) = get_local_storage() {
            let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
            storage.set("ai_orz_config", &json).map_err(|e| format!("{:?}", e))?;
            Ok(())
        } else {
            Err("localStorage not available".to_string())
        }
    }

    /// 重置为编译时默认配置
    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }

    /// 获取完整的 API 地址
    pub fn api_url(&self, path: &str) -> String {
        // path 应该以 / 开头，例如: /api/v1/health
        let base = self.api_base_url.trim_end_matches('/');
        format!("{}{}", base, path)
    }
}

fn get_local_storage() -> Option<Storage> {
    let window = web_sys::window()?;
    match window.local_storage() {
        Ok(opt) => opt,
        Err(_) => None,
    }
}

/// 获取当前全局配置
/// 在 Dioxus 组件中使用 use_context 或 use_signal 管理
pub fn current_config() -> FrontendConfig {
    FrontendConfig::load()
}
