use crate::api::{
    CreateMcpServerRequest, ListMcpServersRequest, McpServerConfigDto, McpServerDetail,
    PaginationParams, UpdateMcpServerRequest, UpdateMcpServerStatusRequest,
};
use crate::enums::{McpServerStatus, McpTransport};
use crate::models::{CredentialBinding, CredentialKind, CredentialRequirement};

#[test]
fn mcp_server_api_dtos_support_management_round_trip_shape() {
    let requirements = vec![CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some("linear".to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Env {
            name: "LINEAR_API_TOKEN".to_string(),
        },
    }];

    let create = CreateMcpServerRequest {
        name: "filesystem".to_string(),
        transport: McpTransport::Stdio,
        config: McpServerConfigDto {
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
            ],
            url: None,
            credential_requirements: requirements.clone(),
            timeout_ms: Some(30_000),
            connect_timeout_ms: Some(10_000),
            response_max_bytes: Some(10 * 1024 * 1024),
        },
    };

    assert_eq!(create.transport, McpTransport::Stdio);
    assert_eq!(create.config.credential_requirements, requirements);

    let update = UpdateMcpServerRequest {
        id: "server-1".to_string(),
        name: Some("filesystem-updated".to_string()),
        transport: None,
        config: None,
    };
    assert_eq!(update.id, "server-1");

    let status = UpdateMcpServerStatusRequest {
        id: "server-1".to_string(),
        status: McpServerStatus::Disabled,
    };
    assert_eq!(status.status, McpServerStatus::Disabled);

    let list = ListMcpServersRequest::default();
    assert!(list.pagination.limit.is_none());
    assert!(list.pagination.offset.is_none());

    let list_with_pagination = ListMcpServersRequest {
        pagination: PaginationParams {
            limit: Some(20),
            offset: Some(40),
        },
        ..Default::default()
    };
    let json = serde_json::to_value(&list_with_pagination).unwrap();
    assert_eq!(json.get("limit").and_then(|value| value.as_u64()), Some(20));
    assert_eq!(
        json.get("offset").and_then(|value| value.as_u64()),
        Some(40)
    );

    let detail = McpServerDetail {
        id: "server-1".to_string(),
        name: create.name,
        transport: create.transport,
        config: create.config,
        status: McpServerStatus::Enabled,
        created_by: Some("user-1".to_string()),
        updated_by: Some("user-1".to_string()),
        created_at: 1,
        updated_at: 2,
    };
    assert_eq!(detail.status, McpServerStatus::Enabled);
}
