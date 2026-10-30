//! Test harness: each test starts its own `mybudget-server` process and talks to it over HTTP.
//!
//! The suite never links the server's code, so nothing the server does at runtime can
//! reach into the test process.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

pub use reqwest;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A running server process on a free localhost port. Killed when dropped.
pub struct Server {
    child: Child,
    base_url: String,
    client: reqwest::blocking::Client,
}

impl Server {
    /// Starts the binary named by `MYBUDGET_SERVER_BIN` (set by `scripts/test.sh`).
    pub fn start() -> Server {
        let bin = std::env::var("MYBUDGET_SERVER_BIN")
            .expect("MYBUDGET_SERVER_BIN is not set; run the suite with scripts/test.sh");
        let mut child = Command::new(&bin)
            .env("MYBUDGET_ADDR", "127.0.0.1:0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to start {bin}: {e}"));

        // Server output is prefixed so it can't pass for test-harness output.
        let (first_line, rx) = mpsc::channel();
        forward_output(child.stdout.take().unwrap(), Some(first_line));
        forward_output(child.stderr.take().unwrap(), None);

        let addr = rx
            .recv_timeout(STARTUP_TIMEOUT)
            .ok()
            .and_then(|line| line.strip_prefix("listening on ").map(str::to_owned));
        let Some(addr) = addr else {
            let _ = child.kill();
            panic!("server did not print `listening on <addr>` within {STARTUP_TIMEOUT:?}");
        };

        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build HTTP client");
        Server { child, base_url: format!("http://{addr}"), client }
    }

    /// Full URL for `path`, which should start with `/`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// HTTP client to use with this server (no proxy, request timeout).
    pub fn client(&self) -> &reqwest::blocking::Client {
        &self.client
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Re-emits each line as `[server] ...` on the test's stderr (shown when a test fails),
/// sending the first line to `first` if given.
fn forward_output(stream: impl Read + Send + 'static, mut first: Option<mpsc::Sender<String>>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if let Some(tx) = first.take() {
                let _ = tx.send(line.clone());
            }
            eprintln!("[server] {line}");
        }
    });
}
