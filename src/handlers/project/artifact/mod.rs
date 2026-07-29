//! Artifact 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

mod create_artifact;
mod create_text_artifact;
mod delete_artifact;
mod get_artifact;
mod get_artifact_content;
mod list_artifacts;
mod query_artifacts;
mod register_artifact_from_path;
mod response;
mod mime_util;
mod update_artifact;

pub use create_artifact::create_artifact_handler;
pub use create_text_artifact::create_text_artifact_handler;
pub use delete_artifact::delete_artifact_handler;
pub use get_artifact::get_artifact_handler;
pub use get_artifact_content::get_artifact_content_handler;
pub use list_artifacts::list_artifacts_handler;
pub use register_artifact_from_path::register_artifact_from_path_handler;
pub use update_artifact::update_artifact_handler;
