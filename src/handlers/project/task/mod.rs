//! Task 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

mod create_task;
mod get_task;
mod list_agent_tasks;
mod list_project_tasks;
mod list_tasks;
mod mark_done;
mod response;
mod update_task;
mod update_task_progress;
mod update_task_status;

pub use create_task::create_task_handler;
pub use get_task::get_task_handler;
pub use list_agent_tasks::list_agent_tasks_handler;
pub use list_project_tasks::list_project_tasks_handler;
pub use list_tasks::list_tasks_handler;
pub use mark_done::mark_done_handler;
pub use update_task::update_task_handler;
pub use update_task_progress::update_task_progress_handler;
pub use update_task_status::update_task_status_handler;
