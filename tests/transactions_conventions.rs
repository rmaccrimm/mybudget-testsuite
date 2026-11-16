//! Sections 2 to 4 as they apply to transactions (sections 8 to 10).

use std::collections::HashSet;

use mybudget_testsuite::{
    Budget, Server, TRANSACTION_FIELDS, Value, assert_error, assert_fields, assert_matches, assert_status, id_of,
    ids_of, json, merged, spec_fields, str_field, with,
};

fn is_path_safe(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A transfer from Checking to Savings; returns (out, in).
fn transfer(server: &Server, b: &Budget) -> (Value, Value) {
    let out = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -500, "payee_id": b.savings.payee}),
    );
    let inn = server.transaction(&b.plan, &str_field(&out, "transfer_transaction_id"));
    (out, inn)
}

// ---- 2.2 ----

#[test]
fn spec_2_2_undefined_transaction_paths_return_404() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-100)));
    for rest in [
        "/transactions/".to_owned(),
        format!("/transactions/{id}/"),
        format!("/transactions/{id}/subtransactions"),
        format!("/transactions//{id}"),
        "/transaction".to_owned(),
        "/Transactions".to_owned(),
        format!("/accounts/{}/transactions", b.checking.id),
        format!("/categories/{}/transactions", b.groceries),
        format!("/payees/{}/transactions", b.checking.payee),
    ] {
        let path = b.path(&rest);
        assert_error(&server.get(&path), 404, &format!("GET {path}"));
    }
}

// ---- 3.1 ----

#[test]
fn spec_3_1_transaction_responses_have_json_content_type() {
    let server = Server::start();
    let b = Budget::new(&server);
    let res = server.post_transaction(&b.plan, &b.spend(-100));
    assert!(res.is_json_content_type(), "POST: {:?}", res.content_type);
    let id = id_of(&res.json());
    for (what, res) in [
        ("GET list", server.get(&b.path("/transactions"))),
        ("GET filtered list", server.get(&b.path("/transactions?approved=true"))),
        ("GET one", server.get_transaction(&b.plan, &id)),
        ("PATCH", server.patch_transaction(&b.plan, &id, &json!({"memo": "x"}))),
        ("400", server.post_transaction(&b.plan, &json!({}))),
        ("404", server.get_transaction(&b.plan, "nope")),
    ] {
        assert!(res.is_json_content_type(), "{what}: {:?}", res.content_type);
    }
}

#[test]
fn spec_3_1_transaction_bodies_must_be_json_objects() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-100)));
    let before = server.plan_snapshot(&b.plan);
    for body in ["", "not json", "[]", "null", "5", "\"x\"", "{", r#"{"memo": "a\ud800b"}"#] {
        let res = server.request_raw("POST", &b.path("/transactions"), body);
        assert_error(&res, 400, &format!("POST {body:?}"));
        let res = server.request_raw("PATCH", &b.txn_path(&id), body);
        assert_error(&res, 400, &format!("PATCH {body:?}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

// ---- 3.2 ----

#[test]
fn spec_3_2_head_on_transactions_has_no_body() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-100)));
    for path in [b.path("/transactions"), b.txn_path(&id), b.path("/transactions?account_id=x"), b.txn_path("nope")] {
        let res = server.head(&path);
        assert!(res.body.is_empty(), "HEAD {path} has a body: {:?}", res.text());
    }
}

// ---- 3.3 ----

#[test]
fn spec_3_3_transaction_statuses() {
    let server = Server::start();
    let b = Budget::new(&server);
    let res = server.post_transaction(&b.plan, &b.spend(-100));
    assert_status(&res, 201, "POST");
    let id = id_of(&res.json());
    assert_status(&server.get_transaction(&b.plan, &id), 200, "GET");
    assert_status(&server.get(&b.path("/transactions")), 200, "GET list");
    let res = server.patch_transaction(&b.plan, &id, &json!({"memo": "x"}));
    assert_status(&res, 200, "PATCH");
    assert_eq!(res.json()["memo"], "x");
    let res = server.delete_transaction(&b.plan, &id);
    assert_status(&res, 204, "DELETE");
    assert!(res.body.is_empty());
}

