#![windows_subsystem = "windows"]
use std::io::Write;
fn main() {
    let out = std::thread::spawn(|| {
        for _ in 0..65536 {
            std::io::stdout()
                .write_all("stdout 中文 😀\n".as_bytes())
                .unwrap();
        }
    });
    for _ in 0..65536 {
        std::io::stderr()
            .write_all("stderr 中文 😀\n".as_bytes())
            .unwrap();
    }
    out.join().unwrap();
    std::process::exit(37);
}
