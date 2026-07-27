//! Project test factory — creates projects via the real `/project/projects` HTTP endpoint.

use crate::common::app::TestApp;
use serde_json::json;

/// Create a test project via the HTTP API. Returns the project ID.
#[allow(dead_code)] // 公共测试 API，保留供未来测试使用
pub async fn create_test_project(app: &TestApp, jwt: &str, name: &str) -> String {
    let req = json!({
        "name": name,
        "description": "Test project",
    });
    let (status, body) = app.post_with_jwt("/api/v1/projects", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in project create response")
        .to_string()
}
