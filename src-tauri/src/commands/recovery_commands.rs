use crate::{
    commands::background::run_background,
    models::{
        app_error::AppError,
        recovery::{RecoveryInventory, RecoveryReceipt, RecoverySnapshot},
    },
    services::recovery_service,
};

#[tauri::command]
pub async fn save_recovery(snapshot: RecoverySnapshot) -> Result<RecoveryReceipt, AppError> {
    run_background("保存恢复副本", move || {
        recovery_service::save(snapshot)
    })
    .await
}

#[tauri::command]
pub async fn list_recovery() -> Result<RecoveryInventory, AppError> {
    run_background("读取恢复列表", recovery_service::list).await
}

#[tauri::command]
pub async fn read_recovery(id: String) -> Result<RecoverySnapshot, AppError> {
    run_background("读取恢复副本", move || recovery_service::read(&id)).await
}

#[tauri::command]
pub async fn resolve_recovery(receipt: RecoveryReceipt) -> Result<(), AppError> {
    run_background("完成恢复副本", move || {
        recovery_service::resolve(receipt)
    })
    .await
}
