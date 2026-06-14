use common::api::{ToolDetail, ToolListItem};
use serde_json::Value;

use crate::models::tool::Tool;

pub(super) fn to_list_item(tool: &Tool) -> ToolListItem {
    ToolListItem {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: tool.po.description.clone(),
        protocol: tool.po.protocol,
        control_mode: tool.po.control_mode,
        parameters_schema: tool.po.parameters_schema.clone(),
        tags: tool.po.get_tags(),
        status: tool.po.status,
        has_config: has_config(&tool.po.config),
        created_at: tool.po.created_at,
        updated_at: tool.po.updated_at,
    }
}

pub(super) fn to_detail(tool: &Tool) -> ToolDetail {
    ToolDetail {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: tool.po.description.clone(),
        protocol: tool.po.protocol,
        control_mode: tool.po.control_mode,
        parameters_schema: tool.po.parameters_schema.clone(),
        tags: tool.po.get_tags(),
        status: tool.po.status,
        has_config: has_config(&tool.po.config),
        created_by: tool.po.created_by.clone(),
        updated_by: tool.po.updated_by.clone(),
        created_at: tool.po.created_at,
        updated_at: tool.po.updated_at,
    }
}

fn has_config(config: &Value) -> bool {
    !config.is_null()
        && !matches!(config, Value::Object(map) if map.is_empty())
        && !matches!(config, Value::String(value) if value.is_empty())
}
