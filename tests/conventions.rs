use mybudget_testsuite::{Response, Server, Value, json};

const MISSING: &str = "/plans/no-such-plan-id";

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

// ---- 3.1 ----

#[test]
fn spec_3_1_json_responses_have_json_content_type() {
    let server = Server::start();
    let res = server.post("/plans", &json!({"name": "Household"}));
    assert!(res.is_json_content_type(), "POST /plans: Content-Type {:?}", res.content_type);
    let id = res.json()["id"].as_str().unwrap().to_owned();

    for (what, res) in [
        ("GET /plans", server.get("/plans")),
        ("GET /plans/{id}", server.get(&format!("/plans/{id}"))),
        ("PATCH /plans/{id}", server.patch(&format!("/plans/{id}"), &json!({"name": "Home"}))),
    ] {
        assert_status(&res, 200, what);
        assert!(res.is_json_content_type(), "{what}: Content-Type {:?}", res.content_type);
        res.json();
    }
}

#[test]
fn spec_3_1_error_responses_have_json_content_type() {
    let server = Server::start();
    for (what, res) in [
        ("GET /no-such-path", server.get("/no-such-path")),
        ("GET missing plan", server.get(MISSING)),
        ("POST /plans invalid JSON", server.request_raw("POST", "/plans", "{")),
        ("PUT /plans", server.request_raw("PUT", "/plans", "{}")),
    ] {
        assert!(res.is_json_content_type(), "{what}: Content-Type {:?}", res.content_type);
    }
}

#[test]
fn spec_3_1_missing_body_is_invalid_json() {
    let server = Server::start();
    let res = server.request_bytes("POST", "/plans", None);
    assert_status(&res, 400, "POST /plans with no body");
    let id = server.plan_id("Household");
    let res = server.request_bytes("PATCH", &format!("/plans/{id}"), None);
    assert_status(&res, 400, "PATCH /plans/{id} with no body");
}

#[test]
fn spec_3_1_empty_body_is_invalid_json() {
    let server = Server::start();
    let res = server.request_raw("POST", "/plans", "");
    assert_status(&res, 400, "POST /plans with empty body");
    let id = server.plan_id("Household");
    let res = server.request_raw("PATCH", &format!("/plans/{id}"), "");
    assert_status(&res, 400, "PATCH /plans/{id} with empty body");
}

#[test]
fn spec_3_1_body_that_is_not_utf8_returns_400() {
    let server = Server::start();
    let mut body = b"{\"name\": \"".to_vec();
    body.extend_from_slice(&[0xff, 0xfe, b'a']);
    body.extend_from_slice(b"\"}");
    let res = server.request_bytes("POST", "/plans", Some(body));
    assert_status(&res, 400, "POST /plans with invalid UTF-8");
}

#[test]
fn spec_3_1_accepts_non_ascii_utf8_body() {
    let server = Server::start();
    let res = server.request_raw("POST", "/plans", "{\"name\": \"Ménage 家計 💶\"}");
    assert_status(&res, 201, "POST /plans with UTF-8 name");
    assert_eq!(res.json()["name"], "Ménage 家計 💶");
}

// Bodies whose strings hold an escaped lone surrogate, each otherwise a valid plan.
const LONE_SURROGATE_BODIES: [&str; 6] = [
    r#"{"name": "a\ud800b"}"#,
    r#"{"name": "a\uDC00b"}"#,
    r#"{"name": "Household\ud83d"}"#,
    r#"{"name": "\ude00\ud83d"}"#,
    r#"{"name": "\ud800A"}"#,
    r#"{"name": "Household", "currency_code": "U\ud800D"}"#,
];

#[test]
fn spec_3_1_lone_surrogate_in_post_returns_400() {
    let server = Server::start();
    for body in LONE_SURROGATE_BODIES {
        assert_status(&server.request_raw("POST", "/plans", body), 400, &format!("POST /plans {body}"));
    }
    assert!(plans(&server).is_empty(), "lone-surrogate POSTs created plans: {:?}", plans(&server));
}

