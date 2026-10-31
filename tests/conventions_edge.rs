//! Edge cases of sections 2 and 3 that the main convention tests don't reach.

use mybudget_testsuite::{Response, Server, Value, json, serde_json};

fn plans(server: &Server) -> Vec<Value> {
    let res = server.get("/plans");
    assert_eq!(res.status, 200, "GET /plans: {}", res.text());
    res.json().as_array().expect("GET /plans did not return an array").clone()
}

fn assert_status(res: &Response, status: u16, what: &str) {
    assert_eq!(res.status, status, "{what}: body {:?}", res.text());
}

fn assert_error_body(res: &Response, status: u16, what: &str) {
    assert_status(res, status, what);
    let body = res.json();
    let error = body.get("error").unwrap_or_else(|| panic!("{what}: no `error` field in {body}"));
    let error = error.as_str().unwrap_or_else(|| panic!("{what}: `error` is not a string: {body}"));
    assert!(!error.is_empty(), "{what}: `error` is empty");
}

// ---- 2.2 ----

#[test]
fn spec_2_2_paths_with_extra_slashes_or_other_case_return_404() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for path in [
        "/plans/".to_owned(),
        format!("/plans/{id}/"),
        format!("/plans//{id}"),
        "//plans".to_owned(),
        "/health/".to_owned(),
        "/PLANS".to_owned(),
        "/Health".to_owned(),
        format!("/plans/{id}/x"),
    ] {
        assert_status(&server.get(&path), 404, &format!("GET {path}"));
    }
    assert_status(&server.delete("/plans/"), 404, "DELETE /plans/");
    assert_eq!(plans(&server).len(), 1);
}

// ---- 3.1 ----

