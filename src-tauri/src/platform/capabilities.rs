use crate::models::{app_error::AppError, export::ExportFormat};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub platform: &'static str,
    pub desktop_files: bool,
    pub document_uris: bool,
    pub export_formats: Vec<ExportFormat>,
}

pub fn current() -> PlatformCapabilities {
    let desktop = cfg!(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    ));
    PlatformCapabilities {
        platform: std::env::consts::OS,
        desktop_files: desktop,
        document_uris: cfg!(target_os = "android"),
        export_formats: if desktop || cfg!(target_os = "android") {
            vec![
                ExportFormat::Html,
                ExportFormat::Pdf,
                ExportFormat::Docx,
                ExportFormat::Svg,
                ExportFormat::Png,
            ]
        } else {
            vec![]
        },
    }
}

impl PlatformCapabilities {
    pub fn require_export(&self, format: ExportFormat) -> Result<(), AppError> {
        if self.export_formats.contains(&format) {
            return Ok(());
        }
        Err(AppError::new(
            "CAPABILITY_UNAVAILABLE",
            format!("当前平台不支持 {} 导出", format.extension()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_platform_does_not_claim_export_support() {
        let unavailable = PlatformCapabilities {
            platform: "android",
            desktop_files: false,
            document_uris: false,
            export_formats: vec![],
        };
        assert_eq!(
            unavailable
                .require_export(ExportFormat::Pdf)
                .unwrap_err()
                .code,
            "CAPABILITY_UNAVAILABLE"
        );
        if cfg!(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )) {
            for format in [
                ExportFormat::Html,
                ExportFormat::Pdf,
                ExportFormat::Docx,
                ExportFormat::Svg,
                ExportFormat::Png,
            ] {
                current().require_export(format).unwrap();
            }
        }
    }
}
