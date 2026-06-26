use common::api::ArtifactDetail;

use crate::models::artifact::Artifact;

pub(super) fn to_detail(artifact: &Artifact) -> ArtifactDetail {
    ArtifactDetail {
        id: artifact.po.id.clone(),
        project_id: artifact.po.project_id.clone(),
        task_id: artifact.po.task_id.clone(),
        name: artifact.po.name.clone(),
        description: artifact.po.description.clone(),
        file_type: artifact.po.file_type,
        source_type: artifact.po.source_type,
        file_path: artifact.po.file_meta.0.file_path.clone(),
        mime_type: artifact.po.file_meta.0.mime_type.clone(),
        file_size: artifact.po.file_meta.0.file_size,
        tags: artifact.tags(),
        status: artifact.po.status,
        created_by: artifact.po.created_by.clone(),
        modified_by: artifact.po.modified_by.clone(),
        created_at: artifact.po.created_at,
        updated_at: artifact.po.updated_at,
    }
}