#[test]
fn spec_3_1_accepts_json_content_type_with_charset() {
    let server = Server::start();
    let ct = "application/json; charset=utf-8";
    let res = server.request_with("POST", "/plans", ct, br#"{"name": "Household"}"#, false);
    assert_status(&res, 201, "POST with charset");
    let id = res.json()["id"].as_str().unwrap().to_owned();
    let res = server.request_with("PATCH", &format!("/plans/{id}"), ct, br#"{"name": "Home"}"#, false);
    assert_status(&res, 200, "PATCH with charset");
    assert_eq!(res.json()["name"], "Home");
}

#[test]
fn spec_3_1_accepts_chunked_request_body() {
    let server = Server::start();
    let ct = "application/json";
    let res = server.request_with("POST", "/plans", ct, br#"{"name": "Household"}"#, true);
    assert_status(&res, 201, "chunked POST");
    assert_eq!(res.json()["name"], "Household");
    let id = res.json()["id"].as_str().unwrap().to_owned();
    let res = server.request_with("PATCH", &format!("/plans/{id}"), ct, br#"{"currency_code": "EUR"}"#, true);
    assert_status(&res, 200, "chunked PATCH");
    assert_eq!(res.json()["currency_code"], "EUR");
    let res = server.request_with("POST", "/plans", ct, b"{\"name\": ", true);
    assert_status(&res, 400, "chunked invalid JSON");
}

#[test]
fn spec_3_1_accepts_json_whitespace_around_tokens() {
    let server = Server::start();
    let body = " \t\r\n{ \r\n\t\"name\" \t:\n \"Household\" ,\"currency_code\"\r:\"EUR\" \n} \t\r\n";
    let res = server.request_raw("POST", "/plans", body);
    assert_status(&res, 201, "POST with JSON whitespace");
    assert_eq!(res.json()["name"], "Household");
    assert_eq!(res.json()["currency_code"], "EUR");
    let id = res.json()["id"].as_str().unwrap().to_owned();
    let res = server.request_raw("PATCH", &format!("/plans/{id}"), "\n\n{}\n\n");
    assert_status(&res, 200, "PATCH {} surrounded by newlines");
}

#[test]
fn spec_3_1_error_bodies_from_parsing_have_json_content_type() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for (what, res) in [
        ("lone surrogate", server.request_raw("POST", "/plans", r#"{"name": "a\ud800"}"#)),
        ("non-object", server.request_raw("PATCH", &format!("/plans/{id}"), "[]")),
        ("PATCH /plans", server.request_raw("PATCH", "/plans", "{}")),
        ("DELETE missing plan", server.delete("/plans/nope")),
    ] {
        assert!(res.status >= 400, "{what}: status {}", res.status);
        assert!(res.is_json_content_type(), "{what}: Content-Type {:?}", res.content_type);
    }
}

// ---- 3.2 ----

#[test]
fn spec_3_2_more_error_responses_have_error() {
    let server = Server::start();
    let id = server.plan_id("Household");
    assert_error_body(&server.post("/no-such-path", &json!({})), 404, "POST unknown path");
    assert_error_body(&server.delete("/plans/x/y"), 404, "DELETE unknown path");
    assert_error_body(&server.get("/plans/"), 404, "GET /plans/");
    assert_error_body(&server.request_raw("POST", "/plans", r#"{"name": "\udc00"}"#), 400, "lone surrogate");
    assert_error_body(&server.request_raw("POST", "/plans", ""), 400, "empty body");
    assert_error_body(&server.request_bytes("PATCH", &format!("/plans/{id}"), None), 400, "missing body");
    assert_error_body(&server.request_raw("PATCH", "/plans", "{}"), 405, "PATCH /plans");
    assert_error_body(&server.request_raw("PUT", "/health", "{}"), 405, "PUT /health");
    assert_error_body(&server.request_raw("OPTIONS", "/plans", ""), 405, "OPTIONS /plans");
}

// ---- 3.4 ----

#[test]
fn spec_3_4_rejects_non_standard_json() {
    let server = Server::start();
    let id = server.plan_id("Household");
    // Each value sits in an ignored field, so only its JSON validity matters.
    for value in [
        "NaN",
        "Infinity",
        "-Infinity",
        "01",
        "-01",
        "+1",
        ".5",
        "1.",
        "1.e5",
        "1e",
        "1e+",
        "-",
        "0x10",
        "True",
        "nul",
        "undefined",
        "'single'",
        "\"tab\there\"",
        "\"new\nline\"",
        "\"nul\u{0}byte\"",
        r#""\x41""#,
        r#""\'""#,
        r#""\U0041""#,
        r#""\u12""#,
        r#""\uZZZZ""#,
        r#""\""#,
        "[1,]",
        "[,1]",
        "{\"a\" 1}",
        "{a: 1}",
        "{\"a\": 1,}",
        "1 /* c */",
        "1 // c",
    ] {
        let body = format!(r#"{{"name": "Home", "x": {value}}}"#);
        assert_status(&server.request_raw("POST", "/plans", &body), 400, &format!("POST {body:?}"));
        let res = server.request_raw("PATCH", &format!("/plans/{id}"), &body);
        assert_status(&res, 400, &format!("PATCH {body:?}"));
    }
    for body in [r#"{"name": "Home"}{}"#, r#"{"name": "Home"}]"#, r#"{"name": "Home"}}"#, r#"{"name": "Home"} 1"#] {
        assert_status(&server.request_raw("POST", "/plans", body), 400, &format!("POST {body:?}"));
    }
    assert_eq!(plans(&server).len(), 1, "invalid bodies created plans");
    assert_eq!(server.get(&format!("/plans/{id}")).json()["name"], "Household");
}

#[test]
fn spec_3_4_other_undefined_methods_return_405() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let plan = format!("/plans/{id}");
    for (method, path) in [
        ("OPTIONS", "/plans"),
        ("OPTIONS", plan.as_str()),
        ("PURGE", plan.as_str()),
        ("PATCH", "/health"),
        ("PUT", "/health"),
        ("POST", "/plans/no-such-plan"),
    ] {
        let res = server.request_raw(method, path, "{}");
        assert_status(&res, 405, &format!("{method} {path}"));
    }
    assert_eq!(server.get(&plan).json()["name"], "Household");
}

// ---- 3.6 ----

#[test]
fn spec_3_6_ignores_query_parameters_on_patch_and_delete() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let res = server.patch(&format!("/plans/{id}?name=Query&currency_code=EUR"), &json!({"name": "Body"}));
    assert_status(&res, 200, "PATCH with query");
    assert_eq!(res.json()["name"], "Body");
    assert_eq!(res.json()["currency_code"], "USD");
    assert_status(&server.get("/health?verbose=1"), 200, "GET /health with query");
    assert_status(&server.delete(&format!("/plans/{id}?force=true")), 204, "DELETE with query");
    assert!(plans(&server).is_empty());
}

// ---- 3.9 ----

#[test]
fn spec_3_9_counts_escaped_surrogate_pairs_as_one_character() {
    let server = Server::start();
    let body = format!(r#"{{"name": "{}"}}"#, r"\ud83d\ude00".repeat(100));
    let res = server.request_raw("POST", "/plans", &body);
    assert_status(&res, 201, "100 escaped emoji");
    assert_eq!(res.json()["name"], "😀".repeat(100));
    let body = format!(r#"{{"name": "{}"}}"#, r"\ud83d\ude00".repeat(101));
    assert_status(&server.request_raw("POST", "/plans", &body), 400, "101 escaped emoji");
    // Six bytes of escape for one character: 100 of them is still 100 characters.
    let body = format!(r#"{{"name": "{}"}}"#, r"\u00e9".repeat(100));
    let res = server.request_raw("POST", "/plans", &body);
    assert_status(&res, 201, "100 escaped é");
    assert_eq!(res.json()["name"], "é".repeat(100));
}

#[test]
fn spec_3_9_accepts_control_characters_that_are_not_white_space() {
    let server = Server::start();
    // U+001C..U+001F are separators in Unicode but not White_Space.
    for name in ["\u{0}", "a\u{0}b", "\u{1}\u{7f}", "\u{1b}[31m", "\u{1c}\u{1d}\u{1e}\u{1f}"] {
        let plan = server.create_plan(&json!({"name": name}));
        assert_eq!(plan["name"], name);
        let id = plan["id"].as_str().unwrap();
        assert_eq!(server.get(&format!("/plans/{id}")).json()["name"], name);
    }
}

#[test]
fn spec_3_9_rejects_other_white_space_only_names() {
    let server = Server::start();
    for name in ["\u{b}", "\u{c}", "\u{1680}", "\u{2000}", "\u{200a}", "\u{2029}", "\u{205f}"] {
        let res = server.post("/plans", &json!({"name": name}));
        assert_status(&res, 400, &format!("POST name {name:?}"));
    }
    assert!(plans(&server).is_empty());
}

// ---- 3.11 ----

#[test]
fn spec_3_11_concurrent_patches_apply_whole() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let codes = ["AAA", "BBB", "CCC", "DDD", "EEE", "FFF", "GGG", "HHH"];
    std::thread::scope(|s| {
        for (i, code) in codes.iter().enumerate() {
            let (server, id) = (&server, &id);
            s.spawn(move || {
                for round in 0..10 {
                    let body = json!({"name": format!("Name {code}"), "currency_code": code});
                    let res = server.patch(&format!("/plans/{id}"), &body);
                    assert_eq!(res.status, 200, "PATCH {i}/{round}: {}", res.text());
                    let got = res.json();
                    assert_eq!(got["name"], format!("Name {code}"), "PATCH response mixes two PATCHes: {got}");
                    assert_eq!(got["currency_code"], *code, "PATCH response mixes two PATCHes: {got}");
                }
            });
        }
    });
    let got = server.get(&format!("/plans/{id}")).json();
    let code = got["currency_code"].as_str().unwrap();
    assert_eq!(got["name"], format!("Name {code}"), "final plan mixes two PATCHes: {got}");
}

// ---- 3.13 ----

#[test]
fn spec_3_13_accepts_in_range_numbers_with_extreme_exponents() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let zeros = "0".repeat(400);
    // Every one of these is zero or a modest number, well inside the 64-bit float range.
    let values = [
        "0e400".to_owned(),
        "0E+999".to_owned(),
        "-0.0e400".to_owned(),
        "0e99999999999999999999".to_owned(),
        "1e-99999999999999999999".to_owned(),
        format!("0.{zeros}1e400"),
        format!("1{zeros}e-400"),
        format!("1{zeros}.5e-300"),
        format!("-0.{zeros}e400"),
    ];
    for value in &values {
        let body = format!(r#"{{"name": "Home", "x": {value}}}"#);
        let res = server.request_raw("POST", "/plans", &body);
        assert_status(&res, 201, &format!("POST /plans with x = {value}"));
        let res = server.request_raw("PATCH", &format!("/plans/{id}"), &body);
        assert_status(&res, 200, &format!("PATCH with x = {value}"));
    }
}

#[test]
fn spec_3_13_accepts_many_fields_and_long_keys_in_ignored_fields() {
    let server = Server::start();
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!("Home"));
    for i in 0..20_000 {
        obj.insert(format!("k{i}"), json!(i));
    }
    obj.insert("k".repeat(100_000), json!("long key"));
    let body = Value::Object(obj);
    assert!(body.to_string().len() < 1024 * 1024);
    let res = server.post("/plans", &body);
    assert_status(&res, 201, "POST with many ignored fields");
    assert_eq!(res.json()["name"], "Home");
    let arr: Vec<Value> = (0..100_000).map(|i| json!(i)).collect();
    let res = server.post("/plans", &json!({"name": "Big", "x": arr}));
    assert_status(&res, 201, "POST with a long ignored array");
}