// ---- 3.4 ----

#[test]
fn spec_3_4_undefined_methods_on_transactions_return_405() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-100)));
    let before = server.plan_snapshot(&b.plan);
    for (method, path) in [
        ("PUT", b.path("/transactions")),
        ("PATCH", b.path("/transactions")),
        ("DELETE", b.path("/transactions")),
        ("POST", b.txn_path(&id)),
        ("PUT", b.txn_path(&id)),
        ("POST", b.txn_path("nope")),
        ("PUT", b.txn_path("nope")),
        ("PATCH", "/plans/nope/transactions".to_owned()),
        ("POST", "/plans/nope/transactions/nope".to_owned()),
    ] {
        let res = server.request_raw(method, &path, &b.spend(-1).to_string());
        assert_error(&res, 405, &format!("{method} {path}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

#[test]
fn spec_3_4_head_works_on_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-100)));
    assert_status(&server.head(&b.path("/transactions")), 200, "HEAD list");
    assert_status(&server.head(&b.txn_path(&id)), 200, "HEAD one");
    assert_status(&server.head(&b.txn_path("nope")), 404, "HEAD missing");
    assert_status(&server.head("/plans/nope/transactions"), 404, "HEAD missing plan");
    assert_status(&server.head(&b.path("/transactions?approved=maybe")), 400, "HEAD bad query");
}

#[test]
fn spec_3_4_not_found_wins_over_bad_request_for_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    assert_error(&server.request_raw("POST", "/plans/nope/transactions", "not json"), 404, "POST missing plan, bad JSON");
    assert_error(&server.post("/plans/nope/transactions", &json!({"amount": 1.5})), 404, "POST missing plan, bad amount");
    assert_error(&server.request_raw("PATCH", &b.txn_path("nope"), "not json"), 404, "PATCH missing, bad JSON");
    assert_error(&server.patch_transaction(&b.plan, "nope", &json!({"account_id": "x"})), 404, "PATCH missing, bad field");
    assert_error(&server.get("/plans/nope/transactions?since_date=bad"), 404, "GET missing plan, bad query");
    assert_error(&server.get(&b.txn_path("nope")), 404, "GET missing transaction");
}

#[test]
fn spec_3_4_bad_request_wins_over_conflict_for_transfer_payees() {
    let server = Server::start();
    let b = Budget::new(&server);
    transfer(&server, &b);
    assert_error(&server.delete(&b.path(&format!("/payees/{}", b.savings.payee))), 400, "DELETE used transfer payee");
}

// ---- 3.5 ----

#[test]
fn spec_3_5_transaction_ids_are_unique_and_path_safe() {
    let server = Server::start();
    let b = Budget::new(&server);
    let mut ids: Vec<String> = vec![
        b.plan.clone(),
        b.checking.id.clone(),
        b.checking.payee.clone(),
        b.savings.id.clone(),
        b.savings.payee.clone(),
        b.groceries.clone(),
        b.group.clone(),
    ];
    for _ in 0..3 {
        ids.push(id_of(&server.create_transaction(&b.plan, &b.spend(-100))));
    }
    let (out, inn) = transfer(&server, &b);
    ids.push(id_of(&out));
    ids.push(id_of(&inn));
    let split = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -3,
                "subtransactions": [{"amount": -1, "category_id": b.groceries}, {"amount": -2, "category_id": b.household}]}),
    );
    ids.push(id_of(&split));
    let unique: HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "ids repeat: {ids:?}");
    for id in &ids {
        assert!(is_path_safe(id), "id {id:?} isn't path-safe");
        assert_ne!(id, "ready_to_assign");
    }
}

