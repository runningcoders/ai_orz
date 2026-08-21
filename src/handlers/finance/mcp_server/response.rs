use common::api::{McpServerConfigDto, McpServerDetail, McpServerListItem};
use common::enums as api_enums;

use crate::models::mcp_server::{McpServer, McpServerConfig, McpServerStatus, McpTransport};

pub(super) fn to_list_item(server: &McpServer) -> McpServerListItem {
    McpServerListItem {
        id: server.po.id.clone(),
        name: server.po.name.clone(),
        transport: to_api_transport(server.po.transport),
        config: to_config_dto(&server.po.config()),
        status: to_api_status(server.po.status),
        created_by: server.po.created_by.clone(),
        updated_by: server.po.updated_by.clone(),
        created_at: server.po.created_at,
        updated_at: server.po.updated_at,
    }
}

pub(super) fn to_detail(server: &McpServer) -> McpServerDetail {
    McpServerDetail {
        id: server.po.id.clone(),
        name: server.po.name.clone(),
        transport: to_api_transport(server.po.transport),
        config: to_config_dto(&server.po.config()),
        status: to_api_status(server.po.status),
        created_by: server.po.created_by.clone(),
        updated_by: server.po.updated_by.clone(),
        created_at: server.po.created_at,
        updated_at: server.po.updated_at,
    }
}

pub(super) fn to_model_config(config: McpServerConfigDto) -> McpServerConfig {
    McpServerConfig {
        command: config.command,
        args: config.args,
        url: config.url,
        credential_requirements: config.credential_requirements,
        timeout_ms: config.timeout_ms.unwrap_or(30_000),
        connect_timeout_ms: config.connect_timeout_ms.unwrap_or(10_000),
        response_max_bytes: config.response_max_bytes.unwrap_or(10 * 1024 * 1024),
    }
}

pub(super) fn to_config_dto(config: &McpServerConfig) -> McpServerConfigDto {
    McpServerConfigDto {
        command: config.command.clone(),
        args: config.args.clone(),
        url: config.url.clone(),
        credential_requirements: config.credential_requirements.clone(),
        timeout_ms: Some(config.timeout_ms),
        connect_timeout_ms: Some(config.connect_timeout_ms),
        response_max_bytes: Some(config.response_max_bytes),
    }
}

pub(super) fn to_model_transport(transport: api_enums::McpTransport) -> McpTransport {
    match transport {
        api_enums::McpTransport::Stdio => McpTransport::Stdio,
        api_enums::McpTransport::StreamableHttp => McpTransport::StreamableHttp,
    }
}

pub(super) fn to_api_transport(transport: McpTransport) -> api_enums::McpTransport {
    match transport {
        McpTransport::Stdio => api_enums::McpTransport::Stdio,
        McpTransport::StreamableHttp => api_enums::McpTransport::StreamableHttp,
    }
}

pub(super) fn to_model_status(status: api_enums::McpServerStatus) -> McpServerStatus {
    match status {
        api_enums::McpServerStatus::Deleted => McpServerStatus::Deleted,
        api_enums::McpServerStatus::Enabled => McpServerStatus::Enabled,
        api_enums::McpServerStatus::Disabled => McpServerStatus::Disabled,
    }
}

pub(super) fn to_api_status(status: McpServerStatus) -> api_enums::McpServerStatus {
    match status {
        McpServerStatus::Deleted => api_enums::McpServerStatus::Deleted,
        McpServerStatus::Enabled => api_enums::McpServerStatus::Enabled,
        McpServerStatus::Disabled => api_enums::McpServerStatus::Disabled,
    }
}
