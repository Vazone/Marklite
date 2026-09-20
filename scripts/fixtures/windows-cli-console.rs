//! Hidden real console: children inherit CONOUT$, never a capture pipe.
#![cfg(windows)]
use std::{
    fs::{self, OpenOptions},
    os::windows::io::AsRawHandle,
    process::{Command, Stdio},
};
#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}
#[link(name = "kernel32")]
extern "system" {
    fn FreeConsole() -> i32;
    fn AllocConsole() -> i32;
    fn GetConsoleWindow() -> isize;
    fn GetConsoleMode(handle: *mut std::ffi::c_void, mode: *mut u32) -> i32;
    fn ReadConsoleOutputCharacterW(
        handle: *mut std::ffi::c_void,
        text: *mut u16,
        len: u32,
        pos: Coord,
        read: *mut u32,
    ) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn ShowWindow(window: isize, command: i32) -> i32;
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert!(
        args.len() >= 2,
        "console-helper <report> <program> [args...]"
    );
    // SAFETY: Isolated helper owns this console; it never attaches to the user's.
    unsafe {
        FreeConsole();
        assert_ne!(AllocConsole(), 0);
        ShowWindow(GetConsoleWindow(), 0);
    }
    let console = OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONOUT$")
        .unwrap();
    let mut mode = 0;
    assert_ne!(
        unsafe { GetConsoleMode(console.as_raw_handle(), &mut mode) },
        0
    );
    let status = Command::new(&args[1])
        .args(&args[2..])
        .stdout(Stdio::from(console.try_clone().unwrap()))
        .stderr(Stdio::from(console.try_clone().unwrap()))
        .status()
        .unwrap();
    let mut text = vec![0_u16; 16_000];
    let mut read = 0;
    assert_ne!(
        unsafe {
            ReadConsoleOutputCharacterW(
                console.as_raw_handle(),
                text.as_mut_ptr(),
                text.len() as u32,
                Coord { x: 0, y: 0 },
                &mut read,
            )
        },
        0
    );
    fs::write(
        &args[0],
        format!(
            "exit={}\nconsole=true\n{}",
            status.code().unwrap_or(-1),
            String::from_utf16_lossy(&text[..read as usize])
        ),
    )
    .unwrap();
    unsafe {
        FreeConsole();
    }
}
