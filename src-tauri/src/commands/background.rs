use crate::models::app_error::AppError;

/// Runs blocking filesystem/serialization work outside Tauri's event loop and
/// normalizes only the task-join failure. Domain errors from the operation are
/// preserved unchanged.
pub async fn run_background<T, F>(operation: &'static str, task: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| AppError::background_task_failed(operation, error))?
}

#[cfg(test)]
mod tests {
    use super::run_background;
    use crate::models::app_error::AppError;

    #[test]
    fn preserves_domain_errors_and_maps_join_failures() {
        let domain = tauri::async_runtime::block_on(run_background("测试", || {
            Err::<(), _>(AppError::new("DOMAIN_FAILURE", "domain"))
        }))
        .unwrap_err();
        assert_eq!(domain.code, "DOMAIN_FAILURE");

        let join = tauri::async_runtime::block_on(run_background::<(), _>("测试", || {
            panic!("fixture panic")
        }))
        .unwrap_err();
        assert_eq!(join.code, "BACKGROUND_TASK_FAILED");
        assert!(join.message.starts_with("测试后台任务失败"));
    }
}
