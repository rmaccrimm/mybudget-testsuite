//! Test harness: each test starts its own `mybudget-server` process and talks to it over HTTP.
//!
//! The suite never links the server's code, so nothing the server does at runtime can
//! reach into the test process.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
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

/// Bodies larger than this are sent by `Server::request_over_tcp`.
const LARGE_BODY: usize = 1024 * 1024;

/// Parses a complete HTTP/1.1 response (Content-Length, chunked, or read to EOF).
fn parse_response(raw: &[u8]) -> Option<Response> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let rest = &raw[end + 4..];
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let mut content_type = None;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-type" => content_type = Some(value.to_owned()),
            "content-length" => content_length = value.parse::<usize>().ok(),
            "transfer-encoding" => chunked = value.to_ascii_lowercase().contains("chunked"),
            _ => {}
        }
    }
    let body = if chunked {
        let mut body = Vec::new();
        let mut rest = rest;
        loop {
            let line_end = rest.windows(2).position(|w| w == b"\r\n")?;
            let size = std::str::from_utf8(&rest[..line_end]).ok()?.split(';').next()?.trim();
            let size = usize::from_str_radix(size, 16).ok()?;
            rest = &rest[line_end + 2..];
            if size == 0 {
                break;
            }
            body.extend_from_slice(rest.get(..size)?);
            rest = rest.get(size + 2..)?;
        }
        body
    } else if let Some(len) = content_length {
        rest.get(..len)?.to_vec()
    } else {
        rest.to_vec()
    };
    Some(Response { status, content_type, body })
}

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
        if let Some(body) = body.as_ref().filter(|b| b.len() > LARGE_BODY) {
            return self.request_over_tcp(method, path, body.clone());
        }
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

    /// Sends a JSON-typed body over a plain TCP connection, writing it on another thread
    /// while reading the response. A server may answer a large body (413) before reading
    /// all of it and close the connection; this still sees that answer.
    fn request_over_tcp(&self, method: &str, path: &str, body: Vec<u8>) -> Response {
        let addr = self.base_url.strip_prefix("http://").unwrap().to_owned();
        let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| panic!("connect to {addr}: {e}"));
        stream.set_read_timeout(Some(REQUEST_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(REQUEST_TIMEOUT)).unwrap();
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let mut writer = stream.try_clone().unwrap();
        let sender = std::thread::spawn(move || {
            // The server may stop reading early; write errors are expected then.
            let _ = writer.write_all(head.as_bytes()).and_then(|_| writer.write_all(&body));
        });
        let mut raw = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => raw.extend_from_slice(&buf[..n]),
                // A reset after the response arrived still leaves a usable response.
                Err(_) if !raw.is_empty() => break,
                Err(e) => panic!("request to {path} failed: no response ({e})"),
            }
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let _ = sender.join();
        parse_response(&raw).unwrap_or_else(|| {
            panic!("request to {path}: malformed response {:?}", String::from_utf8_lossy(&raw))
        })
    }

    /// Sends `method path` with `body` and the given `Content-Type`, optionally with chunked
    /// transfer encoding instead of a `Content-Length`.
    pub fn request_with(&self, method: &str, path: &str, content_type: &str, body: &[u8], chunked: bool) -> Response {
        let method = reqwest::Method::from_bytes(method.as_bytes()).expect("bad method");
        let req = self.client.request(method, self.url(path)).header("Content-Type", content_type);
        let req = if chunked {
            req.body(reqwest::blocking::Body::new(std::io::Cursor::new(body.to_vec())))
        } else {
            req.body(body.to_vec())
        };
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

// ---------------------------------------------------------------------------
// Helpers for the things inside a plan (sections 5 onward).

/// The date the helpers give a non-zero starting balance.
pub const START_DATE: &str = "2026-10-01";

impl Server {
    /// Sends `HEAD path`.
    pub fn head(&self, path: &str) -> Response {
        self.request_bytes("HEAD", path, None)
    }

    /// `POST path body`, asserting `201`; returns the created object.
    pub fn create(&self, path: &str, body: &Value) -> Value {
        let res = self.post(path, body);
        assert_eq!(res.status, 201, "POST {path} {body}: {}", res.text());
        res.json()
    }

    /// `GET path`, asserting `200`; returns the body.
    pub fn get_json(&self, path: &str) -> Value {
        let res = self.get(path);
        assert_eq!(res.status, 200, "GET {path}: {}", res.text());
        res.json()
    }

    /// `PATCH path body`, asserting `200`; returns the body.
    pub fn patch_ok(&self, path: &str, body: &Value) -> Value {
        let res = self.patch(path, body);
        assert_eq!(res.status, 200, "PATCH {path} {body}: {}", res.text());
        res.json()
    }

    /// `GET path`, asserting `200` and a JSON array; returns its items.
    pub fn list(&self, path: &str) -> Vec<Value> {
        let body = self.get_json(path);
        body.as_array().unwrap_or_else(|| panic!("GET {path} did not return an array: {body}")).clone()
    }

    /// Creates an account in `plan` from `body`, asserting success.
    pub fn create_account(&self, plan: &str, body: &Value) -> Value {
        self.create(&format!("/plans/{plan}/accounts"), body)
    }

    /// Creates an account named `name` of type `ty`; a non-zero `starting_balance` is dated
    /// `START_DATE`.
    pub fn add_account(&self, plan: &str, name: &str, ty: &str, starting_balance: i64) -> Value {
        let body = if starting_balance == 0 {
            json!({"name": name, "type": ty})
        } else {
            json!({"name": name, "type": ty, "starting_balance": starting_balance, "starting_date": START_DATE})
        };
        self.create_account(plan, &body)
    }

    /// Creates a payee named `name` in `plan`, asserting success.
    pub fn create_payee(&self, plan: &str, name: &str) -> Value {
        self.create(&format!("/plans/{plan}/payees"), &json!({ "name": name }))
    }

    /// Creates a category group named `name` in `plan`, asserting success.
    pub fn create_group(&self, plan: &str, name: &str) -> Value {
        self.create(&format!("/plans/{plan}/category_groups"), &json!({ "name": name }))
    }

    /// Creates a category named `name` at the end of group `group_id`, asserting success.
    pub fn create_category(&self, plan: &str, group_id: &str, name: &str) -> Value {
        self.create(&format!("/plans/{plan}/categories"), &json!({ "group_id": group_id, "name": name }))
    }

    /// `GET /plans/{plan}/accounts`.
    pub fn accounts(&self, plan: &str) -> Vec<Value> {
        self.list(&format!("/plans/{plan}/accounts"))
    }

    /// `GET /plans/{plan}/payees`.
    pub fn payees(&self, plan: &str) -> Vec<Value> {
        self.list(&format!("/plans/{plan}/payees"))
    }

    /// `GET /plans/{plan}/categories`: the plan's groups, each with its categories.
    pub fn groups(&self, plan: &str) -> Vec<Value> {
        self.list(&format!("/plans/{plan}/categories"))
    }
}

/// Asserts that `res` has `status`, showing the body if it doesn't.
pub fn assert_status(res: &Response, status: u16, what: &str) {
    assert_eq!(res.status, status, "{what}: body {:?}", res.text());
}

/// Asserts that `res` has `status` and a JSON body whose `error` is a non-empty string.
pub fn assert_error(res: &Response, status: u16, what: &str) {
    assert_status(res, status, what);
    let body: Value = serde_json::from_slice(&res.body)
        .unwrap_or_else(|e| panic!("{what}: error body is not JSON ({e}): {:?}", res.text()));
    match body.get("error") {
        Some(Value::String(s)) if !s.is_empty() => {}
        _ => panic!("{what}: body has no non-empty string `error`: {body}"),
    }
}

/// The `id` of `obj`, which must be a non-empty string.
pub fn id_of(obj: &Value) -> String {
    match obj.get("id") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => panic!("no non-empty string `id` in {obj}"),
    }
}

