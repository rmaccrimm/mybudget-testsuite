//! Encoding and method edge cases of sections 3.1 and 3.4 not covered elsewhere.

use mybudget_testsuite::{Response, Server, Value};

fn plans(server: &Server) -> Vec<Value> {
    let res = server.get("/plans");
    assert_eq!(res.status, 200, "GET /plans: {}", res.text());
    res.json().as_array().expect("GET /plans did not return an array").clone()
}

fn assert_status(res: &Response, status: u16, what: &str) {
    assert_eq!(res.status, status, "{what}: body {:?}", res.text());
}

/// `prefix` + `bytes` + `suffix` as a body.
fn body_with(prefix: &str, bytes: &[u8], suffix: &str) -> Vec<u8> {
    let mut body = prefix.as_bytes().to_vec();
    body.extend_from_slice(bytes);
    body.extend_from_slice(suffix.as_bytes());
    body
}

// ---- 3.1 ----

// Byte sequences that aren't valid UTF-8: a UTF-8-encoded surrogate (CESU style), overlong
// encodings, a code point past U+10FFFF, a truncated sequence and a stray continuation byte.
const BAD_UTF8: [&[u8]; 7] = [
    &[0xed, 0xa0, 0x80],
    &[0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80],
    &[0xc0, 0xaf],
    &[0xe0, 0x80, 0xaf],
    &[0xf4, 0x90, 0x80, 0x80],
    &[0xe2, 0x82],
    &[0x80],
];

#[test]
fn spec_3_1_rejects_invalid_utf8_in_names_keys_and_ignored_fields() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let path = format!("/plans/{id}");
    for bad in BAD_UTF8 {
        for (prefix, suffix) in [
            (r#"{"name": "a"#, r#"b"}"#),
            (r#"{"name": "Home", "note": "x"#, r#""}"#),
            (r#"{"name": "Home", "k"#, r#"": 1}"#),
            (r#"{"name": "Home", "x": [{"y": ""#, r#""}]}"#),
        ] {
            let body = body_with(prefix, bad, suffix);
            let what = format!("{:?}", String::from_utf8_lossy(&body));
            let res = server.request_bytes("POST", "/plans", Some(body.clone()));
            assert_status(&res, 400, &format!("POST {what}"));
            let res = server.request_bytes("PATCH", &path, Some(body));
            assert_status(&res, 400, &format!("PATCH {what}"));
        }
    }
    assert_eq!(plans(&server).len(), 1, "invalid UTF-8 bodies created plans");
    assert_eq!(server.get(&path).json()["name"], "Household");
}

#[test]
fn spec_3_1_accepts_escaped_keys_and_every_simple_escape() {
    let server = Server::start();
    let body = r#"{"name": "Home", "currency_code": "EUR"}"#;
    let res = server.request_raw("POST", "/plans", body);
    assert_status(&res, 201, &format!("POST {body}"));
    assert_eq!(res.json()["name"], "Home");
    assert_eq!(res.json()["currency_code"], "EUR");
    let id = res.json()["id"].as_str().unwrap().to_owned();

    let body = r#"{"name": "a\"b\\c\/d\be\ff\ng\rh\ti", "currency_code": "JPY"}"#;
    let res = server.request_raw("PATCH", &format!("/plans/{id}"), body);
    assert_status(&res, 200, &format!("PATCH {body}"));
    let want = "a\"b\\c/d\u{8}e\u{c}f\ng\rh\ti";
    assert_eq!(res.json()["name"], want);
    assert_eq!(res.json()["currency_code"], "JPY");
    let got = server.get(&format!("/plans/{id}")).json();
    assert_eq!(got["name"], want);
    assert_eq!(got["currency_code"], "JPY");
}

#[test]
fn spec_3_1_lone_surrogate_after_other_escapes_returns_400() {
    // A lone surrogate is invalid even when it sits right after a valid pair or an
    // escaped backslash, where a hand-written scanner could lose track of it.
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in [
        r#"{"name": "😀\ud800"}"#,
        r#"{"name": "\\\ud800"}"#,
        r#"{"name": "Home", "x": "\"\udfff"}"#,
        r#"{"name": "\ud800\\udc00"}"#,
    ] {
        assert_status(&server.request_raw("POST", "/plans", body), 400, &format!("POST {body}"));
        let res = server.request_raw("PATCH", &format!("/plans/{id}"), body);
        assert_status(&res, 400, &format!("PATCH {body}"));
    }
    // An escaped backslash followed by text that merely looks like an escape is fine.
    let res = server.request_raw("POST", "/plans", r#"{"name": "\\ud800"}"#);
    assert_status(&res, 201, "POST literal backslash-u text");
    assert_eq!(res.json()["name"], "\\ud800");
    assert_eq!(plans(&server).len(), 2);
}

// ---- 3.4 ----

#[test]
fn spec_3_4_methods_are_case_sensitive() {
    // HTTP methods are case-sensitive, so `get` isn't GET: it's a method the spec doesn't
    // define for these paths.
    let server = Server::start();
    let id = server.plan_id("Household");
    let plan = format!("/plans/{id}");
    for (method, path) in [("get", "/plans"), ("post", "/plans"), ("delete", plan.as_str()), ("Patch", plan.as_str())] {
        let res = server.request_raw(method, path, r#"{"name": "Other"}"#);
        assert_status(&res, 405, &format!("{method} {path}"));
    }
    assert_eq!(plans(&server).len(), 1);
    assert_eq!(server.get(&plan).json()["name"], "Household");
}
