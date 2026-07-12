//! Skill 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_skill;
pub mod delete_skill;
pub mod get_skill;
pub mod get_skill_file_content;
pub mod install_skill_to_agent;
pub mod list_agent_skills;
pub mod list_skill_files;
pub mod list_skills;
pub mod response;
pub mod search_skill;
pub mod search_skills;
pub mod update_skill;
pub mod update_skill_file_content;

pub use create_skill::create_skill_handler;
pub use delete_skill::delete_skill_handler;
pub use get_skill::get_skill_handler;
pub use get_skill_file_content::get_skill_file_content_handler;
pub use install_skill_to_agent::install_skill_to_agent_handler;
pub use list_agent_skills::list_agent_skills_handler;
pub use list_skill_files::list_skill_files_handler;
pub use list_skills::list_skills_handler;
pub use search_skill::search_skill_handler;
pub use search_skills::search_skills_handler;
pub use update_skill::update_skill_handler;
pub use update_skill_file_content::update_skill_file_content_handler;