#[test]
fn spec_3_1_lone_surrogate_in_patch_returns_400() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in LONE_SURROGATE_BODIES {
        let res = server.request_raw("PATCH", &format!("/plans/{id}"), body);
        assert_status(&res, 400, &format!("PATCH /plans/{{id}} {body}"));
    }
    let got = server.get(&format!("/plans/{id}")).json();
    assert_eq!(got["name"], "Household");
    assert_eq!(got["currency_code"], "USD");
}

#[test]
fn spec_3_1_lone_surrogate_in_ignored_field_returns_400() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in [
        r#"{"name": "Home", "note": "x\ud800"}"#,
        r#"{"name": "Home", "extra": {"deep": ["ok", "\udfff"]}}"#,
        r#"{"name": "Home", "\ud800": 1}"#,
        r#"{"name": "Home", "a\udc00b": "x"}"#,
    ] {
        assert_status(&server.request_raw("POST", "/plans", body), 400, &format!("POST /plans {body}"));
        let res = server.request_raw("PATCH", &format!("/plans/{id}"), body);
        assert_status(&res, 400, &format!("PATCH /plans/{{id}} {body}"));
    }
    assert_eq!(plans(&server).len(), 1, "lone-surrogate POSTs created plans");
    assert_eq!(server.get(&format!("/plans/{id}")).json()["name"], "Household");
}

#[test]
fn spec_3_1_accepts_escaped_surrogate_pair() {
    let server = Server::start();
    let res = server.request_raw("POST", "/plans", r#"{"name": "a😀b"}"#);
    assert_status(&res, 201, "POST /plans with an escaped surrogate pair");
    assert_eq!(res.json()["name"], "a😀b");
    let id = res.json()["id"].as_str().unwrap().to_owned();
    let res = server.request_raw("PATCH", &format!("/plans/{id}"), r#"{"name": "💶", "x": "𐀀"}"#);
    assert_status(&res, 200, "PATCH with escaped surrogate pairs");
    assert_eq!(res.json()["name"], "💶");
}

// ---- 3.2 ----

#[test]
fn spec_3_2_unknown_path_404_has_error() {
    let server = Server::start();
    assert_error_body(&server.get("/no-such-path"), 404, "GET /no-such-path");
    assert_error_body(&server.get("/plans/x/no-such-thing/y/z"), 404, "deep unknown path");
}

#[test]
fn spec_3_2_missing_plan_404_has_error() {
    let server = Server::start();
    assert_error_body(&server.get(MISSING), 404, "GET missing plan");
    assert_error_body(&server.patch(MISSING, &json!({"name": "x"})), 404, "PATCH missing plan");
    assert_error_body(&server.delete(MISSING), 404, "DELETE missing plan");
}

#[test]
fn spec_3_2_bad_request_400_has_error() {
    let server = Server::start();
    assert_error_body(&server.request_raw("POST", "/plans", "not json"), 400, "invalid JSON");
    assert_error_body(&server.post("/plans", &json!({})), 400, "missing name");
    assert_error_body(&server.post("/plans", &json!({"name": 7})), 400, "name wrong type");
    let id = server.plan_id("Household");
    assert_error_body(
        &server.patch(&format!("/plans/{id}"), &json!({"currency_code": "usd"})),
        400,
        "bad currency in PATCH",
    );
}

#[test]
fn spec_3_2_method_not_allowed_405_has_error() {
    let server = Server::start();
    let id = server.plan_id("Household");
    assert_error_body(&server.request_raw("PUT", "/plans", "{}"), 405, "PUT /plans");
    assert_error_body(&server.delete("/plans"), 405, "DELETE /plans");
    assert_error_body(&server.post(&format!("/plans/{id}"), &json!({})), 405, "POST /plans/{id}");
    assert_error_body(&server.post("/health", &json!({})), 405, "POST /health");
}

// ---- 3.3 ----

#[test]
fn spec_3_3_post_create_returns_201_with_object() {
    let server = Server::start();
    let res = server.post("/plans", &json!({"name": "Household"}));
    assert_status(&res, 201, "POST /plans");
    let plan = res.json();
    assert!(plan.is_object(), "POST /plans returned {plan}");
    assert_eq!(plan["name"], "Household");
}

#[test]
fn spec_3_3_delete_returns_204_with_no_body() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let res = server.delete(&format!("/plans/{id}"));
    assert_status(&res, 204, "DELETE /plans/{id}");
    assert!(res.body.is_empty(), "DELETE returned a body: {:?}", res.text());
}

#[test]
fn spec_3_3_get_and_patch_return_200() {
    let server = Server::start();
    let id = server.plan_id("Household");
    assert_status(&server.get("/plans"), 200, "GET /plans");
    assert_status(&server.get(&format!("/plans/{id}")), 200, "GET /plans/{id}");
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": "Home"}));
    assert_status(&res, 200, "PATCH /plans/{id}");
    assert_eq!(res.json()["name"], "Home");
}

// ---- 3.4 ----

#[test]
fn spec_3_4_invalid_json_returns_400() {
    let server = Server::start();
    for body in ["{", "{\"name\": }", "{'name': 'x'}", "{\"name\": \"x\"} trailing", "{\"name\": \"x\",}", "nope"] {
        assert_status(&server.request_raw("POST", "/plans", body), 400, &format!("POST /plans {body:?}"));
    }
    let id = server.plan_id("Household");
    let res = server.request_raw("PATCH", &format!("/plans/{id}"), "{\"name\": ");
    assert_status(&res, 400, "PATCH invalid JSON");
}

#[test]
fn spec_3_4_non_object_json_returns_400() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in ["[]", "[{\"name\": \"x\"}]", "\"Household\"", "42", "true", "null"] {
        assert_status(&server.request_raw("POST", "/plans", body), 400, &format!("POST /plans {body}"));
        let res = server.request_raw("PATCH", &format!("/plans/{id}"), body);
        assert_status(&res, 400, &format!("PATCH /plans/{{id}} {body}"));
    }
}