/// The string field `field` of `obj`.
pub fn str_field(obj: &Value, field: &str) -> String {
    obj.get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("`{field}` is not a string in {obj}"))
        .to_owned()
}

/// The ids of a list of objects, in order.
pub fn ids_of(list: &[Value]) -> Vec<String> {
    list.iter().map(id_of).collect()
}

/// The names of a list of objects, in order.
pub fn names_of(list: &[Value]) -> Vec<String> {
    list.iter().map(|v| str_field(v, "name")).collect()
}

/// Asserts that `obj` is an object with every field in `fields` present (possibly `null`).
pub fn assert_fields(obj: &Value, fields: &[&str], what: &str) {
    let map = obj.as_object().unwrap_or_else(|| panic!("{what}: not an object: {obj}"));
    for field in fields {
        assert!(map.contains_key(*field), "{what}: lacks field `{field}`: {obj}");
    }
}

/// Every response field sections 4 to 7 name.
const SPEC_FIELDS: [&str; 18] = [
    "id",
    "name",
    "currency_code",
    "type",
    "on_budget",
    "closed",
    "note",
    "balance",
    "cleared_balance",
    "uncleared_balance",
    "transfer_payee_id",
    "payment_category_id",
    "transfer_account_id",
    "hidden",
    "internal",
    "categories",
    "group_id",
    "payment_account_id",
];

/// `value` with only the object fields the spec names, at every depth. Responses may carry
/// other fields (SPEC 3.6), so compare objects from different responses through this.
pub fn spec_fields(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| SPEC_FIELDS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), spec_fields(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(spec_fields).collect()),
        other => other.clone(),
    }
}

/// Asserts that `actual` has everything `expected` has: each field of an expected object,
/// present with a matching value, and arrays of the same length whose items match in order.
/// Fields that `expected` doesn't name are allowed (SPEC 3.6).
pub fn assert_matches(actual: &Value, expected: &Value, what: &str) {
    fn walk(actual: &Value, expected: &Value, at: &str, what: &str, whole: &Value) {
        match expected {
            Value::Object(fields) => {
                let Some(obj) = actual.as_object() else {
                    panic!("{what}: {at} is {actual}, expected an object\nwhole: {whole}");
                };
                for (key, want) in fields {
                    let Some(got) = obj.get(key) else {
                        panic!("{what}: {at} lacks field `{key}`\nwhole: {whole}");
                    };
                    walk(got, want, &format!("{at}.{key}"), what, whole);
                }
            }
            Value::Array(items) => {
                let Some(arr) = actual.as_array() else {
                    panic!("{what}: {at} is {actual}, expected an array\nwhole: {whole}");
                };
                assert_eq!(arr.len(), items.len(), "{what}: {at} has {} items, expected {}\nwhole: {whole}", arr.len(), items.len());
                for (i, (got, want)) in arr.iter().zip(items).enumerate() {
                    walk(got, want, &format!("{at}[{i}]"), what, whole);
                }
            }
            _ => assert!(actual == expected, "{what}: {at} is {actual}, expected {expected}\nwhole: {whole}"),
        }
    }
    walk(actual, expected, "body", what, actual);
}
