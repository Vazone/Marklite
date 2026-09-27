use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<MarkliteMobile<R>> {
    let handle =
        api.register_android_plugin("com.marklite.editor.mobile", "MarkliteDocumentPlugin")?;
    Ok(MarkliteMobile(handle))
}

/// Access to the marklite-mobile APIs.
pub struct MarkliteMobile<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> MarkliteMobile<R> {
    pub fn set_status_bar_appearance(&self, dark: bool) -> crate::Result<()> {
        self.0
            .run_mobile_plugin(
                "setStatusBarAppearance",
                StatusBarAppearanceRequest { dark },
            )
            .map_err(Into::into)
    }

    pub fn system_insets(&self) -> crate::Result<SystemInsetsResponse> {
        self.0
            .run_mobile_plugin("systemInsets", ())
            .map_err(Into::into)
    }

    pub fn pick_document(&self) -> crate::Result<PickResponse> {
        self.0
            .run_mobile_plugin("pickDocument", ())
            .map_err(Into::into)
    }

    pub fn create_document(&self, title: &str) -> crate::Result<PickResponse> {
        self.0
            .run_mobile_plugin("createDocument", CreateDocumentRequest { title })
            .map_err(Into::into)
    }

    pub fn create_export_document(&self, title: &str, mime: &str) -> crate::Result<PickResponse> {
        self.0
            .run_mobile_plugin(
                "createExportDocument",
                CreateExportDocumentRequest { title, mime },
            )
            .map_err(Into::into)
    }

    pub fn render_export_diagrams(&self, html: &str) -> crate::Result<RenderExportResponse> {
        self.0
            .run_mobile_plugin("renderExportDiagrams", RenderExportRequest { html })
            .map_err(Into::into)
    }

    pub fn capture_export_png(
        &self,
        request: CapturePngRequest<'_>,
    ) -> crate::Result<CapturePngResult> {
        self.0
            .run_mobile_plugin("captureExportPng", request)
            .map_err(Into::into)
    }

    pub fn cancel_capture_png(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("cancelCapturePng", ())
            .map_err(Into::into)
    }

    pub fn draw_export_pdf(&self, request: DrawPdfRequest<'_>) -> crate::Result<DrawPdfResult> {
        self.0
            .run_mobile_plugin("drawExportPdf", request)
            .map_err(Into::into)
    }

    pub fn cancel_draw_pdf(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("cancelDrawPdf", ())
            .map_err(Into::into)
    }

    pub fn begin_image_export(
        &self,
        tree_uri: &str,
        folder_name: &str,
    ) -> crate::Result<ImageUriResult> {
        self.0
            .run_mobile_plugin(
                "beginImageExport",
                BeginImageExportRequest {
                    tree_uri,
                    folder_name,
                },
            )
            .map_err(Into::into)
    }

    pub fn create_image_file(&self, folder_uri: &str, name: &str) -> crate::Result<ImageUriResult> {
        self.0
            .run_mobile_plugin(
                "createImageFile",
                CreateImageFileRequest { folder_uri, name },
            )
            .map_err(Into::into)
    }

    pub fn finish_image_export(&self, folder_uri: &str) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("finishImageExport", ImageFolderRequest { folder_uri })
            .map_err(Into::into)
    }

    pub fn abort_image_export(&self, folder_uri: &str) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("abortImageExport", ImageFolderRequest { folder_uri })
            .map_err(Into::into)
    }

    pub fn pick_tree(&self) -> crate::Result<PickResponse> {
        self.0.run_mobile_plugin("pickTree", ()).map_err(Into::into)
    }

    pub fn inspect_tree(&self, tree_uri: &str) -> crate::Result<TreeName> {
        self.0
            .run_mobile_plugin(
                "inspectTree",
                TreePathRequest {
                    tree_uri,
                    relative_path: "",
                },
            )
            .map_err(Into::into)
    }

    pub fn list_tree(&self, tree_uri: &str, relative_path: &str) -> crate::Result<TreeEntries> {
        self.0
            .run_mobile_plugin(
                "listTree",
                TreePathRequest {
                    tree_uri,
                    relative_path,
                },
            )
            .map_err(Into::into)
    }

    pub fn resolve_tree_file(
        &self,
        tree_uri: &str,
        relative_path: &str,
    ) -> crate::Result<PickResponse> {
        self.0
            .run_mobile_plugin(
                "resolveTreeFile",
                TreePathRequest {
                    tree_uri,
                    relative_path,
                },
            )
            .map_err(Into::into)
    }

    pub fn resolve_tree_image(
        &self,
        tree_uri: &str,
        relative_path: &str,
    ) -> crate::Result<PickResponse> {
        self.0
            .run_mobile_plugin(
                "resolveTreeImage",
                TreePathRequest {
                    tree_uri,
                    relative_path,
                },
            )
            .map_err(Into::into)
    }

    pub fn locate_tree_document(
        &self,
        tree_uri: &str,
        source_uri: &str,
    ) -> crate::Result<TreeLocation> {
        self.0
            .run_mobile_plugin(
                "locateTreeDocument",
                TreeSourceRequest {
                    tree_uri,
                    source_uri,
                },
            )
            .map_err(Into::into)
    }

    pub fn read_image(&self, uri: &str) -> crate::Result<ImageBytes> {
        self.0
            .run_mobile_plugin("readImage", UriRequest { uri })
            .map_err(Into::into)
    }

    pub fn read_document(&self, uri: &str) -> crate::Result<DocumentContents> {
        self.0
            .run_mobile_plugin("readDocument", UriRequest { uri })
            .map_err(Into::into)
    }

    pub fn document_version(&self, uri: &str) -> crate::Result<DocumentVersion> {
        self.0
            .run_mobile_plugin("documentVersion", UriRequest { uri })
            .map_err(Into::into)
    }

    pub fn document_name(&self, uri: &str) -> crate::Result<DocumentName> {
        self.0
            .run_mobile_plugin("documentName", UriRequest { uri })
            .map_err(Into::into)
    }

    pub fn write_document(
        &self,
        request: DocumentWriteRequest<'_>,
    ) -> crate::Result<DocumentWriteResult> {
        self.0
            .run_mobile_plugin("writeDocument", request)
            .map_err(Into::into)
    }

    pub fn commit_export_file(
        &self,
        request: ExportFileRequest<'_>,
    ) -> crate::Result<ExportFileResult> {
        self.0
            .run_mobile_plugin("commitExportFile", request)
            .map_err(Into::into)
    }

    pub fn abort_export_document(&self, uri: &str) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("abortExportDocument", UriRequest { uri })
            .map_err(Into::into)
    }

    pub fn drain_intents(&self) -> crate::Result<IntentUris> {
        self.0
            .run_mobile_plugin("drainIntents", ())
            .map_err(Into::into)
    }
}