#[test]
fn spec_3_4_missing_required_field_returns_400() {
    let server = Server::start();
    assert_status(&server.post("/plans", &json!({})), 400, "POST /plans {}");
    assert_status(&server.post("/plans", &json!({"currency_code": "EUR"})), 400, "POST /plans without name");
}

#[test]
fn spec_3_4_wrong_json_type_returns_400() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in [
        json!({"name": 1}),
        json!({"name": true}),
        json!({"name": ["Household"]}),
        json!({"name": {"text": "Household"}}),
        json!({"name": "Household", "currency_code": 840}),
        json!({"name": "Household", "currency_code": ["USD"]}),
    ] {
        assert_status(&server.post("/plans", &body), 400, &format!("POST /plans {body}"));
        assert_status(&server.patch(&format!("/plans/{id}"), &body), 400, &format!("PATCH {body}"));
    }
}

#[test]
fn spec_3_4_path_naming_nothing_returns_404() {
    let server = Server::start();
    assert_status(&server.get(MISSING), 404, "GET missing plan");
    assert_status(&server.patch(MISSING, &json!({"name": "x"})), 404, "PATCH missing plan");
    assert_status(&server.delete(MISSING), 404, "DELETE missing plan");
}

#[test]
fn spec_3_4_undefined_method_returns_405() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let plan = format!("/plans/{id}");
    for (method, path) in [
        ("PUT", "/plans"),
        ("PATCH", "/plans"),
        ("DELETE", "/plans"),
        ("POST", plan.as_str()),
        ("PUT", plan.as_str()),
        ("POST", "/health"),
        ("DELETE", "/health"),
    ] {
        let res = server.request_raw(method, path, "{}");
        assert_status(&res, 405, &format!("{method} {path}"));
    }
}

#[test]
fn spec_3_4_method_not_allowed_wins_over_not_found() {
    let server = Server::start();
    let res = server.post(MISSING, &json!({"name": "x"}));
    assert_status(&res, 405, "POST to a missing plan");
    let res = server.request_raw("PUT", MISSING, "{}");
    assert_status(&res, 405, "PUT to a missing plan");
}

#[test]
fn spec_3_4_method_not_allowed_wins_over_bad_request() {
    let server = Server::start();
    assert_status(&server.request_raw("PUT", "/plans", "not json"), 405, "PUT /plans invalid JSON");
    assert_status(&server.request_raw("PATCH", "/plans", "[]"), 405, "PATCH /plans non-object");
}

