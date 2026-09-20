use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use super::export_progress::{ExportReporter, ExportStage, ExportWork};
use super::png_artifact::{validate_dimensions, IMAGE_WIDTH, MAX_IMAGE_BYTES};
use crate::models::app_error::AppError;
use serde::Deserialize;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const READY_TIMEOUT: Duration = Duration::from_secs(20);
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
enum ControlState {
    #[default]
    Running,
    Cancelled,
    Committed,
}
#[derive(Clone, Default)]
pub(crate) struct CaptureControl(Arc<Mutex<ControlState>>);
impl CaptureControl {
    pub(crate) fn cancel(&self) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*state, ControlState::Running) {
            *state = ControlState::Cancelled;
            true
        } else {
            false
        }
    }
    pub(crate) fn check(&self) -> Result<(), AppError> {
        if matches!(
            *self.0.lock().unwrap_or_else(|e| e.into_inner()),
            ControlState::Cancelled
        ) {
            Err(AppError::new("EXPORT_CANCELLED", "图片导出已取消"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn commit(
        &self,
        commit: impl FnOnce() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(*state, ControlState::Running) {
            return Err(AppError::new("EXPORT_CANCELLED", "图片导出已取消或完成"));
        }
        let result = commit();
        *state = if result.is_ok() {
            ControlState::Committed
        } else {
            ControlState::Cancelled
        };
        result
    }
}

pub(crate) struct CapturedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
enum PageState {
    Pending,
    Ready { width: u32, height: u32 },
    Failed,
}

pub(crate) fn image_surface(html: &str) -> String {
    let html = html.replace("script-src 'none'", "script-src 'unsafe-inline'");
    let style = format!("<style>html,body{{width:{IMAGE_WIDTH}px;overflow:hidden}}main{{max-width:none;width:{IMAGE_WIDTH}px;padding:48px}}.mermaid-diagram{{overflow:visible}}.mermaid-diagram svg{{max-width:100%;height:auto}}math[display=block]{{max-width:none;overflow:visible}}</style>");
    let script = r#"<script>(()=>{
window.__marklitePngExport={status:'pending'};
const timeout = task => Promise.race([task,new Promise((_,reject)=>setTimeout(()=>reject(new Error('timeout')),8000))]);
(async()=>{try{
await timeout(document.fonts.ready);
await timeout(Promise.all([...document.images].map(image=>image.decode())));
await new Promise(resolve=>{requestAnimationFrame(resolve);setTimeout(resolve,100)});
const main=document.querySelector('main');
window.__marklitePngExport={status:'ready',width:Math.ceil(Math.max(main.scrollWidth,document.documentElement.scrollWidth)),height:Math.ceil(Math.max(main.scrollHeight,main.getBoundingClientRect().height))};
}catch{window.__marklitePngExport={status:'failed'}}})();
})()</script>"#;
    html.replace("</head>", &format!("{style}</head>"))
        .replace("</body>", &format!("{script}</body>"))
}

struct WindowGuard(Option<tauri::WebviewWindow>);
impl WindowGuard {
    fn window(&self) -> &tauri::WebviewWindow {
        self.0.as_ref().expect("capture window exists")
    }
    fn close(mut self) -> Result<(), AppError> {
        self.0
            .take()
            .expect("capture window closes once")
            .destroy()
            .map_err(|e| AppError::new("PNG_WEBVIEW_DESTROY_FAILED", e.to_string()))
    }
}
impl Drop for WindowGuard {
    fn drop(&mut self) {
        if let Some(window) = self.0.take() {
            let _ = window.destroy();
        }
    }
}

pub(crate) async fn capture(
    app: &tauri::AppHandle,
    html_path: &Path,
    control: &CaptureControl,
    reporter: &ExportReporter,
    work: ExportWork,
) -> Result<CapturedImage, AppError> {
    control.check()?;
    let url = url::Url::from_file_path(html_path)
        .map_err(|_| AppError::new("PNG_INVALID_SOURCE", "无法建立章节页面 URL"))?;
    let allowed = url.clone();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let builder = tauri::WebviewWindowBuilder::new(
        app,
        format!("png-export-{}-{sequence}", std::process::id()),
        tauri::WebviewUrl::External(url),
    )
    .title("MarkLite Image Export")
    .visible(false)
    .focused(false)
    .decorations(false)
    .resizable(false)
    .skip_taskbar(true)
    .inner_size(f64::from(IMAGE_WIDTH), 768.0)
    .on_navigation(move |url| *url == allowed);
    #[cfg(target_os = "macos")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let window =
        WindowGuard(Some(builder.build().map_err(|e| {
            AppError::new("PNG_WEBVIEW_CREATE_FAILED", e.to_string())
        })?));
    let operation = async {
        let deadline = Instant::now() + READY_TIMEOUT;
        let (width, height) = loop {
            control.check()?;
            if Instant::now() >= deadline {
                return Err(AppError::new(
                    "PNG_RENDER_TIMEOUT",
                    "章节页面未在 20 秒内就绪",
                ));
            }
            let (sender, receiver) = mpsc::channel();
            window
                .window()
                .eval_with_callback(
                    "JSON.stringify(window.__marklitePngExport || {status:'pending'})",
                    move |value| {
                        let _ = sender.send(Ok(value));
                    },
                )
                .map_err(|e| AppError::new("PNG_RENDER_FAILED", e.to_string()))?;
            let value = receive(receiver, deadline, control.clone()).await?;
            if value.len() > 1024 {
                return Err(AppError::new("PNG_RENDER_FAILED", "章节状态超出预算"));
            }
            let json: String = serde_json::from_str(&value)
                .map_err(|_| AppError::new("PNG_RENDER_FAILED", "章节状态无效"))?;
            let state: PageState = serde_json::from_str(&json)
                .map_err(|_| AppError::new("PNG_RENDER_FAILED", "章节状态无效"))?;
            match state {
                PageState::Ready { width, height } => {
                    validate_dimensions(width, height)?;
                    break (width, height);
                }
                PageState::Failed => {
                    return Err(AppError::new(
                        "PNG_RESOURCE_FAILED",
                        "章节字体或图片未能完成加载",
                    ))
                }
                PageState::Pending => {
                    tauri::async_runtime::spawn_blocking(|| {
                        std::thread::sleep(Duration::from_millis(30))
                    })
                    .await
                    .map_err(|_| AppError::new("PNG_RENDER_FAILED", "章节等待失败"))?;
                }
            }
        };
        reporter.processing(ExportStage::Encoding, work);
        let bytes = platform_capture(window.window(), width, height, control).await?;
        control.check()?;
        Ok(CapturedImage {
            bytes,
            width,
            height,
        })
    }
    .await;
    let closed = window.close();
    match (operation, closed) {
        (Err(e), _) => Err(e),
        (Ok(_), Err(e)) => Err(e),
        (Ok(image), Ok(())) => Ok(image),
    }
}

async fn receive<T: Send + 'static>(
    receiver: mpsc::Receiver<Result<T, AppError>>,
    deadline: Instant,
    control: CaptureControl,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(move || loop {
        control.check()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AppError::new("PNG_CAPTURE_TIMEOUT", "章节捕获超过阶段时限"));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(AppError::new("PNG_CAPTURE_FAILED", "章节捕获通道已关闭"))
            }
        }
    })
    .await
    .map_err(|_| AppError::new("PNG_CAPTURE_FAILED", "章节捕获等待异常"))?
}

