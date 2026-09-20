use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::Value;

use crate::services::{
    image_load_service::{ExportImage, PreviewImageBatch, PreviewImageEntry, PreviewImageResource},
    local_image_protocol,
};

use super::{
    app_error::AppError,
    diagram::DiagramSource,
    document::{DocumentDto, DocumentOperationDto, FileVersionDto},
    export::{ExportFormat, ExportRequest, ExportResult, ExportWarning},
    markdown::{
        DocumentStats, MarkdownAnalysisDto, OutlineItem, RenderedMarkdownDto, SourceBlock,
        VirtualPreviewIndex, VirtualPreviewRenderedSegment, VirtualPreviewSegment,
        VirtualPreviewWindow,
    },
    navigation::MarkdownTargetDto,
    recent::RecentFileDto,
    session::SessionState,
    settings::{AppSettings, ThemeMode},
    startup::{
        FrontendStartupCode, FrontendStartupEventDto, FrontendStartupStage, FrontendStartupStatus,
        StartupDiagnosticsExportDto, StartupReadyDto,
    },
};

const DESKTOP_CONTRACT_FIXTURES: &str =
    include_str!("../../../src/shared/desktop-contract-fixtures.json");

#[test]
fn export_location_contract_matches_shared_fixture() {
    use crate::services::export_location_service::{ExportLocationDocument, ExportPathSuggestion};
    let fixtures: ContractFixtures = serde_json::from_str(DESKTOP_CONTRACT_FIXTURES).unwrap();
    let suggestion = ExportPathSuggestion {
        path: r"C:\exports\new.html".into(),
        warning: Some(AppError::new(
            "EXPORT_LOCATION_READ_FAILED",
            "fixture warning",
        )),
    };
    assert_eq!(
        serde_json::to_value(suggestion).unwrap(),
        fixtures.export_path_suggestion
    );
    let state = ExportLocationDocument {
        version: 1,
        directory: r"C:\exports".into(),
    };
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        fixtures.export_location_persistence
    );
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContractFixtures {
    export_progress: crate::services::export_progress::ExportProgress,
    document_operation: Value,
    untitled_document: Value,
    file_version: Value,
    recent_persistence: Value,
    recent_response: Value,
    session: Value,
    startup_event: Value,
    startup_ready: Value,
    app_error: Value,
    rendered_markdown: Value,
    virtual_preview_index: Value,
    virtual_preview_window: Value,
    markdown_analysis: Value,
    markdown_targets: Vec<Value>,
    export_request: Value,
    export_result: Value,
    export_path_suggestion: Value,
    export_location_persistence: Value,
    local_image_response: Value,
    startup_diagnostics_export: Value,
}

fn fixtures() -> ContractFixtures {
    serde_json::from_str(DESKTOP_CONTRACT_FIXTURES).expect("shared desktop fixture must be valid")
}

#[test]
fn shared_fixture_round_trips_document_operation_and_explicit_nulls() {
    let fixtures = fixtures();
    let document: DocumentDto =
        serde_json::from_value(fixtures.document_operation["document"].clone()).unwrap();
    let operation = DocumentOperationDto {
        document,
        auxiliary_error: Some(AppError::new(
            fixtures.document_operation["auxiliaryError"]["code"]
                .as_str()
                .unwrap(),
            fixtures.document_operation["auxiliaryError"]["message"]
                .as_str()
                .unwrap(),
        )),
    };
    assert_eq!(
        serde_json::to_value(operation).unwrap(),
        fixtures.document_operation
    );

    let document: DocumentDto = serde_json::from_value(fixtures.untitled_document.clone()).unwrap();
    assert_eq!(document.path, None);
    assert_eq!(document.file_identity, None);
    assert_eq!(document.content_version, None);
    assert_eq!(
        serde_json::to_value(document).unwrap(),
        fixtures.untitled_document
    );

    let version: FileVersionDto = serde_json::from_value(fixtures.file_version.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(version).unwrap(),
        fixtures.file_version
    );
}