#[test]
fn spec_3_4_not_found_wins_over_bad_request() {
    let server = Server::start();
    assert_status(&server.request_raw("PATCH", MISSING, "not json"), 404, "PATCH missing plan, invalid JSON");
    assert_status(&server.patch(MISSING, &json!({"name": 5})), 404, "PATCH missing plan, bad name");
    assert_status(&server.patch(MISSING, &json!({"currency_code": "x"})), 404, "PATCH missing plan, bad currency");
}

// ---- 3.5 ----

#[test]
fn spec_3_5_ids_are_unique_strings() {
    let server = Server::start();
    let mut ids = std::collections::HashSet::new();
    for i in 0..20 {
        let plan = server.create_plan(&json!({"name": format!("Plan {i}")}));
        let id = plan["id"].as_str().unwrap_or_else(|| panic!("id is not a string: {plan}"));
        assert!(ids.insert(id.to_owned()), "id {id} was returned twice");
    }
}

#[test]
fn spec_3_5_ids_are_not_reused_after_delete() {
    let server = Server::start();
    let mut deleted = Vec::new();
    for i in 0..5 {
        let id = server.plan_id(&format!("Plan {i}"));
        assert_status(&server.delete(&format!("/plans/{id}")), 204, "DELETE plan");
        deleted.push(id);
    }
    for i in 0..10 {
        let id = server.plan_id(&format!("New {i}"));
        assert!(!deleted.contains(&id), "deleted id {id} was reused");
    }
}

#[test]
fn spec_3_5_ignores_client_supplied_id() {
    let server = Server::start();
    let plan = server.create_plan(&json!({"name": "Household", "id": "my-own-id"}));
    assert_ne!(plan["id"], "my-own-id", "server used the client's id");
    assert_status(&server.get("/plans/my-own-id"), 404, "GET client-chosen id");
}

// ---- 3.6 ----

#[test]
fn spec_3_6_ignores_undefined_request_fields() {
    let server = Server::start();
    let res = server.post("/plans", &json!({"name": "Household", "colour": "blue", "budget": {"x": [1]}}));
    assert_status(&res, 201, "POST /plans with extra fields");
    let plan = res.json();
    assert_eq!(plan["name"], "Household");
    assert_eq!(plan["currency_code"], "USD");
    let id = plan["id"].as_str().unwrap();
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": "Home", "colour": null}));
    assert_status(&res, 200, "PATCH with extra fields");
    assert_eq!(res.json()["name"], "Home");
}

#[test]
fn spec_3_6_ignores_undefined_query_parameters() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let res = server.get("/plans?limit=0&sort=desc");
    assert_status(&res, 200, "GET /plans with query");
    assert_eq!(res.json().as_array().map(Vec::len), Some(1));
    let res = server.get(&format!("/plans/{id}?verbose=true"));
    assert_status(&res, 200, "GET /plans/{id} with query");
    assert_eq!(res.json()["id"], id.as_str());
    let res = server.request_raw("POST", "/plans?name=Other", "{\"name\": \"Household 2\"}");
    assert_status(&res, 201, "POST /plans with query");
    assert_eq!(res.json()["name"], "Household 2");
}

#[test]
fn spec_3_6_plan_response_has_every_example_field() {
    let server = Server::start();
    let plan = server.create_plan(&json!({"name": "Household"}));
    let id = plan["id"].as_str().unwrap().to_owned();
    let patched = server.patch(&format!("/plans/{id}"), &json!({"currency_code": "EUR"})).json();
    let got = server.get(&format!("/plans/{id}")).json();
    let listed = plans(&server)[0].clone();
    for (what, obj) in [("POST", plan), ("PATCH", patched), ("GET", got), ("list", listed)] {
        for field in ["id", "name", "currency_code"] {
            assert!(obj.get(field).is_some(), "{what} response lacks `{field}`: {obj}");
        }
    }
}

// ---- 3.9 ----