#[test]
fn spec_3_5_transaction_ids_are_not_reused() {
    let server = Server::start();
    let b = Budget::new(&server);
    let mut seen = HashSet::new();
    for _ in 0..3 {
        let (out, inn) = transfer(&server, &b);
        let plain = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
        for id in [id_of(&out), id_of(&inn), plain.clone()] {
            assert!(seen.insert(id.clone()), "id {id} was reused");
        }
        server.delete_transaction(&b.plan, &id_of(&out));
        server.delete_transaction(&b.plan, &plain);
    }
    let txn = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    server.patch_transaction_ok(&b.plan, &txn, &json!({"payee_id": b.savings.payee}));
    let first = str_field(&server.transaction(&b.plan, &txn), "transfer_transaction_id");
    server.patch_transaction_ok(&b.plan, &txn, &json!({"payee_id": b.wallet.payee}));
    let second = str_field(&server.transaction(&b.plan, &txn), "transfer_transaction_id");
    assert!(seen.insert(first.clone()), "id {first} was reused");
    assert!(seen.insert(second.clone()), "id {second} was reused");
}

#[test]
fn spec_3_5_ignores_client_supplied_transaction_id() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-1), "id", Some(json!("my-id"))));
    assert_ne!(id_of(&txn), "my-id");
    assert_error(&server.get_transaction(&b.plan, "my-id"), 404, "GET client-supplied id");
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"id": "other-id"}));
    assert_eq!(got["id"], txn["id"]);
}

// ---- 3.6 ----

#[test]
fn spec_3_6_transaction_responses_have_every_field() {
    let server = Server::start();
    let b = Budget::new(&server);
    let plain = server.create_transaction(&b.plan, &b.spend(-1));
    assert_fields(&plain, &TRANSACTION_FIELDS, "POST");
    let (out, inn) = transfer(&server, &b);
    assert_fields(&out, &TRANSACTION_FIELDS, "POST transfer");
    assert_fields(&inn, &TRANSACTION_FIELDS, "GET other half");
    let patched = server.patch_transaction_ok(&b.plan, &id_of(&plain), &json!({"memo": null}));
    assert_fields(&patched, &TRANSACTION_FIELDS, "PATCH");
    for t in server.transactions(&b.plan) {
        assert_fields(&t, &TRANSACTION_FIELDS, "list item");
    }
    let split = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -3,
                "subtransactions": [{"amount": -1, "category_id": b.groceries}, {"amount": -2, "category_id": b.household}]}),
    );
    for sub in split["subtransactions"].as_array().unwrap() {
        assert_fields(sub, &["amount", "category_id", "memo"], "subtransaction");
    }
}

#[test]
fn spec_3_6_transaction_post_ignores_undefined_fields() {
    let server = Server::start();
    let b = Budget::new(&server);
    let extra = json!({"transfer_account_id": b.savings.id, "transfer_transaction_id": "x", "plan_id": "x",
                       "deleted": true, "import_id": "abc", "payee": {"name": "x"}, "name": 5});
    let txn = server.create_transaction(&b.plan, &merged(&b.spend(-100), &extra));
    assert_matches(&txn, &json!({"transfer_account_id": null, "transfer_transaction_id": null, "payee_id": null}), "POST");
    assert_eq!(server.transactions(&b.plan).len(), 1);
}

#[test]
fn spec_3_6_transaction_patch_ignores_undefined_fields() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-100));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&txn),
        &json!({"transfer_account_id": b.savings.id, "transfer_transaction_id": "x", "memo": "m"}),
    );
    assert_matches(&got, &json!({"transfer_account_id": null, "memo": "m"}), "PATCH");
    assert_eq!(server.transactions(&b.plan).len(), 1);
}

#[test]
fn spec_3_6_subtransactions_ignore_undefined_fields() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -3,
                      "subtransactions": [{"amount": -1, "category_id": b.groceries, "id": "x", "note": 5},
                                          {"amount": -2, "category_id": b.household, "deleted": false}]});
    let txn = server.create_transaction(&b.plan, &body);
    assert_matches(
        &txn["subtransactions"],
        &json!([{"amount": -1, "category_id": b.groceries, "memo": null}, {"amount": -2, "category_id": b.household, "memo": null}]),
        "POST",
    );
}

