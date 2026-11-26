//! Cron Trigger 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件。

pub mod cancel_schedule;
pub mod create_cron_trigger;
pub mod delete_cron_trigger;
pub mod get_cron_trigger;
pub mod list_cron_triggers;
pub mod pause_cron_trigger;
pub mod query_schedule;
pub mod query_trigger_exec;
pub mod query_trigger_exec_stats;
pub mod resume_cron_trigger;
pub mod schedule_agent_task;
pub mod update_cron_trigger;
pub mod update_schedule;

mod agent_guard;
mod response;

pub use cancel_schedule::cancel_schedule_handler;
pub use create_cron_trigger::create_cron_trigger_handler;
pub use delete_cron_trigger::delete_cron_trigger_handler;
pub use get_cron_trigger::get_cron_trigger_handler;
pub use list_cron_triggers::list_cron_triggers_handler;
pub use pause_cron_trigger::pause_cron_trigger_handler;
pub use query_schedule::query_schedule_handler;
pub use query_trigger_exec::query_trigger_exec_handler;
pub use query_trigger_exec_stats::query_trigger_exec_stats_handler;
pub use resume_cron_trigger::resume_cron_trigger_handler;
pub use schedule_agent_task::schedule_agent_task_handler;
pub use update_cron_trigger::update_cron_trigger_handler;
pub use update_schedule::update_schedule_handler;
