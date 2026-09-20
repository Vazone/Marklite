use std::{ffi::OsStr, process::Command};

#[cfg(windows)]
use std::{
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

const INTERNAL_CLI_MARKER: &str = "--marklite-internal-cli";
#[cfg(windows)]
const CANCEL_FILE_ENV: &str = "MARKLITE_CLI_CANCEL_FILE";
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
static FORWARDED_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

fn main() {
    let exit_code = run().unwrap_or_else(|error| {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!("CLI_LAUNCH_FAILED: {error}\n").as_bytes(),
        );
        6
    });
    std::process::exit(exit_code);
}

fn run() -> Result<i32, String> {
    let launcher = std::env::current_exe()
        .map_err(|error| format!("cannot resolve launcher path: {error}"))?;
    let executable_name = if cfg!(windows) {
        OsStr::new("marklite.exe")
    } else {
        OsStr::new("marklite")
    };
    let application = launcher
        .parent()
        .ok_or_else(|| "launcher has no parent directory".to_string())?
        .join(executable_name);
    if application == launcher {
        return Err("launcher resolved to itself".to_string());
    }
    let mut command = Command::new(&application);
    command
        .arg(INTERNAL_CLI_MARKER)
        .args(std::env::args_os().skip(1));
    let signal_guard = ForwardedSignalGuard::install(&mut command)?;
    #[cfg(windows)]
    let status = {
        let cancel_file = allocate_cancel_file()?;
        command
            .env(CANCEL_FILE_ENV, &cancel_file)
            .env("MARKLITE_CLI_PROGRESS", if io::stdout().is_terminal() && io::stderr().is_terminal() { "1" } else { "0" })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NEW_PROCESS_GROUP);
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", application.display()))?;
        // The GUI subsystem child needs pipe handles. Only this console launcher
        // writes to the terminal; drain both streams concurrently and boundedly.
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let out = thread::spawn(move || {
            forward_output(stdout, io::stdout().lock(), io::stdout().is_terminal())
        });
        let err = thread::spawn(move || {
            forward_output(stderr, io::stderr().lock(), io::stderr().is_terminal())
        });
        let wait_result = (|| {
            let mut cancel_forwarded = false;
            loop {
                if FORWARDED_CANCEL_REQUESTED.swap(false, Ordering::AcqRel) && !cancel_forwarded {
                    let write_result = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&cancel_file)
                        .and_then(|mut file| file.write_all(b"cancel"));
                    if let Err(error) = write_result {
                        return Err(format!("cannot forward cancellation: {error}"));
                    }
                    cancel_forwarded = true;
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return Ok(status);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        return Err(format!(
                            "cannot wait for {}: {error}",
                            application.display()
                        ));
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
        })();
        if wait_result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Join both before propagating any failure; never abandon the other pipe.
        let out_result = out.join();
        let err_result = err.join();
        let _ = fs::remove_file(&cancel_file);
        let status = wait_result?;
        out_result
            .map_err(|_| "stdout forwarding thread failed")?
            .map_err(|error| format!("stdout forwarding failed: {error}"))?;
        err_result
            .map_err(|_| "stderr forwarding thread failed")?
            .map_err(|error| format!("stderr forwarding failed: {error}"))?;
        status
    };
    #[cfg(not(windows))]
    let status = command
        .status()
        .map_err(|error| format!("cannot start {}: {error}", application.display()))?;
    drop(signal_guard);
    let code = status.code().unwrap_or(130);
    #[cfg(windows)]
    if code == -1_073_741_510 {
        return Ok(130);
    }
    Ok(code)
}

