use std::collections::HashSet;

use tauri_plugin_marklite_mobile::MarkliteMobileExt;

use crate::{
    models::{app_error::AppError, diagram_assets::PreparedDiagrams},
    services::{
        diagram_export_service::{self, BrowserResult, DiagramExportMode},
        diagram_runtime_service,
        export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind},
        export_semantic::SemanticDocument,
    },
};

pub(crate) async fn prepare(
    app: &tauri::AppHandle,
    document: &SemanticDocument,
    mode: DiagramExportMode,
    reporter: &ExportReporter,
) -> Result<PreparedDiagrams, AppError> {
    let (sources, mut warnings) = diagram_export_service::inspect(document);
    if sources.is_empty() {
        return Ok(PreparedDiagrams::empty());
    }
    let invalid: HashSet<_> = warnings
        .iter()
        .filter_map(|warning| warning.target.as_deref())
        .filter_map(|target| target.split(':').next())
        .collect();
    let valid: Vec<_> = sources
        .iter()
        .filter(|source| !invalid.contains(source.diagram_id.as_str()))
        .cloned()
        .collect();
    if valid.len() > 64 {
        return Err(AppError::new(
            "DIAGRAM_OUTPUT_TOO_LARGE",
            "Android 单次导出最多渲染 64 个 Mermaid 图表",
        ));
    }
    if valid.is_empty() {
        let mut empty = PreparedDiagrams::empty();
        empty.warnings = warnings;
        return Ok(empty);
    }
    let runtime = diagram_runtime_service::load_for_current_platform(&super::app_data_dir()?)?;
    let per_batch = if matches!(mode, DiagramExportMode::Docx) {
        2
    } else {
        4
    };
    let total = valid.len().div_ceil(per_batch) as u32;
    let mut prepared = PreparedDiagrams::empty();
    reporter.phase(ExportStage::Resources);
    for (index, batch) in valid.chunks(per_batch).enumerate() {
        reporter.processing(
            ExportStage::Rendering,
            ExportWork {
                kind: WorkKind::Part,
                index: index as u32 + 1,
                total,
                completed: index as u32,
            },
        );
        let page = diagram_export_service::render_page(&runtime.script_utf8, batch, mode)?;
        let app_for_render = app.clone();
        let response = tauri::async_runtime::spawn_blocking(move || {
            app_for_render
                .marklite_mobile()
                .render_export_diagrams(&page)
                .map_err(|error| AppError::new("DIAGRAM_RUNTIME_CRASHED", error.to_string()))
        })
        .await
        .map_err(|_| AppError::new("DIAGRAM_RUNTIME_CRASHED", "Android 图表渲染任务异常结束"))??;
        let chunk = match diagram_export_service::parse_browser_result(&response.result)? {
            BrowserResult::Ready {
                artifacts,
                errors,
                print_svgs,
                rasters,
                raster_errors,
            } => diagram_export_service::validate_batch(
                batch,
                artifacts,
                errors,
                print_svgs,
                rasters,
                raster_errors,
                mode,
            )?,
            BrowserResult::Failed { message } => {
                return Err(AppError::new("DIAGRAM_RUNTIME_CRASHED", message));
            }
            BrowserResult::Pending => {
                return Err(AppError::new(
                    "DIAGRAM_RUNTIME_CRASHED",
                    "Android 图表渲染尚未完成",
                ));
            }
        };
        for (key, value) in chunk.artifacts {
            if prepared.artifacts.insert(key, value).is_some() {
                return Err(AppError::new("DIAGRAM_RUNTIME_CRASHED", "重复的图表身份"));
            }
        }
        prepared.print_artifacts.extend(chunk.print_artifacts);
        prepared.rasters.extend(chunk.rasters);
        prepared.warnings.extend(chunk.warnings);
        let svg_bytes: usize = prepared
            .artifacts
            .values()
            .map(|item| item.svg_utf8.len())
            .sum();
        let raster_bytes: usize = prepared.rasters.values().map(|item| item.png.len()).sum();
        if svg_bytes > 32 * 1024 * 1024 || raster_bytes > 32 * 1024 * 1024 {
            return Err(AppError::new(
                "DIAGRAM_OUTPUT_TOO_LARGE",
                "Android 图表产物超过 32 MiB 预算",
            ));
        }
    }
    warnings.append(&mut prepared.warnings);
    prepared.warnings = warnings;
    Ok(prepared)
}
