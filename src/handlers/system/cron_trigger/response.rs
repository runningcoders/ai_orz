use ai_orz_macros::Params;
use common::enums::TriggerType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::cron_trigger::CronTriggerPo;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateCronTriggerRequest {
    pub name: String,
    pub trigger_type: TriggerType,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub run_at: Option<i64>,
    pub payload: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetCronTriggerRequest {
    #[param(source = "path")]
    pub trigger_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListCronTriggersRequest {
    #[param(source = "query")]
    pub trigger_type: Option<TriggerType>,
    #[param(source = "query")]
    pub is_enabled: Option<bool>,
    #[param(source = "query")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateCronTriggerRequest {
    #[param(source = "path")]
    pub trigger_id: String,
    pub name: Option<String>,
    pub trigger_type: Option<TriggerType>,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub run_at: Option<i64>,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteCronTriggerRequest {
    #[param(source = "path")]
    pub trigger_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct PauseCronTriggerRequest {
    #[param(source = "path")]
    pub trigger_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ResumeCronTriggerRequest {
    #[param(source = "path")]
    pub trigger_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CronTriggerDetail {
    pub id: String,
    pub name: String,
    pub trigger_type: TriggerType,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub run_at: Option<i64>,
    pub next_run_at: i64,
    pub is_enabled: bool,
    pub payload: String,
    pub last_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListCronTriggersResponse {
    pub triggers: Vec<CronTriggerDetail>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteCronTriggerResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PauseCronTriggerResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResumeCronTriggerResponse {
    pub success: bool,
}

pub type CreateCronTriggerResponse = CronTriggerDetail;
pub type GetCronTriggerResponse = CronTriggerDetail;
pub type UpdateCronTriggerResponse = CronTriggerDetail;

pub(super) fn to_detail(trigger: &CronTriggerPo) -> CronTriggerDetail {
    CronTriggerDetail {
        id: trigger.id.clone(),
        name: trigger.name.clone(),
        trigger_type: trigger.trigger_type,
        cron_expression: trigger.cron_expression.clone(),
        interval_seconds: trigger.interval_seconds,
        run_at: trigger.run_at,
        next_run_at: trigger.next_run_at,
        is_enabled: trigger.is_enabled == 1,
        payload: trigger.payload.clone(),
        last_run_at: trigger.last_run_at,
        created_at: trigger.created_at,
        updated_at: trigger.updated_at,
        created_by: trigger.created_by.clone(),
        updated_by: trigger.updated_by.clone(),
    }
}