#[cfg(windows)]
fn forward_output(mut input: impl Read, mut output: impl Write, console: bool) -> io::Result<()> {
    let result = (|| {
        let mut buffer = [0_u8; 8192];
        let mut pending = 0;
        loop {
            let count = match input.read(&mut buffer[pending..]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                if pending != 0 {
                    if console {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "incomplete UTF-8 console output",
                        ));
                    }
                    output.write_all(&buffer[..pending])?;
                }
                return output.flush();
            }
            let available = pending + count;
            // Preserve UTF-8 boundaries for redirected streams too: stdout and
            // stderr may be redirected to the same file. Non-UTF-8 pipe bytes
            // still pass through unchanged (console output must be valid UTF-8).
            let complete = match std::str::from_utf8(&buffer[..available]) {
                Ok(_) => available,
                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                Err(error) if console => {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, error))
                }
                Err(_) => available,
            };
            output.write_all(&buffer[..complete])?;
            output.flush()?;
            pending = available - complete;
            buffer.copy_within(complete..available, 0);
        }
    })();
    if result.is_err() {
        // A closed consumer must not deadlock the child or its other stream.
        // Finish draining, wait for its normal cleanup, then report exit 6.
        let _ = io::copy(&mut input, &mut io::sink());
    }
    result
}

#[cfg(windows)]
fn allocate_cancel_file() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("marklite-cli-cancel-{}", std::process::id()));
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot reset cancellation channel: {error}")),
    }
    Ok(path)
}

struct ForwardedSignalGuard {
    #[cfg(windows)]
    installed: bool,
    #[cfg(unix)]
    previous_interrupt: usize,
}

impl ForwardedSignalGuard {
    fn install(command: &mut Command) -> Result<Self, String> {
        #[cfg(windows)]
        {
            FORWARDED_CANCEL_REQUESTED.store(false, Ordering::Release);
            #[link(name = "kernel32")]
            extern "system" {
                fn SetConsoleCtrlHandler(
                    handler: Option<unsafe extern "system" fn(u32) -> i32>,
                    add: i32,
                ) -> i32;
            }
            // SAFETY: The static handler has the required ABI and is removed by
            // the guard after the child exits.
            let installed = unsafe { SetConsoleCtrlHandler(Some(capture_console_signal), 1) } != 0;
            if !installed {
                return Err("cannot install launcher console handler".to_string());
            }
            let _ = command;
            return Ok(Self { installed });
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            const SIGINT: i32 = 2;
            const SIG_DFL: usize = 0;
            const SIG_IGN: usize = 1;
            extern "C" {
                fn signal(signal: i32, handler: usize) -> usize;
            }
            // SAFETY: `signal` installs the standard ignore disposition in the
            // launcher. `pre_exec` restores default only in the child between
            // fork and exec and calls no allocating code.
            let previous_interrupt = unsafe { signal(SIGINT, SIG_IGN) };
            unsafe {
                command.pre_exec(|| {
                    signal(SIGINT, SIG_DFL);
                    Ok(())
                });
            }
            return Ok(Self { previous_interrupt });
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = command;
            Ok(Self {})
        }
    }
}

impl Drop for ForwardedSignalGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        if self.installed {
            #[link(name = "kernel32")]
            extern "system" {
                fn SetConsoleCtrlHandler(
                    handler: Option<unsafe extern "system" fn(u32) -> i32>,
                    add: i32,
                ) -> i32;
            }
            // SAFETY: Removes the exact handler installed by this guard.
            let _ = unsafe { SetConsoleCtrlHandler(Some(capture_console_signal), 0) };
        }
        #[cfg(unix)]
        {
            const SIGINT: i32 = 2;
            extern "C" {
                fn signal(signal: i32, handler: usize) -> usize;
            }
            // SAFETY: Restores the prior process disposition.
            let _ = unsafe { signal(SIGINT, self.previous_interrupt) };
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn capture_console_signal(signal: u32) -> i32 {
    if matches!(signal, 0 | 1) {
        FORWARDED_CANCEL_REQUESTED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    struct ByteReader<'a>(&'a [u8]);
    impl Read for ByteReader<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            bytes[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }
    #[test]
    fn console_keeps_split_unicode_scalars_intact() {
        let text = "中文 😀\n".repeat(4096);
        let mut output = Vec::new();
        forward_output(ByteReader(text.as_bytes()), &mut output, true).unwrap();
        assert_eq!(output, text.as_bytes());
    }
    #[test]
    fn pipes_preserve_arbitrary_bytes_and_console_rejects_truncated_utf8() {
        let mut output = Vec::new();
        forward_output(&[0xff, 0xe4][..], &mut output, false).unwrap();
        assert_eq!(output, [0xff, 0xe4]);
        assert!(forward_output(&[0xe4][..], &mut Vec::new(), true).is_err());
    }
}