#[test]
fn spec_3_6_transaction_list_ignores_undefined_query_parameters() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(&b.plan, &b.spend(-1));
    server.create_transaction(&b.plan, &b.spend(-2));
    let all = server.transactions(&b.plan);
    for query in ["foo=bar", "type=uncategorized", "last_knowledge_of_server=5", "date=bad", "account=nope", "cleared=x"] {
        assert_eq!(server.transactions_where(&b.plan, query), all, "?{query}");
    }
}

// ---- 3.7 ----

#[test]
fn spec_3_7_transaction_amounts_must_be_integers_in_range() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let before = server.plan_snapshot(&b.plan);
    let account = &b.checking.id;
    let category = &b.groceries;
    for amount in ["1.0", "1.5", "1e3", "1E3", "-0.0", "1000000000001", "-1000000000001", "\"5\"", "true", "[1]", "{}"] {
        let body = format!(r#"{{"account_id": "{account}", "date": "2026-10-04", "amount": {amount}, "category_id": "{category}"}}"#);
        assert_error(&server.request_raw("POST", &b.path("/transactions"), &body), 400, &format!("POST amount {amount}"));
        let body = format!(r#"{{"amount": {amount}}}"#);
        assert_error(&server.request_raw("PATCH", &b.txn_path(&id), &body), 400, &format!("PATCH amount {amount}"));
        let body = format!(
            r#"{{"account_id": "{account}", "date": "2026-10-04", "amount": 1, "subtransactions": [{{"amount": {amount}, "category_id": "{category}"}}]}}"#
        );
        assert_error(&server.request_raw("POST", &b.path("/transactions"), &body), 400, &format!("POST sub amount {amount}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

#[test]
fn spec_3_7_transaction_amount_bounds_are_accepted() {
    let server = Server::start();
    let b = Budget::new(&server);
    for amount in [1_000_000_000_000_i64, -1_000_000_000_000, 0, 1, -1] {
        let txn = server.create_transaction(&b.plan, &b.spend(amount));
        assert_eq!(txn["amount"], amount);
        let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"amount": -amount}));
        assert_eq!(got["amount"], -amount);
    }
}

#[test]
fn spec_3_7_negative_zero_transaction_amount_means_zero() {
    let server = Server::start();
    let b = Budget::new(&server);
    let account = &b.checking.id;
    let category = &b.groceries;
    let body = format!(r#"{{"account_id": "{account}", "date": "2026-10-04", "amount": -0, "category_id": "{category}"}}"#);
    let res = server.request_raw("POST", &b.path("/transactions"), &body);
    assert_status(&res, 201, "POST amount -0");
    let txn = res.json();
    assert_eq!(txn["amount"], 0);
    assert!(txn["amount"].is_i64() || txn["amount"].is_u64(), "{txn}");
    let res = server.request_raw("PATCH", &b.txn_path(&id_of(&txn)), r#"{"amount": 5}"#);
    assert_status(&res, 200, "PATCH amount 5");
    let res = server.request_raw("PATCH", &b.txn_path(&id_of(&txn)), r#"{"amount": -0}"#);
    assert_status(&res, 200, "PATCH amount -0");
    assert_eq!(res.json()["amount"], 0);
    let body = format!(
        r#"{{"account_id": "{account}", "date": "2026-10-04", "amount": 0, "subtransactions": [{{"amount": -0, "category_id": "{category}"}}]}}"#
    );
    let res = server.request_raw("POST", &b.path("/transactions"), &body);
    assert_status(&res, 201, "POST split of 0 as -0");
    assert_eq!(res.json()["subtransactions"][0]["amount"], 0);
    let body = format!(
        r#"{{"account_id": "{account}", "date": "2026-10-04", "amount": -0, "subtransactions": [{{"amount": 0, "category_id": "{category}"}}]}}"#
    );
    assert_status(&server.request_raw("POST", &b.path("/transactions"), &body), 201, "POST split of -0 as 0");
}

// ---- 3.8 ----

#[test]
fn spec_3_8_transaction_dates_must_be_real_dates_in_range() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let before = server.plan_snapshot(&b.plan);
    for bad in [
        json!("2026-02-29"),
        json!("2026-04-31"),
        json!("2026-13-01"),
        json!("2026-00-10"),
        json!("2026-10-00"),
        json!("1899-12-31"),
        json!("3000-01-01"),
        json!("2026-10-4"),
        json!("26-10-04"),
        json!("2026/10/04"),
        json!("2026-10-04T00:00:00Z"),
        json!(" 2026-10-04"),
        json!("2026-10"),
        json!(""),
        json!(20261004),
        json!(["2026-10-04"]),
    ] {
        let body = with(&b.spend(-1), "date", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST date {bad}"));
        let res = server.patch_transaction(&b.plan, &id, &json!({"date": bad}));
        assert_error(&res, 400, &format!("PATCH date {bad}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

#[test]
fn spec_3_8_transaction_dates_accept_any_real_date_in_range() {
    let server = Server::start();
    let b = Budget::new(&server);
    for date in ["1900-01-01", "2999-12-31", "2000-02-29", "2028-02-29", "2026-12-31", "2099-01-01"] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-1), "date", Some(json!(date))));
        assert_eq!(txn["date"], date);
        let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"date": date}));
        assert_eq!(got["date"], date);
    }
}

// ---- 3.9 ----

#[test]
fn spec_3_9_memos_are_up_to_500_characters() {
    let server = Server::start();
    let b = Budget::new(&server);
    for memo in ["x".repeat(500), "\u{1f600}".repeat(500), "\u{e9}".repeat(500), " ".repeat(500)] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-1), "memo", Some(json!(memo))));
        assert_eq!(txn["memo"], json!(memo));
        let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"memo": memo}));
        assert_eq!(got["memo"], json!(memo));
    }
    for memo in ["x".repeat(501), "\u{1f600}".repeat(501)] {
        let res = server.post_transaction(&b.plan, &with(&b.spend(-1), "memo", Some(json!(memo))));
        assert_error(&res, 400, "POST memo of 501 characters");
    }
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let res = server.patch_transaction(&b.plan, &id, &json!({"memo": "y".repeat(501)}));
    assert_error(&res, 400, "PATCH memo of 501 characters");
}

