use reqwest::Error;

/// 调用后端健康检查接口
pub async fn fetch_health() -> Result<String, String> {
    // 从编译时环境变量读取后端 API 地址，默认 http://localhost:3000
    let backend_url = option_env!("BACKEND_API_URL").unwrap_or("http://localhost:3000");
    let url = format!("{}/health", backend_url);

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
