#[tauri::command]
pub fn get_platform_capabilities() -> crate::platform::capabilities::PlatformCapabilities {
    crate::platform::capabilities::current()
}
