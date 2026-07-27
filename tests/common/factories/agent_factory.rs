//! Agent test factory — creates agents via the real `/hr/agents` HTTP endpoint.

use crate::common::app::TestApp;
use serde_json::json;

/// Create a test agent via the HTTP API.
///
/// `provider_id` should come from `bootstrap_system` (chat provider id).
/// Returns the created agent's ID.
#[allow(dead_code)] // 公共测试 API，保留供未来测试使用
pub async fn create_test_agent(app: &TestApp, jwt: &str, provider_id: &str, name: &str) -> String {
    let req = json!({
        "name": name,
        "tags": ["test"],
        "description": "Test agent",
        "capabilities": ["chat"],
        "soul": "Test soul",
        "model_provider_id": provider_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/hr/agents", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in agent create response")
        .to_string()
}
