#[tauri::command]
pub fn exit_android_application(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn set_android_status_bar_appearance(app: tauri::AppHandle, dark: bool) -> Result<(), String> {
    use tauri_plugin_marklite_mobile::MarkliteMobileExt;

    app.marklite_mobile()
        .set_status_bar_appearance(dark)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn android_system_insets(
    app: tauri::AppHandle,
) -> Result<tauri_plugin_marklite_mobile::SystemInsetsResponse, String> {
    use tauri_plugin_marklite_mobile::MarkliteMobileExt;

    let insets = app
        .marklite_mobile()
        .system_insets()
        .map_err(|error| error.to_string())?;
    if [insets.top, insets.right, insets.bottom, insets.left]
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=256.0).contains(value))
    {
        return Err("Invalid Android system inset".into());
    }
    Ok(insets)
}
