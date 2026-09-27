use crate::models::{app_error::AppError, export::ExportRequest, resource::ResourceRef};

/// Legacy export paths are a desktop projection, never an alternate URI interpretation.
pub(crate) fn validate(
    request: &ExportRequest,
    source: Option<&ResourceRef>,
    target: Option<&ResourceRef>,
) -> Result<(), AppError> {
    for (resource, expected) in [
        (source, request.snapshot.source_path.as_deref()),
        (target, Some(request.target_path.as_str())),
    ] {
        if let Some(resource) = resource {
            let path = super::resources::path(resource)?;
            if path.to_str() != expected {
                return Err(AppError::new(
                    "INVALID_EXPORT_RESOURCE",
                    "导出资源引用与兼容路径不一致",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mismatched_or_uri_export_targets_are_rejected_before_io() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../src/shared/desktop-contract-fixtures.json"
        ))
        .unwrap();
        let mut request: ExportRequest =
            serde_json::from_value(fixtures["exportRequest"].clone()).unwrap();
        let directory = crate::utils::test_support::TestDirectory::new("export-resource");
        let target = directory.path().join("output.html");
        request.target_path = target.to_str().unwrap().into();
        request.snapshot.source_path = None;
        let resource = ResourceRef::DesktopFile {
            path: request.target_path.clone(),
        };
        validate(&request, None, Some(&resource)).unwrap();
        let mismatch = ResourceRef::DesktopFile {
            path: directory.path().join("other.html").to_str().unwrap().into(),
        };
        assert_eq!(
            validate(&request, None, Some(&mismatch)).unwrap_err().code,
            "INVALID_EXPORT_RESOURCE"
        );
        let uri = ResourceRef::AndroidDocument {
            uri: "content://provider/document/output".into(),
        };
        assert_eq!(
            validate(&request, None, Some(&uri)).unwrap_err().code,
            "RESOURCE_UNSUPPORTED"
        );
        assert_eq!(
            validate(&request, Some(&resource), None).unwrap_err().code,
            "INVALID_EXPORT_RESOURCE"
        );
        assert!(!target.exists());
    }
}