#[cfg(windows)]
async fn protocol(
    window: &tauri::WebviewWindow,
    method: &str,
    parameters: serde_json::Value,
    deadline: Instant,
    control: &CaptureControl,
) -> Result<String, AppError> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;
    let (sender, receiver) = mpsc::channel();
    let method = method.to_owned();
    let parameters = parameters.to_string();
    window
        .with_webview(move |webview| unsafe {
            let callback_sender = sender.clone();
            let result = (|| -> windows::core::Result<()> {
                let core = webview.controller().CoreWebView2()?;
                let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                    move |status, json| {
                        let result = status
                            .map(|_| json)
                            .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()));
                        let _ = callback_sender.send(result);
                        Ok(())
                    },
                ));
                core.CallDevToolsProtocolMethod(
                    &HSTRING::from(&method),
                    &HSTRING::from(&parameters),
                    &handler,
                )
            })();
            if let Err(error) = result {
                let _ = sender.send(Err(AppError::new("PNG_CAPTURE_FAILED", error.to_string())));
            }
        })
        .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
    receive(receiver, deadline, control.clone()).await
}

#[cfg(windows)]
async fn platform_capture(
    window: &tauri::WebviewWindow,
    width: u32,
    height: u32,
    control: &CaptureControl,
) -> Result<Vec<u8>, AppError> {
    use base64::Engine;
    let deadline = Instant::now() + CAPTURE_TIMEOUT;
    protocol(
        window,
        "Emulation.setDeviceMetricsOverride",
        serde_json::json!({"width":width,"height":768,"deviceScaleFactor":1,"mobile":false}),
        deadline,
        control,
    )
    .await?;
    let value=protocol(window,"Page.captureScreenshot",serde_json::json!({"format":"png","captureBeyondViewport":true,"fromSurface":true,"clip":{"x":0,"y":0,"width":width,"height":height,"scale":1}}),deadline,control).await?;
    if value.len() > MAX_IMAGE_BYTES * 4 / 3 + 1024 {
        return Err(AppError::new(
            "PNG_OUTPUT_TOO_LARGE",
            "单章 PNG 超过 64 MiB",
        ));
    }
    #[derive(Deserialize)]
    struct ResultData {
        data: String,
    }
    let value: ResultData = serde_json::from_str(&value)
        .map_err(|_| AppError::new("PNG_CAPTURE_FAILED", "平台截图数据无效"))?;
    base64::engine::general_purpose::STANDARD
        .decode(value.data)
        .map_err(|_| AppError::new("PNG_CAPTURE_FAILED", "平台截图编码无效"))
}