// ---- 3.10 ----

#[test]
fn spec_3_10_null_for_non_nullable_transaction_fields_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let before = server.plan_snapshot(&b.plan);
    for field in ["account_id", "date", "amount", "cleared", "approved", "subtransactions"] {
        let body = with(&b.spend(-1), field, Some(Value::Null));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST {field} null"));
        if field != "account_id" {
            let res = server.patch_transaction(&b.plan, &id, &with(&json!({}), field, Some(Value::Null)));
            assert_error(&res, 400, &format!("PATCH {field} null"));
        }
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

#[test]
fn spec_3_10_transaction_patch_without_known_fields_changes_nothing() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &merged(&b.spend(-1), &json!({"memo": "m", "flag_color": "red"})));
    let before = server.plan_snapshot(&b.plan);
    for body in [json!({}), json!({"foo": 1}), json!({"name": "x", "note": null})] {
        let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &body);
        assert_eq!(spec_fields(&got), spec_fields(&txn), "PATCH {body}");
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

// ---- 3.11 ----

#[test]
fn spec_3_11_failed_transaction_post_creates_nothing() {
    let server = Server::start();
    let b = Budget::new(&server);
    let before = server.plan_snapshot(&b.plan);
    let named = |extra: Value| merged(&with(&b.spend(-1), "payee_name", Some(json!("Brand New"))), &extra);
    for body in [
        named(json!({"date": "2026-02-30"})),
        named(json!({"category_id": "nope"})),
        named(json!({"category_id": b.visa_payment})),
        named(json!({"flag_color": "pink"})),
        named(json!({"memo": "x".repeat(501)})),
        named(json!({"category_id": null})),
        named(json!({"subtransactions": [{"amount": -2, "category_id": b.groceries}], "category_id": null})),
        json!({"account_id": b.house.id, "date": "2026-10-04", "amount": 1, "payee_name": "Brand New", "category_id": b.groceries}),
        json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": 1, "payee_name": "Brand New", "payee_id": "nope"}),
        json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": 1, "payee_id": b.house.payee}),
        json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": 1, "payee_id": b.checking.payee}),
        json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": 1, "payee_id": b.savings.payee, "memo": 5}),
    ] {
        let res = server.post_transaction(&b.plan, &body);
        assert_error(&res, 400, &format!("POST {body}"));
        assert_eq!(server.plan_snapshot(&b.plan), before, "after POST {body}");
    }
}

