//! Artifact 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

mod create_artifact;
mod delete_artifact;
mod get_artifact;
mod get_artifact_content;
mod list_artifacts;
mod response;
mod update_artifact_content;

pub use create_artifact::create_artifact;
pub use delete_artifact::delete_artifact;
pub use get_artifact::get_artifact;
pub use get_artifact_content::get_artifact_content;
pub use list_artifacts::list_artifacts;
pub use update_artifact_content::update_artifact_content;
