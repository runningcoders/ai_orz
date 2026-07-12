//! Cron Trigger related API request/response DTOs - shared between backend and frontend

use crate::enums::TriggerType;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create Cron Trigger request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateCronTriggerRequest {
    /// Trigger name
    pub name: String,
    /// Trigger type
    pub trigger_type: TriggerType,
    /// Cron expression (required for Cron type)
    pub cron_expression: Option<String>,
    /// Interval in seconds (required for Interval type)
    pub interval_seconds: Option<i64>,
    /// Run at timestamp (required for Once type)
    pub run_at: Option<i64>,
    /// Trigger payload JSON string
    pub payload: String,
}

/// Update Cron Trigger request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateCronTriggerRequest {
    /// Trigger ID
    #[param(source = "path")]
    pub trigger_id: String,
    /// New trigger name
    pub name: Option<String>,
    /// New trigger type
    pub trigger_type: Option<TriggerType>,
    /// New cron expression
    pub cron_expression: Option<String>,
    /// New interval in seconds
    pub interval_seconds: Option<i64>,
    /// New run at timestamp
    pub run_at: Option<i64>,
    /// New trigger payload JSON string
    pub payload: Option<String>,
}

/// Cron Trigger detail
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CronTriggerDetail {
    /// Trigger ID
    pub id: String,
    /// Trigger name
    pub name: String,
    /// Trigger type
    pub trigger_type: TriggerType,
    /// Cron expression (for Cron type)
    pub cron_expression: Option<String>,
    /// Interval in seconds (for Interval type)
    pub interval_seconds: Option<i64>,
    /// Run at timestamp (for Once type)
    pub run_at: Option<i64>,
    /// Next run at timestamp
    pub next_run_at: i64,
    /// Whether the trigger is enabled
    pub is_enabled: bool,
    /// Trigger payload JSON string
    pub payload: String,
    /// Last run at timestamp
    pub last_run_at: Option<i64>,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
    /// Creator user ID
    pub created_by: Option<String>,
    /// Last modifier user ID
    pub updated_by: Option<String>,
}

/// List Cron Triggers response
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListCronTriggersResponse {
    /// List of triggers
    pub triggers: Vec<CronTriggerDetail>,
    /// Total count
    pub total: usize,
}

/// Cron Trigger list item alias (frontend compatibility)
pub type ListCronTriggersResponseItem = CronTriggerDetail;

/// Create Cron Trigger response
pub type CreateCronTriggerResponse = CronTriggerDetail;

/// Get Cron Trigger response
pub type GetCronTriggerResponse = CronTriggerDetail;

/// Update Cron Trigger response
pub type UpdateCronTriggerResponse = CronTriggerDetail;

/// Delete Cron Trigger response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteCronTriggerResponse {
    /// Whether deletion succeeded
    pub success: bool,
}
