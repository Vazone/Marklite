use tauri::State;

use crate::services::open_request_service::OpenRequestState;

#[tauri::command]
pub fn drain_open_file_requests(state: State<'_, OpenRequestState>) -> Vec<String> {
    state.drain()
}
