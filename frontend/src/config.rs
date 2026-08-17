//! 前端配置管理
//!
//! 配置优先级：
//! 1. localStorage 中用户保存的配置（最高优先级）
//! 2. 编译时嵌入的默认配置（从后端 ai_orz.toml 读取）

use serde::{Deserialize, Serialize};

use crate::utils::local_storage;

/// 前端可配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendConfig {
    /// 后端 API 基础地址
    /// 例如: http://localhost:3000, https://api.example.com
    pub api_base_url: String,
}

impl Default for FrontendConfig {
    fn default() -> Self {
        // 前端由后端静态托管，页面与 API 同源：优先使用浏览器当前 origin，
        // 避免编译期嵌入的 listen_addr 与运行时实际端口不一致时请求打偏
        // （如部署改端口、E2E 隔离端口等场景）
        if let Some(origin) = web_sys::window().and_then(|w| w.location().origin().ok()) {
            return Self {
                api_base_url: origin,
            };
        }

        // 无 window 环境（单元测试等）回退编译时嵌入的配置
        let compiled_config = crate::get_config();

        let mut listen_addr = compiled_config.server.listen_addr.clone();

        // 将 0.0.0.0 替换为 localhost，确保浏览器可访问
        if listen_addr.starts_with("0.0.0.0:") {
            listen_addr = listen_addr.replace("0.0.0.0:", "localhost:");
        }

        let api_base_url =
            if listen_addr.starts_with("http://") || listen_addr.starts_with("https://") {
                listen_addr
            } else {
                format!("http://{}", listen_addr)
            };

        Self { api_base_url }
    }
}

impl FrontendConfig {
    pub fn load() -> Self {
        if let Some(storage) = local_storage() {
            match storage.get("ai_orz_config") {
                Ok(json_opt) => {
                    if let Some(json) = json_opt {
                        serde_json::from_str(&json).unwrap_or_default()
                    } else {
                        Self::default()
                    }
                }
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(storage) = local_storage() {
            let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
            storage
                .set("ai_orz_config", &json)
                .map_err(|e| format!("{:?}", e))?;
            Ok(())
        } else {
            Err("localStorage not available".to_string())
        }
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }

    /// 解除用户保存的配置：删除 localStorage 键，回到 origin 动态探测。
    ///
    /// 与 `reset_to_default` + `save` 的区别：后者会把「点击瞬间的 origin 快照」持久化，
    /// 换环境访问（如换机器/换域名）仍被旧快照粘住；删除键才能恢复真正的默认行为。
    pub fn clear_saved(&self) -> Result<(), String> {
        if let Some(storage) = local_storage() {
            storage
                .remove_item("ai_orz_config")
                .map_err(|e| format!("{:?}", e))?;
        }
        Ok(())
    }

    pub fn api_url(&self, path: &str) -> String {
        let base = self.api_base_url.trim_end_matches('/');
        format!("{}{}", base, path)
    }
}

pub fn current_config() -> FrontendConfig {
    FrontendConfig::load()
}