#[test]
fn shared_fixture_round_trips_recent_session_and_startup_contracts() {
    let fixtures = fixtures();
    let recent: Vec<RecentFileDto> =
        serde_json::from_value(fixtures.recent_response.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(recent).unwrap(),
        fixtures.recent_response
    );

    let session: SessionState = serde_json::from_value(fixtures.session.clone()).unwrap();
    assert_eq!(session.active_path, None);
    assert_eq!(serde_json::to_value(session).unwrap(), fixtures.session);

    let event: FrontendStartupEventDto =
        serde_json::from_value(fixtures.startup_event.clone()).unwrap();
    assert!(matches!(event.stage, FrontendStartupStage::FrontendEntry));
    assert!(matches!(event.status, FrontendStartupStatus::Failed));
    assert!(matches!(
        event.code,
        Some(FrontendStartupCode::UnhandledError)
    ));
    assert_eq!(event.elapsed_ms, 12);

    let ready = StartupReadyDto {
        launch_id: fixtures.startup_ready["launchId"]
            .as_str()
            .unwrap()
            .to_string(),
        webview_version: fixtures.startup_ready["webviewVersion"]
            .as_str()
            .map(str::to_string),
    };
    assert_eq!(serde_json::to_value(ready).unwrap(), fixtures.startup_ready);
}

#[test]
fn shared_fixture_freezes_recent_persistence_wrapper_and_error_shape() {
    let fixtures = fixtures();
    assert_eq!(fixtures.recent_persistence["version"], 1);
    let files: Vec<RecentFileDto> =
        serde_json::from_value(fixtures.recent_persistence["files"].clone()).unwrap();
    assert_eq!(files.len(), 1);

    assert_eq!(
        serde_json::to_value(AppError::unsupported_path_encoding()).unwrap(),
        fixtures.app_error
    );
}

#[test]
fn strict_requests_reject_misspelled_or_unknown_fields() {
    let fixtures = fixtures();
    let mut session = fixtures.session.as_object().unwrap().clone();
    session.remove("activePath");
    session.insert("active_path".to_string(), Value::Null);
    assert!(serde_json::from_value::<SessionState>(session.into()).is_err());

    let mut event = fixtures.startup_event.as_object().unwrap().clone();
    event.insert("details".to_string(), Value::String("unknown".to_string()));
    assert!(serde_json::from_value::<FrontendStartupEventDto>(event.into()).is_err());

    let value = serde_json::to_value(AppSettings::default()).unwrap();
    assert_eq!(value["theme"], "system");
    assert!(value.get("accentColor").is_some());
    let mut invalid = value.as_object().unwrap().clone();
    invalid.insert(
        "accent_colour".to_string(),
        Value::String("#000000".to_string()),
    );
    assert!(serde_json::from_value::<AppSettings>(invalid.into()).is_err());

    let theme: ThemeMode = serde_json::from_value(Value::String("dark".to_string())).unwrap();
    assert_eq!(theme, ThemeMode::Dark);

    let mut export = fixtures.export_request.as_object().unwrap().clone();
    let target_path = export.remove("targetPath").unwrap();
    export.insert("target_path".to_string(), target_path);
    assert!(serde_json::from_value::<ExportRequest>(export.into()).is_err());
}

#[test]
fn shared_fixture_round_trips_render_navigation_export_and_diagnostics() {
    let fixtures = fixtures();
    let rendered = RenderedMarkdownDto {
        html: "<h1 id=\"contract\">Contract</h1>".to_string(),
        outline: vec![OutlineItem {
            level: 1,
            title: "Contract".to_string(),
            line: 1,
            slug: "contract".to_string(),
        }],
        stats: DocumentStats {
            word_count: 1,
            character_count: 10,
            line_count: 1,
            heading_count: 1,
            link_count: 0,
            image_count: 0,
        },
        source_blocks: vec![SourceBlock {
            start_utf16: 0,
            end_utf16: 10,
            start_line: 1,
            end_line: 1,
        }],
        diagrams: vec![DiagramSource {
            diagram_id: "diagram-0-bd82be55b98e".to_string(),
            ordinal: 0,
            source_utf8: "flowchart TD\nA-->B\n".to_string(),
            source_sha256: "bd82be55b98e9030585603754372f5e90ae47e48336f0f05284d4290f0cd52f8"
                .to_string(),
            source_start_byte: 11,
            source_end_byte: 30,
        }],
        diagram_diagnostics: Vec::new(),
        virtual_preview: None,
    };
    assert_eq!(
        serde_json::to_value(rendered).unwrap(),
        fixtures.rendered_markdown
    );
    let analysis = MarkdownAnalysisDto {
        outline: vec![OutlineItem {
            level: 1,
            title: "Contract".to_string(),
            line: 1,
            slug: "contract".to_string(),
        }],
        stats: DocumentStats {
            word_count: 1,
            character_count: 10,
            line_count: 1,
            heading_count: 1,
            link_count: 0,
            image_count: 0,
        },
    };
    assert_eq!(
        serde_json::to_value(analysis).unwrap(),
        fixtures.markdown_analysis
    );

    let targets = vec![
        MarkdownTargetDto::Anchor {
            fragment: "contract".to_string(),
        },
        MarkdownTargetDto::LocalDocument {
            path: r"C:\notes\other.md".to_string(),
            fragment: Some("section".to_string()),
        },
        MarkdownTargetDto::External {
            url: "https://example.com/docs".to_string(),
        },
        MarkdownTargetDto::Email {
            address: "writer@example.com".to_string(),
        },
    ];
    assert_eq!(
        targets
            .into_iter()
            .map(|target| serde_json::to_value(target).unwrap())
            .collect::<Vec<_>>(),
        fixtures.markdown_targets
    );

    let request: ExportRequest = serde_json::from_value(fixtures.export_request.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        fixtures.export_request
    );
    let result = ExportResult {
        job_id: "job-1".to_string(),
        format: ExportFormat::Pdf,
        path: r"C:\exports\contract.pdf".to_string(),
        target_kind: Default::default(),
        warnings: vec![ExportWarning::new(
            "LOCAL_IMAGE_SKIPPED",
            "一张本地图片未嵌入",
            Some("missing.png".to_string()),
        )],
    };
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        fixtures.export_result
    );

    let diagnostics = StartupDiagnosticsExportDto { record_count: 4 };
    assert_eq!(
        serde_json::to_value(diagnostics).unwrap(),
        fixtures.startup_diagnostics_export
    );
}