#[test]
fn spec_3_9_accepts_names_of_1_and_100_characters() {
    let server = Server::start();
    for name in ["a".to_owned(), "a".repeat(100), "€".repeat(100), "😀".repeat(100)] {
        let plan = server.create_plan(&json!({"name": name}));
        assert_eq!(plan["name"], name.as_str());
    }
}

#[test]
fn spec_3_9_rejects_names_over_100_characters() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for name in ["a".repeat(101), "😀".repeat(101), "x".repeat(1000)] {
        assert_status(&server.post("/plans", &json!({"name": name})), 400, "POST long name");
        assert_status(&server.patch(&format!("/plans/{id}"), &json!({"name": name})), 400, "PATCH long name");
    }
}

#[test]
fn spec_3_9_counts_scalar_values_not_graphemes() {
    let server = Server::start();
    // "e" + COMBINING ACUTE ACCENT is one grapheme but two scalar values.
    let ok = "e\u{301}".repeat(50);
    let plan = server.create_plan(&json!({"name": ok}));
    assert_eq!(plan["name"], ok.as_str(), "name was not stored exactly (normalised?)");
    let too_long = "e\u{301}".repeat(51);
    assert_status(&server.post("/plans", &json!({"name": too_long})), 400, "102 scalar values");
}

#[test]
fn spec_3_9_rejects_empty_name() {
    let server = Server::start();
    let id = server.plan_id("Household");
    assert_status(&server.post("/plans", &json!({"name": ""})), 400, "POST empty name");
    assert_status(&server.patch(&format!("/plans/{id}"), &json!({"name": ""})), 400, "PATCH empty name");
}

#[test]
fn spec_3_9_rejects_whitespace_only_names() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for name in [" ", "   ", "\t", "\n\r", "\u{a0}", "\u{3000}", "\u{2028}", "\u{85}", " \u{2003}\u{202f} "] {
        assert_status(&server.post("/plans", &json!({"name": name})), 400, &format!("POST name {name:?}"));
        let res = server.patch(&format!("/plans/{id}"), &json!({"name": name}));
        assert_status(&res, 400, &format!("PATCH name {name:?}"));
    }
}

#[test]
fn spec_3_9_accepts_non_white_space_invisible_characters() {
    let server = Server::start();
    // ZERO WIDTH SPACE and ZERO WIDTH NO-BREAK SPACE aren't White_Space.
    for name in ["\u{200b}", "\u{feff}"] {
        let plan = server.create_plan(&json!({"name": name}));
        assert_eq!(plan["name"], name);
    }
}

#[test]
fn spec_3_9_stores_names_exactly_as_given() {
    let server = Server::start();
    for name in ["  Household  ", "a\tb\nc", "Cafe\u{301}", "Café", "<b>&amp;</b> \"quoted\" \\ /"] {
        let plan = server.create_plan(&json!({"name": name}));
        assert_eq!(plan["name"], name, "POST response");
        let id = plan["id"].as_str().unwrap();
        assert_eq!(server.get(&format!("/plans/{id}")).json()["name"], name, "GET");
    }
    let id = server.plan_id("Household");
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": " Home "}));
    assert_eq!(res.json()["name"], " Home ");
    assert_eq!(server.get(&format!("/plans/{id}")).json()["name"], " Home ");
}

#[test]
fn spec_3_9_plans_may_share_a_name() {
    let server = Server::start();
    let a = server.plan_id("Household");
    let b = server.plan_id("Household");
    assert_ne!(a, b);
    let res = server.patch(&format!("/plans/{a}"), &json!({"name": "Other"}));
    assert_status(&res, 200, "rename");
    let res = server.patch(&format!("/plans/{a}"), &json!({"name": "Household"}));
    assert_status(&res, 200, "rename back to a shared name");
}

// ---- 3.10 ----

#[test]
fn spec_3_10_null_for_non_nullable_field_returns_400() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in [json!({"name": null}), json!({"name": "Household", "currency_code": null})] {
        assert_status(&server.post("/plans", &body), 400, &format!("POST {body}"));
    }
    for body in [json!({"name": null}), json!({"currency_code": null})] {
        assert_status(&server.patch(&format!("/plans/{id}"), &body), 400, &format!("PATCH {body}"));
    }
    let plan = server.get(&format!("/plans/{id}")).json();
    assert_eq!(plan["name"], "Household");
    assert_eq!(plan["currency_code"], "USD");
}

