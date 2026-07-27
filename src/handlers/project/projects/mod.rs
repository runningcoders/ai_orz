//! Project 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

mod create_project;
mod get_project;
mod list_projects;
mod query_projects;
mod response;
mod update_project;
mod update_project_status;

pub use create_project::create_project_handler;
pub use get_project::get_project_handler;
pub use list_projects::list_projects_handler;
pub use query_projects::query_projects_handler;
pub use update_project::update_project_handler;
pub use update_project_status::update_project_status_handler;
