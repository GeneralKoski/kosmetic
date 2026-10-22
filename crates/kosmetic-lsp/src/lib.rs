//! Language server for kosmetic, speaking LSP over stdio.
//!
//! It publishes the diagnostics of every open TOML file, offers the fixes of
//! the rules as quick fixes and as a "fix all" source action, and formats
//! documents with `kosmetic-format`. Documents are synchronized in full, and
//! the nearest `.kosmetic.toml` is read again on every change.
//!
//! JSON-RPC framing and the handful of messages used are written by hand on
//! top of `serde_json`, like the rest of the project: no LSP crate is used.

mod position;
mod server;
mod transport;

use std::io::{self, BufRead, Write};

pub use server::Server;
pub use transport::{read_message, write_message};

/// Serves one client until it sends `exit` or closes the input. Returns the
/// exit code of the process: 0 after an orderly shutdown, 1 otherwise.
///
/// # Errors
///
/// Fails on I/O errors and on messages that cannot be read.
pub fn run(mut input: impl BufRead, mut output: impl Write) -> io::Result<i32> {
    let mut server = Server::new();
    while let Some(message) = read_message(&mut input)? {
        for reply in server.handle(&message) {
            write_message(&mut output, &reply)?;
        }
        if let Some(code) = server.exit_code() {
            return Ok(code);
        }
    }
    Ok(1)
}
