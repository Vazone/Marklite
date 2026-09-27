use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use std::{error::Error, fs::File, io::Read, path::Path, process::ExitCode};

fn decoded(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut text = String::new();
    let input: Box<dyn Read> = if path == Path::new("-") {
        Box::new(std::io::stdin())
    } else {
        Box::new(File::open(path)?)
    };
    input.take(16385).read_to_string(&mut text)?;
    if text.len() > 16384 {
        return Err("encoded key/signature exceeds size limit".into());
    }
    Ok(String::from_utf8(STANDARD.decode(text.trim())?)?)
}

fn verify() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: marklite-verify-signature <public-key-file> <artifact> <signature-file>".into(),
        );
    }
    let key = PublicKey::decode(&decoded(Path::new(&args[0]))?)?;
    let signature = Signature::decode(&decoded(Path::new(&args[2]))?)?;
    let mut verifier = key.verify_stream(&signature)?;
    let mut file = File::open(&args[1])?;
    let mut buffer = [0u8; 65536];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        verifier.update(&buffer[..size]);
    }
    verifier.finalize()?;
    Ok(())
}

fn main() -> ExitCode {
    match verify() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Update signature verification failed: {error}");
            ExitCode::FAILURE
        }
    }
}