#[test]
fn shared_fixture_freezes_virtual_preview_index_and_window() {
    let fixtures = fixtures();
    let index = VirtualPreviewIndex {
        session_id: "session-7".into(),
        segments: vec![
            VirtualPreviewSegment {
                start_utf16: 0,
                end_utf16: 16,
                start_line: 1,
                end_line: 2,
                estimated_height: 64,
                estimated_nodes: 2,
            },
            VirtualPreviewSegment {
                start_utf16: 16,
                end_utf16: 32,
                start_line: 3,
                end_line: 4,
                estimated_height: 80,
                estimated_nodes: 3,
            },
        ],
    };
    assert_eq!(
        serde_json::to_value(index).unwrap(),
        fixtures.virtual_preview_index
    );
    let window = VirtualPreviewWindow {
        session_id: "session-7".into(),
        start: 0,
        end: 1,
        segments: vec![VirtualPreviewRenderedSegment {
            index: 0,
            html: "<p>First</p>".into(),
            source_block_start: 0,
            source_blocks: vec![SourceBlock {
                start_utf16: 0,
                end_utf16: 16,
                start_line: 1,
                end_line: 2,
            }],
            diagrams: vec![],
            diagram_diagnostics: vec![],
        }],
    };
    assert_eq!(
        serde_json::to_value(window).unwrap(),
        fixtures.virtual_preview_window
    );
}

#[test]
fn shared_fixture_freezes_the_production_local_image_binary_envelope() {
    let fixtures = fixtures();
    let fixture = fixtures.local_image_response;
    let metadata = &fixture["metadata"];
    let payload = STANDARD
        .decode(fixture["payloadBase64"].as_str().unwrap())
        .unwrap();
    let expected = STANDARD
        .decode(fixture["responseBase64"].as_str().unwrap())
        .unwrap();

    let resource = &metadata["resources"][0];
    let entry = &metadata["entries"][0];
    assert_eq!(resource["mime"], "image/png");
    let encoded = local_image_protocol::encode_response(&PreviewImageBatch {
        entries: vec![PreviewImageEntry {
            target: entry["target"].as_str().unwrap().to_string(),
            resource_index: Some(0),
            error: None,
        }],
        resources: vec![PreviewImageResource {
            image: ExportImage {
                bytes: payload,
                mime: "image/png",
                path: resource["path"].as_str().unwrap().to_string(),
            },
            width: resource["width"].as_u64().unwrap() as usize,
            height: resource["height"].as_u64().unwrap() as usize,
            decoded_bytes: resource["decodedBytes"].as_u64().unwrap(),
        }],
    })
    .unwrap();
    assert_eq!(encoded, expected);
}

#[test]
fn shared_fixture_deserializes_export_progress_protocol() {
    let event = fixtures().export_progress;
    assert_eq!(event.job_id, "export-progress-fixture");
    assert_eq!(event.work.unwrap().completed, 1);
}