#[test]
fn spec_3_11_failed_transaction_patch_changes_nothing() {
    let server = Server::start();
    let b = Budget::new(&server);
    let plain = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let (out, _) = transfer(&server, &b);
    let out = id_of(&out);
    let before = server.plan_snapshot(&b.plan);
    for (id, body) in [
        (&plain, json!({"payee_name": "Brand New", "memo": "changed", "amount": 1.5})),
        (&plain, json!({"payee_name": "Brand New", "category_id": null})),
        (&plain, json!({"payee_id": b.savings.payee, "category_id": b.groceries})),
        (&plain, json!({"payee_id": b.house.payee, "memo": "changed", "flag_color": "nope"})),
        (&plain, json!({"subtransactions": [{"amount": -1, "category_id": b.groceries}], "category_id": b.groceries})),
        (&out, json!({"payee_name": "Brand New"})),
        (&out, json!({"payee_id": b.house.payee})),
        (&out, json!({"amount": 1000, "date": "2026-02-30"})),
        (&out, json!({"date": "2026-12-01", "category_id": b.groceries})),
        (&out, json!({"memo": "changed", "subtransactions": [{"amount": -500, "category_id": b.groceries}]})),
    ] {
        let res = server.patch_transaction(&b.plan, id, &body);
        assert_error(&res, 400, &format!("PATCH {body}"));
        assert_eq!(server.plan_snapshot(&b.plan), before, "after PATCH {body}");
    }
}

#[test]
fn spec_3_11_concurrent_payee_names_create_one_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = with(&b.spend(-1), "payee_name", Some(json!("Corner Shop")));
    let txns: Vec<Value> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let (server, b, body) = (&server, &b, &body);
                s.spawn(move || server.create_transaction(&b.plan, body))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let shops: Vec<Value> = server.payees(&b.plan).into_iter().filter(|p| p["name"] == "Corner Shop").collect();
    assert_eq!(shops.len(), 1, "{shops:?}");
    for t in &txns {
        assert_eq!(t["payee_id"], shops[0]["id"]);
    }
    assert_eq!(server.transactions(&b.plan).len(), 16);
}

