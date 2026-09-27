use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

mod mobile;

mod error;
mod models;

pub use error::{Error, Result};

use mobile::MarkliteMobile;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the marklite-mobile APIs.
pub trait MarkliteMobileExt<R: Runtime> {
    fn marklite_mobile(&self) -> &MarkliteMobile<R>;
}

impl<R: Runtime, T: Manager<R>> crate::MarkliteMobileExt<R> for T {
    fn marklite_mobile(&self) -> &MarkliteMobile<R> {
        self.state::<MarkliteMobile<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("marklite-mobile")
        .setup(|app, api| {
            let marklite_mobile = mobile::init(app, api)?;
            app.manage(marklite_mobile);
            Ok(())
        })
        .build()
}