#[cfg(target_os = "linux")]
async fn platform_capture(
    window: &tauri::WebviewWindow,
    width: u32,
    height: u32,
    control: &CaptureControl,
) -> Result<Vec<u8>, AppError> {
    use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};
    let (sender, receiver) = mpsc::channel();
    window
        .with_webview(move |webview| {
            webview.inner().snapshot(
                SnapshotRegion::FullDocument,
                SnapshotOptions::NONE,
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    let result = (|| {
                        let surface = result
                            .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
                        let source = cairo::ImageSurface::try_from(surface)
                            .map_err(|_| AppError::new("PNG_CAPTURE_FAILED", "平台截图不是位图"))?;
                        let output = cairo::ImageSurface::create(
                            cairo::Format::ARgb32,
                            width as i32,
                            height as i32,
                        )
                        .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
                        let context = cairo::Context::new(&output)
                            .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
                        // Normalize device pixels to the requested logical chapter size.
                        let (sx, sy) = source.device_scale();
                        source.set_device_scale(1.0, 1.0);
                        context.scale(1.0 / sx, 1.0 / sy);
                        context
                            .set_source_surface(&source, 0.0, 0.0)
                            .and_then(|_| context.paint())
                            .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
                        let mut bytes = Vec::new();
                        output
                            .write_to_png(&mut bytes)
                            .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
                        if bytes.len() > MAX_IMAGE_BYTES {
                            return Err(AppError::new(
                                "PNG_OUTPUT_TOO_LARGE",
                                "单章 PNG 超过 64 MiB",
                            ));
                        }
                        Ok(bytes)
                    })();
                    let _ = sender.send(result);
                },
            );
        })
        .map_err(|e| AppError::new("PNG_CAPTURE_FAILED", e.to_string()))?;
    receive(receiver, Instant::now() + CAPTURE_TIMEOUT, control.clone()).await
}

#[cfg(target_os = "macos")]
async fn platform_capture(
    window: &tauri::WebviewWindow,
    width: u32,
    height: u32,
    control: &CaptureControl,
) -> Result<Vec<u8>, AppError> {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
        NSGraphicsContext, NSImage,
    };
    use objc2_foundation::{NSDictionary, NSError, NSPoint, NSRect, NSSize};
    use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |webview| unsafe {
        let view=&*(webview.inner().cast::<WKWebView>());
        let size=NSSize::new(f64::from(width),f64::from(height));
        let rect=NSRect::new(NSPoint::new(0.0,0.0),size);
        view.setFrameSize(size);
        let Some(main_thread) = MainThreadMarker::new() else {
            let _ = sender.send(Err(AppError::new("PNG_CAPTURE_FAILED", "章节截图必须在主线程创建")));
            return;
        };
        let configuration=WKSnapshotConfiguration::new(main_thread); configuration.setRect(rect);
        let callback=block2::RcBlock::new(move |image:*mut NSImage,error:*mut NSError| {
            let result=(|| {
                if !error.is_null() || image.is_null() { return Err(AppError::new("PNG_CAPTURE_FAILED","macOS 未生成章节截图")); }
                let image=&*image;
                let bitmap=NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),std::ptr::null_mut(),width as isize,height as isize,8,4,true,false,NSDeviceRGBColorSpace,0,0)
                    .ok_or_else(|| AppError::new("PNG_CAPTURE_FAILED","无法分配章节位图"))?;
                let context=NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap).ok_or_else(|| AppError::new("PNG_CAPTURE_FAILED","无法创建位图上下文"))?;
                NSGraphicsContext::saveGraphicsState_class(); NSGraphicsContext::setCurrentContext(Some(&context));
                image.drawInRect_fromRect_operation_fraction(rect,NSRect::new(NSPoint::new(0.0,0.0),image.size()),NSCompositingOperation::Copy,1.0);
                NSGraphicsContext::restoreGraphicsState_class();
                let data=bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG,&NSDictionary::new()).ok_or_else(|| AppError::new("PNG_CAPTURE_FAILED","无法编码章节 PNG"))?;
                if data.len()>MAX_IMAGE_BYTES { return Err(AppError::new("PNG_OUTPUT_TOO_LARGE","单章 PNG 超过 64 MiB")); }
                Ok(data.to_vec())
            })(); let _=sender.send(result);
        });
        view.takeSnapshotWithConfiguration_completionHandler(Some(&configuration),&callback);
    }).map_err(|e| AppError::new("PNG_CAPTURE_FAILED",e.to_string()))?;
    receive(receiver, Instant::now() + CAPTURE_TIMEOUT, control.clone()).await
}
