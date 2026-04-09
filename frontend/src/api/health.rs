use crate::config::current_config;

/// 调用后端健康检查接口
pub async fn fetch_health() -> Result<String, String> {
    let config = current_config();
    let url = config.api_url("/health");

    let client = reqwest::Client::new();
    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if response.status().is_success() {
        match response.text().await {
            Ok(text) => Ok(format!("后端服务正常: {}", text)),
            Err(e) => Err(e.to_string()),
        }
    } else {
        Err(format!("HTTP 错误: {}", response.status()))
    }
}
