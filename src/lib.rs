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

pub use serde_json::{self, Value, json};

/// A response read in full: status, `Content-Type` header and body.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Response {
    /// The body as text (lossy UTF-8).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body parsed as JSON. Panics with the body if it isn't JSON.
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!("response body is not JSON ({e}): status {}, body {:?}", self.status, self.text())
        })
    }

    /// Whether the `Content-Type` media type is `application/json` (parameters such as
    /// `charset` are allowed).
    pub fn is_json_content_type(&self) -> bool {
        self.content_type.as_deref().is_some_and(|ct| {
            ct.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json")
        })
    }
}

impl Server {
    /// Sends `method path`, with `body` as a JSON-typed body if given.
    pub fn request_bytes(&self, method: &str, path: &str, body: Option<Vec<u8>>) -> Response {
        let method = reqwest::Method::from_bytes(method.as_bytes()).expect("bad method");
        let mut req = self.client.request(method, self.url(path));
        if let Some(body) = body {
            req = req.header("Content-Type", "application/json").body(body);
        }
        let res = req.send().unwrap_or_else(|e| panic!("request to {path} failed: {e}"));
        let status = res.status().as_u16();
        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned());
        let body = res.bytes().expect("failed to read response body").to_vec();
        Response { status, content_type, body }
    }

    /// Sends `method path` with a raw text body (which need not be valid JSON).
    pub fn request_raw(&self, method: &str, path: &str, body: &str) -> Response {
        self.request_bytes(method, path, Some(body.as_bytes().to_vec()))
    }

    pub fn get(&self, path: &str) -> Response {
        self.request_bytes("GET", path, None)
    }

    pub fn delete(&self, path: &str) -> Response {
        self.request_bytes("DELETE", path, None)
    }

    pub fn post(&self, path: &str, body: &Value) -> Response {
        self.request_raw("POST", path, &body.to_string())
    }

    pub fn patch(&self, path: &str, body: &Value) -> Response {
        self.request_raw("PATCH", path, &body.to_string())
    }

    /// Creates a plan with `body`, asserting success, and returns the created plan.
    pub fn create_plan(&self, body: &Value) -> Value {
        let res = self.post("/plans", body);
        assert_eq!(res.status, 201, "POST /plans {body} failed: {}", res.text());
        res.json()
    }

    /// Creates a plan named `name` and returns its id.
    pub fn plan_id(&self, name: &str) -> String {
        let plan = self.create_plan(&json!({ "name": name }));
        plan["id"].as_str().expect("plan id is not a string").to_owned()
    }
}