#[test]
fn spec_3_11_concurrent_deletes_of_transfer_halves_succeed_once() {
    let server = Server::start();
    let b = Budget::new(&server);
    for _ in 0..4 {
        let (out, inn) = transfer(&server, &b);
        let ids = [id_of(&out), id_of(&inn)];
        let statuses: Vec<u16> = std::thread::scope(|s| {
            let handles: Vec<_> = ids
                .iter()
                .map(|id| {
                    let (server, b) = (&server, &b);
                    s.spawn(move || server.delete_transaction(&b.plan, id).status)
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut sorted = statuses.clone();
        sorted.sort();
        assert_eq!(sorted, vec![204, 404], "statuses {statuses:?}");
        assert!(server.transactions(&b.plan).is_empty());
    }
}

#[test]
fn spec_3_11_concurrent_transfers_all_land() {
    let server = Server::start();
    let b = Budget::new(&server);
    let outs: Vec<Value> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..12)
            .map(|i| {
                let (server, b) = (&server, &b);
                s.spawn(move || {
                    let (from, to) = if i % 2 == 0 { (&b.checking, &b.savings) } else { (&b.savings, &b.checking) };
                    server.create_transaction(
                        &b.plan,
                        &json!({"account_id": from.id, "date": "2026-10-05", "amount": -100, "payee_id": to.payee}),
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(server.transactions(&b.plan).len(), 24);
    for out in &outs {
        let other = server.transaction(&b.plan, &str_field(out, "transfer_transaction_id"));
        assert_eq!(other["transfer_transaction_id"], out["id"]);
    }
    assert_eq!(server.balances(&b.plan, &b.checking.id), (0, 0, 0));
}

// ---- 3.12 ----

#[test]
fn spec_3_12_transaction_lists_are_arrays() {
    let server = Server::start();
    let b = Budget::new(&server);
    for query in ["", "?account_id=nope", "?approved=false"] {
        let body = server.get_json(&b.path(&format!("/transactions{query}")));
        assert_eq!(body, json!([]), "GET transactions{query}");
    }
}

// ---- 4.4 ----

#[test]
fn spec_4_4_deleting_a_plan_deletes_its_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let other = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let kept = server.create_transaction(&other.plan, &other.spend(-2));
    assert_status(&server.delete(&format!("/plans/{}", b.plan)), 204, "DELETE plan");
    assert_error(&server.get(&b.path("/transactions")), 404, "GET deleted plan's transactions");
    assert_error(&server.get_transaction(&b.plan, &id), 404, "GET deleted plan's transaction");
    assert_eq!(ids_of(&server.transactions(&other.plan)), vec![id_of(&kept)]);
}

// ---- 4.5 ----

#[test]
fn spec_4_5_another_plans_transaction_is_not_found() {
    let server = Server::start();
    let b = Budget::new(&server);
    let other = Budget::new(&server);
    let theirs = server.create_transaction(&other.plan, &other.spend(-1));
    let id = id_of(&theirs);
    assert_error(&server.get_transaction(&b.plan, &id), 404, "GET");
    assert_status(&server.head(&b.txn_path(&id)), 404, "HEAD");
    assert_error(&server.patch_transaction(&b.plan, &id, &json!({"memo": "x"})), 404, "PATCH");
    assert_error(&server.delete_transaction(&b.plan, &id), 404, "DELETE");
    assert_eq!(spec_fields(&server.transaction(&other.plan, &id)), spec_fields(&theirs));
}

#[test]
fn spec_4_5_body_fields_from_another_plan_return_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let other = Budget::new(&server);
    let their_payee = id_of(&server.create_payee(&other.plan, "Shop"));
    let mine = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let before = server.plan_snapshot(&b.plan);
    for (what, body) in [
        ("account_id", with(&b.spend(-1), "account_id", Some(json!(other.checking.id)))),
        ("payee_id", with(&b.spend(-1), "payee_id", Some(json!(their_payee)))),
        ("transfer payee", json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": -1, "payee_id": other.savings.payee})),
        ("category_id", with(&b.spend(-1), "category_id", Some(json!(other.groceries)))),
        ("payment category", with(&b.spend(-1), "category_id", Some(json!(other.visa_payment)))),
        ("sub category_id", json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": -1,
                                   "subtransactions": [{"amount": -1, "category_id": other.groceries}]})),
    ] {
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with another plan's {what}"));
    }
    for (what, body) in [
        ("account_id", json!({"account_id": other.checking.id})),
        ("payee_id", json!({"payee_id": their_payee})),
        ("transfer payee", json!({"payee_id": other.savings.payee})),
        ("category_id", json!({"category_id": other.groceries})),
        ("sub category_id", json!({"subtransactions": [{"amount": -1, "category_id": other.groceries}]})),
    ] {
        let res = server.patch_transaction(&b.plan, &mine, &body);
        assert_error(&res, 400, &format!("PATCH with another plan's {what}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}
