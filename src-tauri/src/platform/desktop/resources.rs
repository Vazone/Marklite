use std::path::Path;

use crate::models::{app_error::AppError, resource::ResourceRef};

/// Explicit resource dispatch; never resolves Android content URIs as disk paths.
pub fn path(resource: &ResourceRef) -> Result<&Path, AppError> {
    match resource {
        ResourceRef::DesktopFile { path } => absolute_path(path),
        _ => Err(AppError::new(
            "RESOURCE_UNSUPPORTED",
            "桌面适配不支持此资源类型",
        )),
    }
}

pub fn absolute_path(value: &str) -> Result<&Path, AppError> {
    let path = Path::new(value);
    if value.contains('\0') || !path.is_absolute() {
        return Err(AppError::invalid_file_path(value));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_uri_and_relative_paths_without_conversion() {
        let resource = ResourceRef::AndroidDocument {
            uri: "content://provider/document/a.md".into(),
        };
        assert_eq!(path(&resource).unwrap_err().code, "RESOURCE_UNSUPPORTED");
        assert_eq!(
            crate::services::file_service::read_resource(&resource)
                .unwrap_err()
                .code,
            "RESOURCE_UNSUPPORTED"
        );
        for value in [
            "content://provider/a.md",
            "file:///notes/a.md",
            "relative.md",
            "",
        ] {
            assert!(absolute_path(value).is_err());
        }
        let directory = crate::utils::test_support::TestDirectory::new("resource-path");
        let native = directory.path().join("100% #计划.md");
        assert_eq!(absolute_path(native.to_str().unwrap()).unwrap(), native);
    }
}
