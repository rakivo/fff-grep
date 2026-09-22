use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

const SOCKET_PATH: &str = "/tmp/fffd.sock";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = std::env::args().nth(1).ok_or("usage: fffq <query>")?;

    let mut stream = UnixStream::connect(SOCKET_PATH).map_err(|e| format!(
        "Cannot reach fffd at {SOCKET_PATH} ({e}), start it first!"
    ))?;

    writeln!(stream, "{query}")?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut out = Vec::new();
    stream.read_to_end(&mut out)?;

    Ok(std::io::stdout().lock().write_all(&out)?)
}
