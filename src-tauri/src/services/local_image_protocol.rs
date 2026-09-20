use serde::Serialize;

use crate::{
    models::app_error::AppError,
    services::image_load_service::{PreviewImageBatch, PreviewImageResource},
};

const METADATA_LENGTH_BYTES: usize = size_of::<u32>();
const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalImageBatchMetadata<'a> {
    entries: Vec<LocalImageEntryMetadata<'a>>,
    resources: Vec<LocalImageResourceMetadata<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalImageEntryMetadata<'a> {
    target: &'a str,
    resource_index: Option<usize>,
    error: Option<&'a AppError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalImageResourceMetadata<'a> {
    mime: &'a str,
    path: &'a str,
    width: usize,
    height: usize,
    encoded_bytes: usize,
    decoded_bytes: u64,
    offset: usize,
}

pub(crate) fn encode_response(batch: &PreviewImageBatch) -> Result<Vec<u8>, AppError> {
    let mut offset = 0_usize;
    let resources = batch
        .resources
        .iter()
        .map(|resource| {
            let metadata = resource_metadata(resource, offset);
            offset = offset.saturating_add(resource.image.bytes.len());
            metadata
        })
        .collect();
    let entries = batch
        .entries
        .iter()
        .map(|entry| LocalImageEntryMetadata {
            target: &entry.target,
            resource_index: entry.resource_index,
            error: entry.error.as_ref(),
        })
        .collect();
    let metadata = serde_json::to_vec(&LocalImageBatchMetadata { entries, resources })
        .map_err(|error| AppError::new("IMAGE_RESPONSE_FAILED", error.to_string()))?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(AppError::new(
            "IMAGE_RESPONSE_FAILED",
            "图片元数据超过协议上限",
        ));
    }
    let metadata_length = u32::try_from(metadata.len())
        .map_err(|_| AppError::new("IMAGE_RESPONSE_FAILED", "图片元数据超过协议上限"))?;
    let payload_length = batch
        .resources
        .iter()
        .map(|resource| resource.image.bytes.len())
        .sum::<usize>();
    let mut body = Vec::with_capacity(METADATA_LENGTH_BYTES + metadata.len() + payload_length);
    body.extend_from_slice(&metadata_length.to_le_bytes());
    body.extend_from_slice(&metadata);
    for resource in &batch.resources {
        body.extend_from_slice(&resource.image.bytes);
    }
    Ok(body)
}

fn resource_metadata(
    resource: &PreviewImageResource,
    offset: usize,
) -> LocalImageResourceMetadata<'_> {
    LocalImageResourceMetadata {
        mime: resource.image.mime,
        path: &resource.image.path,
        width: resource.width,
        height: resource.height,
        encoded_bytes: resource.image.bytes.len(),
        decoded_bytes: resource.decoded_bytes,
        offset,
    }
}
