// Production parser/preview probe without desktop, export or filesystem services.
#![allow(dead_code)]

mod models {
    pub mod app_error { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/models/app_error.rs")); }
    pub mod diagram { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/models/diagram.rs")); }
    pub mod export { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/models/export.rs")); }
    pub mod markdown { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/models/markdown.rs")); }
}
mod services {
    pub mod diagram_service { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/services/diagram_service.rs")); }
    pub mod markdown_service { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/services/markdown_service.rs")); }
    pub mod math_service { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/services/math_service.rs")); }
}
mod utils {
    pub mod security { include!(concat!(env!("MARKLITE_REVIEW_ROOT"), "/src-tauri/src/utils/security.rs")); }
}

#[path = "markdown_conformance.rs"]
mod markdown_conformance;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().expect("cases path");
    let output = args.next().expect("rendered path");
    markdown_conformance::run(&input, &output);
}