#[test]
fn spec_3_10_patch_changes_only_present_fields() {
    let server = Server::start();
    let plan = server.create_plan(&json!({"name": "Household", "currency_code": "EUR"}));
    let id = plan["id"].as_str().unwrap();
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": "Home"}));
    let patched = res.json();
    assert_eq!(patched["id"], id);
    assert_eq!(patched["name"], "Home");
    assert_eq!(patched["currency_code"], "EUR");
    let got = server.get(&format!("/plans/{id}")).json();
    assert_eq!(got["name"], "Home");
    assert_eq!(got["currency_code"], "EUR");
    let res = server.patch(&format!("/plans/{id}"), &json!({"currency_code": "GBP"}));
    assert_eq!(res.json()["name"], "Home");
    let got = server.get(&format!("/plans/{id}")).json();
    assert_eq!(got["name"], "Home");
    assert_eq!(got["currency_code"], "GBP");
}

#[test]
fn spec_3_10_patch_without_known_fields_changes_nothing() {
    let server = Server::start();
    let plan = server.create_plan(&json!({"name": "Household", "currency_code": "JPY"}));
    let id = plan["id"].as_str().unwrap();
    for body in [json!({}), json!({"nickname": "x", "id": "other"})] {
        let res = server.patch(&format!("/plans/{id}"), &body);
        assert_status(&res, 200, &format!("PATCH {body}"));
        let got = res.json();
        assert_eq!(got["id"], id);
        assert_eq!(got["name"], "Household");
        assert_eq!(got["currency_code"], "JPY");
    }
    let got = server.get(&format!("/plans/{id}")).json();
    assert_eq!(got["name"], "Household");
    assert_eq!(got["currency_code"], "JPY");
}

// ---- 3.11 ----

#[test]
fn spec_3_11_failed_post_creates_nothing() {
    let server = Server::start();
    for body in [
        json!({"name": "Household", "currency_code": "usd"}),
        json!({"name": "", "currency_code": "EUR"}),
        json!({"currency_code": "EUR"}),
    ] {
        assert_status(&server.post("/plans", &body), 400, &format!("POST {body}"));
    }
    assert!(plans(&server).is_empty(), "failed POSTs created plans: {:?}", plans(&server));
}

#[test]
fn spec_3_11_failed_patch_changes_nothing() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in [
        json!({"name": "Home", "currency_code": "euro"}),
        json!({"currency_code": "EUR", "name": ""}),
        json!({"currency_code": "EUR", "name": null}),
    ] {
        assert_status(&server.patch(&format!("/plans/{id}"), &body), 400, &format!("PATCH {body}"));
        let got = server.get(&format!("/plans/{id}")).json();
        assert_eq!(got["name"], "Household", "after PATCH {body}");
        assert_eq!(got["currency_code"], "USD", "after PATCH {body}");
    }
}

#[test]
fn spec_3_11_concurrent_creates_all_succeed() {
    let server = Server::start();
    let ids: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..32)
            .map(|i| {
                let server = &server;
                s.spawn(move || server.plan_id(&format!("Plan {i}")))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "concurrent creates returned duplicate ids");
    let listed = plans(&server);
    assert_eq!(listed.len(), 32);
    for id in &ids {
        assert!(listed.iter().any(|p| p["id"] == id.as_str()), "plan {id} missing from list");
    }
}

#[test]
fn spec_3_11_concurrent_deletes_succeed_once() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let statuses: Vec<u16> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let (server, id) = (&server, &id);
                s.spawn(move || server.delete(&format!("/plans/{id}")).status)
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(statuses.iter().filter(|&&s| s == 204).count(), 1, "statuses {statuses:?}");
    assert_eq!(statuses.iter().filter(|&&s| s == 404).count(), 15, "statuses {statuses:?}");
}

// ---- 3.12 ----

#[test]
fn spec_3_12_empty_list_is_json_array() {
    let server = Server::start();
    let res = server.get("/plans");
    assert_status(&res, 200, "GET /plans");
    assert_eq!(res.json(), json!([]));
}
