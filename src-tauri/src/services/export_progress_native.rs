//! CLI and Shell presentation consume the same events; they do not own exports.
use super::export_progress::{ExportProgress, ExportStage, ProgressStatus, WorkKind};

pub fn progress_text(event: &ExportProgress) -> String {
    let stage = match event.status {
        ProgressStatus::Succeeded => "Export completed",
        ProgressStatus::Failed => "Export failed",
        ProgressStatus::Cancelled => "Export cancelled",
        _ => match event.stage {
            ExportStage::Loading => "Loading document",
            ExportStage::Snapshot => "Preparing snapshot",
            ExportStage::PreparingTarget => "Preparing destination",
            ExportStage::ChoosingTarget => "Waiting for destination",
            ExportStage::Parsing => "Parsing document",
            ExportStage::Resources => "Preparing resources",
            ExportStage::Rendering => "Rendering content",
            ExportStage::Printing => "Printing PDF",
            ExportStage::Encoding => "Encoding output",
            ExportStage::Merging => "Merging PDF parts",
            ExportStage::Validating => "Checking output",
            ExportStage::Writing => "Writing output",
            ExportStage::Committing => "Writing and committing output",
            ExportStage::CleaningUp => "Releasing resources",
            ExportStage::Finalizing => "Confirming result",
        },
    };
    let mut text = format!("{}: {stage}", event.format.extension().to_uppercase());
    if let Some(work) = event.work {
        let kind = match work.kind {
            WorkKind::Chapter => "chapter",
            WorkKind::Part => "part",
        };
        text.push_str(&format!(
            " ({kind} {}/{}, {} completed)",
            work.index, work.total, work.completed
        ));
    }
    text
}

pub fn terminal_event(event: ExportProgress) {
    use std::io::Write;
    let mut stderr = std::io::stderr().lock();
    let _ = write!(stderr, "\r{:<100}", progress_text(&event));
    if matches!(
        event.status,
        ProgressStatus::Succeeded | ProgressStatus::Failed | ProgressStatus::Cancelled
    ) {
        let _ = writeln!(stderr);
    }
    let _ = stderr.flush();
}

#[cfg(all(windows, not(test)))]
pub fn shell_presenter() -> impl FnMut(ExportProgress) {
    use windows::{
        core::{IUnknown, HSTRING},
        Win32::{System::Com::*, UI::Shell::*},
    };
    struct Dialog {
        dialog: Option<IProgressDialog>,
        initialized: bool,
    }
    impl Drop for Dialog {
        fn drop(&mut self) {
            unsafe {
                if let Some(dialog) = self.dialog.take() {
                    let _ = dialog.StopProgressDialog();
                    drop(dialog);
                }
                if self.initialized {
                    CoUninitialize();
                }
            }
        }
    }
    let mut owner = Dialog {
        dialog: None,
        initialized: false,
    };
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() {
            owner.initialized = true;
            if let Ok(dialog) = CoCreateInstance::<_, IProgressDialog>(
                &CLSID_ProgressDialog,
                None::<&IUnknown>,
                CLSCTX_INPROC_SERVER,
            ) {
                let _ = dialog.SetTitle(&HSTRING::from("MarkLite Export"));
                if dialog
                    .StartProgressDialog(
                        None,
                        None::<&IUnknown>,
                        PROGDLG_MARQUEEPROGRESS | PROGDLG_NOCANCEL | PROGDLG_NOTIME,
                        None,
                    )
                    .is_ok()
                {
                    owner.dialog = Some(dialog);
                }
            }
        }
    }
    move |event| {
        if let Some(dialog) = &owner.dialog {
            unsafe {
                let _ = dialog.SetLine(1, &HSTRING::from(progress_text(&event)), false, None);
            }
        }
    }
}
