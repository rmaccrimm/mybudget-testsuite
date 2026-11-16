//! SPEC section 11, months and category month entries (11.1 to 11.6), for plans without
//! credit accounts, plus the section 2 and 3 conventions on the months endpoints.

use mybudget_testsuite::{
    CashBudget, MONTH_ENTRY_FIELDS, MONTH_FIELDS, Server, Value, assert_error, assert_fields, assert_matches,
    assert_status, entry, entry_path, id_of, json, month_path,
};

/// The month's `categories`, which must be an array.
fn categories_of(month: &Value) -> Vec<Value> {
    month["categories"].as_array().unwrap_or_else(|| panic!("`categories` isn't an array: {month}")).clone()
}

/// The `category_id`s of the month's `categories`, in order.
fn category_ids(month: &Value) -> Vec<String> {
    categories_of(month)
        .iter()
        .map(|e| e["category_id"].as_str().unwrap_or_else(|| panic!("entry without a string category_id: {e}")).to_owned())
        .collect()
}

/// The entry for `category` in the month's `categories`.
fn entry_in(month: &Value, category: &str) -> Value {
    let found: Vec<Value> = categories_of(month).into_iter().filter(|e| e["category_id"] == category).collect();
    assert_eq!(found.len(), 1, "month lists {category} {} times: {month}", found.len());
    found[0].clone()
}

/// A month in which nothing happens, with the given entries.
fn empty_month(month: &str, categories: Vec<Value>) -> Value {
    json!({"month": month, "income": 0, "cash_advances": 0, "assigned": 0, "activity": 0,
           "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
           "assigned_in_future": 0, "ready_to_assign": 0, "categories": categories})
}

// ---- 2.2 ----

#[test]
fn spec_2_2_undefined_month_paths_return_404() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let g = &b.groceries;
    for rest in [
        "/months".to_owned(),
        "/month/2026-10".to_owned(),
        "/Months/2026-10".to_owned(),
        "/months/2026-10/".to_owned(),
        "/months/2026-10/categories".to_owned(),
        "/months/2026-10/categories/".to_owned(),
        format!("/months/2026-10/categories/{g}/"),
        format!("/months/2026-10/categories/{g}/x"),
        format!("/months/2026-10/category/{g}"),
        format!("/months/2026-10/{g}"),
        format!("/categories/{g}/months/2026-10"),
    ] {
        let path = format!("/plans/{}{rest}", b.plan);
        assert_error(&server.get(&path), 404, &format!("GET {path}"));
    }
}

// ---- 3.1 ----

#[test]
fn spec_3_1_month_responses_have_json_content_type() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let p = &b.plan;
    let g = &b.groceries;
    let responses = [
        ("GET month", 200, server.get(&month_path(p, "2026-10"))),
        ("GET entry", 200, server.get(&entry_path(p, "2026-10", g))),
        ("PATCH entry", 200, server.patch(&entry_path(p, "2026-10", g), &json!({"assigned": 1000}))),
        ("GET missing entry", 404, server.get(&entry_path(p, "2026-10", "no-such-category"))),
        ("GET bad month", 400, server.get(&month_path(p, "2026-13"))),
        ("PATCH bad assigned", 400, server.patch(&entry_path(p, "2026-10", g), &json!({"assigned": "1000"}))),
        ("DELETE month", 405, server.delete(&month_path(p, "2026-10"))),
    ];
    for (what, status, res) in responses {
        assert_status(&res, status, what);
        assert!(res.is_json_content_type(), "{what}: Content-Type {:?}", res.content_type);
        res.json();
    }
}

#[test]
fn spec_3_1_month_entry_patch_body_must_be_a_json_object() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    let path = entry_path(&b.plan, "2026-10", &b.groceries);
    assert_error(&server.request_bytes("PATCH", &path, None), 400, "PATCH with no body");
    for body in ["", "{", "[]", "null", "5", "\"assigned\"", r#"[{"assigned": 5}]"#, r#"{"assigned": 5,}"#, r#"{"assigned": 5"#] {
        assert_error(&server.request_raw("PATCH", &path, body), 400, &format!("PATCH {body:?}"));
    }
    assert_matches(&b.entry(&server, "2026-10", &b.groceries), &entry(&b.groceries, 400000, 0, 400000, 0), "after rejected bodies");
}

// ---- 3.4 ----

#[test]
fn spec_3_4_undefined_methods_on_months_return_405() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    let month = month_path(&b.plan, "2026-10");
    let entry_url = entry_path(&b.plan, "2026-10", &b.groceries);
    let cases = [
        ("POST", month.clone()),
        ("PUT", month.clone()),
        ("PATCH", month.clone()),
        ("DELETE", month.clone()),
        ("OPTIONS", month.clone()),
        ("POST", entry_url.clone()),
        ("PUT", entry_url.clone()),
        ("DELETE", entry_url.clone()),
        ("OPTIONS", entry_url.clone()),
    ];
    for (method, path) in &cases {
        assert_error(&server.request_raw(method, path, r#"{"assigned": 1000}"#), 405, &format!("{method} {path}"));
    }
    assert_matches(&b.entry(&server, "2026-10", &b.groceries), &entry(&b.groceries, 400000, 0, 400000, 0), "after 405s");
}

#[test]
fn spec_3_4_method_not_allowed_wins_on_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    for (method, path) in [
        ("DELETE", month_path("no-such-plan", "2026-10")),
        ("PATCH", month_path(&b.plan, "2026-13")),
        ("POST", month_path("no-such-plan", "bad")),
        ("DELETE", entry_path(&b.plan, "2026-10", "no-such-category")),
        ("POST", entry_path("no-such-plan", "2026-13", "no-such-category")),
        ("PUT", entry_path(&b.plan, "2026-10", "ready_to_assign")),
    ] {
        assert_error(&server.request_raw(method, &path, "not json"), 405, &format!("{method} {path}"));
    }
}

#[test]
fn spec_3_4_not_found_wins_over_bad_month() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let g = &b.groceries;
    assert_error(&server.get(&month_path("no-such-plan", "2026-13")), 404, "GET month, no plan, bad month");
    assert_error(&server.get(&entry_path("no-such-plan", "2026-13", g)), 404, "GET entry, no plan, bad month");
    assert_error(&server.get(&entry_path(&b.plan, "2026-13", "no-such-category")), 404, "GET entry, no category, bad month");
    let res = server.request_raw("PATCH", &entry_path(&b.plan, "2026-13", "no-such-category"), r#"{"assigned": "x"}"#);
    assert_error(&res, 404, "PATCH entry, no category, bad month, bad body");
    let res = server.request_raw("PATCH", &entry_path(&b.plan, "bad", "ready_to_assign"), "not json");
    assert_error(&res, 404, "PATCH ready_to_assign, bad month, bad body");
    // With everything named existing, the bad month or body is a 400.
    assert_error(&server.patch(&entry_path(&b.plan, "2026-13", g), &json!({"assigned": 5})), 400, "bad month");
    assert_error(&server.request_raw("PATCH", &entry_path(&b.plan, "2026-10", g), "not json"), 400, "bad body");
}

#[test]
fn spec_3_4_head_on_months_returns_get_without_body() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    for path in [month_path(&b.plan, "2026-10"), entry_path(&b.plan, "2026-10", &b.groceries)] {
        let res = server.head(&path);
        assert_status(&res, 200, &format!("HEAD {path}"));
        assert!(res.body.is_empty(), "HEAD {path} returned a body: {:?}", res.text());
    }
    for (path, status) in [
        (entry_path(&b.plan, "2026-10", "no-such-category"), 404),
        (month_path("no-such-plan", "2026-10"), 404),
        (month_path(&b.plan, "2026-13"), 400),
    ] {
        let res = server.head(&path);
        assert_status(&res, status, &format!("HEAD {path}"));
        assert!(res.body.is_empty(), "HEAD {path} returned a body: {:?}", res.text());
    }
}

// ---- 3.6 ----

#[test]
fn spec_3_6_month_entry_patch_ignores_undefined_fields() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let res = server.patch(
        &entry_path(&b.plan, "2026-10", &b.groceries),
        &json!({"assigned": 400000, "activity": 5, "available": 7, "category_id": b.household, "month": "2026-11"}),
    );
    assert_status(&res, 200, "PATCH with extra fields");
    assert_matches(&res.json(), &entry(&b.groceries, 400000, 0, 400000, 0), "PATCH response");
    assert_matches(&b.entry(&server, "2026-10", &b.household), &entry(&b.household, 0, 0, 0, 0), "household");
    assert_matches(&b.entry(&server, "2026-11", &b.groceries), &entry(&b.groceries, 0, 0, 400000, 0), "November");
}

#[test]
fn spec_3_6_month_queries_ignore_undefined_parameters() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    let plain = b.month(&server, "2026-10");
    let queried = server.get_json(&format!("{}?month=2026-11&category_id={}", month_path(&b.plan, "2026-10"), b.household));
    assert_eq!(queried, plain);
    let e = server.get_json(&format!("{}?month=2026-11", entry_path(&b.plan, "2026-10", &b.groceries)));
    assert_matches(&e, &entry(&b.groceries, 400000, 0, 400000, 0), "entry with a query");
}

// ---- 3.8 ----

const BAD_MONTHS: [&str; 18] = [
    "2026-13", "2026-00", "2026-1", "2026-010", "26-10", "202610", "2026-10-01", "1899-12", "3000-01", "0000-01",
    "9999-12", "2026_10", "2026-1a", "abcd-ef", "2026-10x", "2026--10", "-2026-10", "+2026-10",
];

#[test]
fn spec_3_8_month_in_path_must_be_valid() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    for month in BAD_MONTHS {
        assert_error(&server.get(&month_path(&b.plan, month)), 400, &format!("GET month {month}"));
        assert_error(&server.get(&entry_path(&b.plan, month, &b.groceries)), 400, &format!("GET entry {month}"));
        let res = server.patch(&entry_path(&b.plan, month, &b.groceries), &json!({"assigned": 5}));
        assert_error(&res, 400, &format!("PATCH entry {month}"));
    }
    assert_matches(&b.entry(&server, "2026-10", &b.groceries), &entry(&b.groceries, 400000, 0, 400000, 0), "October");
}

#[test]
fn spec_3_8_months_at_the_ends_of_the_range_are_accepted() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    for month in ["1900-01", "1900-12", "2999-01", "2999-12"] {
        assert_matches(&b.month(&server, month), &json!({"month": month}), &format!("GET {month}"));
        let e = b.assign(&server, month, &b.household, 1000);
        assert_eq!(e["assigned"], 1000, "PATCH {month}: {e}");
        assert_eq!(b.entry(&server, month, &b.household)["assigned"], 1000, "GET entry {month}");
    }
}

// ---- 3.10 ----

#[test]
fn spec_3_10_month_entry_patch_without_fields_changes_nothing() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    for body in [json!({}), json!({"budgeted": 5, "available": 9})] {
        let res = server.patch(&entry_path(&b.plan, "2026-10", &b.groceries), &body);
        assert_status(&res, 200, &format!("PATCH {body}"));
        assert_matches(&res.json(), &entry(&b.groceries, 400000, 0, 400000, 0), &format!("PATCH {body}"));
    }
    assert_matches(&b.entry(&server, "2026-10", &b.groceries), &entry(&b.groceries, 400000, 0, 400000, 0), "after");
}

// ---- 3.11 ----

#[test]
fn spec_3_11_rejected_assignments_change_no_month() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 400000);
    let before = [b.month(&server, "2026-09"), b.month(&server, "2026-10"), b.month(&server, "2026-11")];
    for body in [r#"{"assigned": 1.5}"#, r#"{"assigned": null}"#, r#"{"assigned": 99999999999999}"#] {
        let res = server.request_raw("PATCH", &entry_path(&b.plan, "2026-10", &b.groceries), body);
        assert_error(&res, 400, body);
    }
    let after = [b.month(&server, "2026-09"), b.month(&server, "2026-10"), b.month(&server, "2026-11")];
    assert_eq!(after, before);
}

// ---- 4.5 ----

#[test]
fn spec_4_5_months_of_other_plans_categories_return_404() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let other = CashBudget::new(&server);
    other.assign(&server, "2026-10", &other.groceries, 400000);
    let foreign = &other.groceries;
    assert_error(&server.get(&entry_path(&b.plan, "2026-10", foreign)), 404, "GET another plan's category");
    let res = server.patch(&entry_path(&b.plan, "2026-10", foreign), &json!({"assigned": 5}));
    assert_error(&res, 404, "PATCH another plan's category");
    assert_eq!(other.entry(&server, "2026-10", foreign)["assigned"], 400000);
    assert!(!category_ids(&b.month(&server, "2026-10")).contains(foreign));
}

#[test]
fn spec_4_5_months_of_a_deleted_plan_return_404() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    assert_status(&server.delete(&format!("/plans/{}", b.plan)), 204, "DELETE plan");
    assert_error(&server.get(&month_path(&b.plan, "2026-10")), 404, "GET month");
    assert_error(&server.get(&entry_path(&b.plan, "2026-10", &b.groceries)), 404, "GET entry");
    let res = server.patch(&entry_path(&b.plan, "2026-10", &b.groceries), &json!({"assigned": 5}));
    assert_error(&res, 404, "PATCH entry");
}

// ---- 7.8 ----

#[test]
fn spec_7_8_hidden_categories_take_part_in_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let hidden_group = id_of(&server.create_group(&b.plan, "Old"));
    let old = id_of(&server.create_category(&b.plan, &hidden_group, "Old Hobby"));
    server.patch_ok(&format!("/plans/{}/category_groups/{hidden_group}", b.plan), &json!({"hidden": true}));
    server.patch_ok(&format!("/plans/{}/categories/{}", b.plan, b.household), &json!({"hidden": true}));
    b.txn(&server, &b.checking, "2026-10-01", 1000000, "ready_to_assign");
    b.txn(&server, &b.checking, "2026-10-05", -30000, &old);
    b.txn(&server, &b.checking, "2026-10-06", -10000, &b.household);
    assert_matches(&b.assign(&server, "2026-10", &old, 50000), &entry(&old, 50000, -30000, 20000, 0), "PATCH hidden");
    b.assign(&server, "2026-10", &b.household, 5000);
    let month = b.month(&server, "2026-10");
    assert_eq!(category_ids(&month), vec![b.groceries.clone(), b.household.clone(), old.clone()]);
    assert_matches(
        &month,
        &json!({"income": 1000000, "assigned": 55000, "activity": -40000, "ready_to_assign": 945000,
                "categories": [entry(&b.groceries, 0, 0, 0, 0), entry(&b.household, 5000, -10000, -5000, 5000),
                               entry(&old, 50000, -30000, 20000, 0)]}),
        "October",
    );
    assert_matches(&b.entry(&server, "2026-11", &old), &entry(&old, 0, 0, 20000, 0), "November");
    assert_eq!(b.month(&server, "2026-11")["cash_overspending_last_month"], 5000);
}

// ---- 11.1 ----

#[test]
fn spec_11_1_tracking_accounts_never_affect_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let brokerage = server.create_account(
        &b.plan,
        &json!({"name": "Brokerage", "type": "otherAsset", "starting_balance": 5000000, "starting_date": "2026-10-01"}),
    );
    server.create_account(
        &b.plan,
        &json!({"name": "Car Loan", "type": "otherLiability", "starting_balance": -900000, "starting_date": "2026-10-02"}),
    );
    server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-05", "amount": 100000}));
    server.create_transaction(
        &b.plan,
        &json!({"account_id": id_of(&brokerage), "date": "2026-10-06", "amount": -250000, "payee_id": b.mortgage.payee}),
    );
    let categories = vec![entry(&b.groceries, 0, 0, 0, 0), entry(&b.household, 0, 0, 0, 0)];
    for month in ["2026-09", "2026-10", "2026-11"] {
        assert_matches(&b.month(&server, month), &empty_month(month, categories.clone()), month);
    }
}

#[test]
fn spec_11_1_each_subtransaction_is_a_line() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -90000,
                "subtransactions": [{"amount": -60000, "category_id": b.groceries},
                                    {"amount": -30000, "category_id": b.household}]}),
    );
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.wallet.id, "date": "2026-10-07", "amount": 70000,
                "subtransactions": [{"amount": 100000, "category_id": "ready_to_assign"},
                                    {"amount": -25000, "category_id": b.groceries},
                                    {"amount": -5000, "category_id": b.groceries}]}),
    );
    assert_matches(
        &b.month(&server, "2026-10"),
        &json!({"income": 100000, "activity": -120000, "assigned": 0,
                "categories": [entry(&b.groceries, 0, -90000, -90000, 90000),
                               entry(&b.household, 0, -30000, -30000, 30000)]}),
        "October",
    );
}

#[test]
fn spec_11_1_on_budget_half_of_a_tracking_transfer_is_a_line() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    // Selling part of the house into Checking: the request is for the tracking half, and
    // its category goes to Checking's half (9.3).
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.house.id, "date": "2026-10-03", "amount": -500000, "payee_id": b.checking.payee,
                "category_id": "ready_to_assign"}),
    );
    // Paying the mortgage from Savings, out of Household.
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.savings.id, "date": "2026-10-04", "amount": -200000, "payee_id": b.mortgage.payee,
                "category_id": b.household}),
    );
    assert_matches(
        &b.month(&server, "2026-10"),
        &json!({"income": 500000, "activity": -200000,
                "categories": [entry(&b.groceries, 0, 0, 0, 0), entry(&b.household, 0, -200000, -200000, 200000)]}),
        "October",
    );
}

#[test]
fn spec_11_1_transfers_between_cash_accounts_change_no_category() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 100000);
    for (from, to, amount) in [(&b.checking, &b.savings, -300000), (&b.wallet, &b.checking, -40000), (&b.savings, &b.wallet, 25000)] {
        server.create_transaction(
            &b.plan,
            &json!({"account_id": from.id, "date": "2026-10-10", "amount": amount, "payee_id": to.payee}),
        );
    }
    assert_matches(
        &b.month(&server, "2026-10"),
        &json!({"income": 1000000, "assigned": 100000, "activity": 0, "ready_to_assign": 900000,
                "categories": [entry(&b.groceries, 100000, 0, 100000, 0), entry(&b.household, 0, 0, 0, 0)]}),
        "October",
    );
}

#[test]
fn spec_11_1_a_line_belongs_to_the_month_of_its_date() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    for (date, amount) in [
        ("2026-09-30", -1000),
        ("2026-10-01", -2000),
        ("2026-10-31", -3000),
        ("2026-11-01", -4000),
        ("2026-12-31", -5000),
        ("2027-01-01", -6000),
        ("2028-02-29", -7000),
        ("2028-03-01", -8000),
    ] {
        b.txn(&server, &b.checking, date, amount, &b.groceries);
        b.txn(&server, &b.savings, date, -amount * 10, "ready_to_assign");
    }
    for (month, activity) in [
        ("2026-08", 0),
        ("2026-09", -1000),
        ("2026-10", -5000),
        ("2026-11", -4000),
        ("2026-12", -5000),
        ("2027-01", -6000),
        ("2027-02", 0),
        ("2028-02", -7000),
        ("2028-03", -8000),
    ] {
        let m = b.month(&server, month);
        assert_matches(&m, &json!({"month": month, "income": -activity * 10, "activity": activity}), month);
        assert_eq!(entry_in(&m, &b.groceries)["activity"], activity, "{month}");
    }
}

#[test]
fn spec_11_1_lines_in_closed_accounts_still_count() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let jar = server.create_account(
        &b.plan,
        &json!({"name": "Jar", "type": "cash", "starting_balance": 50000, "starting_date": "2026-10-01"}),
    );
    let jar_id = id_of(&jar);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": jar_id, "date": "2026-10-02", "amount": -50000, "category_id": b.groceries}),
    );
    server.patch_ok(&format!("/plans/{}/accounts/{jar_id}", b.plan), &json!({"closed": true}));
    server.create_transaction(
        &b.plan,
        &json!({"account_id": jar_id, "date": "2026-11-03", "amount": 20000, "category_id": "ready_to_assign"}),
    );
    assert_matches(&b.month(&server, "2026-10"), &json!({"income": 50000, "activity": -50000}), "October");
    assert_matches(&b.month(&server, "2026-11"), &json!({"income": 20000, "activity": 0}), "November");
}

#[test]
fn spec_11_1_changing_transactions_changes_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let id = id_of(&b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries));
    let check = |what: &str, oct: [i64; 3], nov: [i64; 3]| {
        for (month, [income, groceries, household]) in [("2026-10", oct), ("2026-11", nov)] {
            let m = b.month(&server, month);
            assert_matches(&m, &json!({"income": income, "activity": groceries + household}), &format!("{what}: {month}"));
            assert_eq!(entry_in(&m, &b.groceries)["activity"], groceries, "{what}: {month} groceries");
            assert_eq!(entry_in(&m, &b.household)["activity"], household, "{what}: {month} household");
        }
    };
    check("created", [0, -120000, 0], [0, 0, 0]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"date": "2026-11-02"}));
    check("moved to November", [0, 0, 0], [0, -120000, 0]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"category_id": b.household}));
    check("recategorized", [0, 0, 0], [0, 0, -120000]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"amount": -50000}));
    check("new amount", [0, 0, 0], [0, 0, -50000]);
    server.patch_transaction_ok(
        &b.plan,
        &id,
        &json!({"subtransactions": [{"amount": -20000, "category_id": b.groceries},
                                    {"amount": -30000, "category_id": "ready_to_assign"}]}),
    );
    check("split", [0, 0, 0], [-30000, -20000, 0]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"subtransactions": [], "category_id": "ready_to_assign"}));
    check("ready to assign", [0, 0, 0], [-50000, 0, 0]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": b.house.payee}));
    check("transfer to a tracking account", [0, 0, 0], [-50000, 0, 0]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": b.savings.payee}));
    check("transfer to savings", [0, 0, 0], [0, 0, 0]);
    server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": null, "category_id": b.groceries, "date": "2026-10-09"}));
    check("ordinary again", [0, -50000, 0], [0, 0, 0]);
    assert_status(&server.delete_transaction(&b.plan, &id), 204, "DELETE");
    check("deleted", [0, 0, 0], [0, 0, 0]);
}

// ---- 11.2 ----

#[test]
fn spec_11_2_assign_matches_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    let res = server.patch(&entry_path(&plan, "2026-10", &groceries), &json!({"assigned": 400000}));
    assert_status(&res, 200, "11.2's PATCH");
    let body = res.json();
    assert_fields(&body, &MONTH_ENTRY_FIELDS, "11.2's response");
    assert_matches(
        &body,
        &json!({"category_id": groceries, "assigned": 400000, "activity": 0, "available": 400000,
                "cash_overspending": 0, "credit_overspending": 0}),
        "11.2's example",
    );
}

#[test]
fn spec_11_2_assigned_is_per_month_and_category() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    b.assign(&server, "2026-11", &b.household, 100000);
    for (month, category, assigned) in [
        ("2026-09", &b.groceries, 0),
        ("2026-09", &b.household, 0),
        ("2026-10", &b.groceries, 400000),
        ("2026-10", &b.household, 0),
        ("2026-11", &b.groceries, 0),
        ("2026-11", &b.household, 100000),
        ("2026-12", &b.groceries, 0),
        ("2026-12", &b.household, 0),
        ("2027-10", &b.groceries, 0),
    ] {
        assert_eq!(b.entry(&server, month, category)["assigned"], assigned, "{month} {category}");
        assert_eq!(entry_in(&b.month(&server, month), category)["assigned"], assigned, "{month} {category} in month");
    }
}

#[test]
fn spec_11_2_patch_sets_assigned() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    for amount in [400000, 250000, 250000, 0, 7] {
        assert_matches(&b.assign(&server, "2026-10", &b.groceries, amount), &entry(&b.groceries, amount, 0, amount, 0), "PATCH");
        assert_eq!(b.entry(&server, "2026-10", &b.groceries)["assigned"], amount, "GET after setting {amount}");
        assert_eq!(b.month(&server, "2026-10")["assigned"], amount, "month after setting {amount}");
    }
}

#[test]
fn spec_11_2_assigned_may_be_negative() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 300000);
    let e = b.assign(&server, "2026-11", &b.groceries, -100000);
    assert_matches(&e, &entry(&b.groceries, -100000, 0, 200000, 0), "PATCH -100000");
    assert_eq!(b.entry(&server, "2026-11", &b.groceries)["assigned"], -100000);
    assert_eq!(b.month(&server, "2026-11")["assigned"], -100000);
}

#[test]
fn spec_11_2_assigned_must_be_an_amount() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    let path = entry_path(&b.plan, "2026-10", &b.groceries);
    for value in [
        "null", "1.5", "1.0", "400000.0", "1e3", "1E3", "-0.0", "1000000000001", "-1000000000001",
        "99999999999999999999", "\"100\"", "\"\"", "true", "false", "[1]", "{}", "{\"amount\": 1}",
    ] {
        let body = format!(r#"{{"assigned": {value}}}"#);
        assert_error(&server.request_raw("PATCH", &path, &body), 400, &format!("PATCH {body}"));
    }
    assert_matches(&b.entry(&server, "2026-10", &b.groceries), &entry(&b.groceries, 400000, 0, 400000, 0), "after");
}

#[test]
fn spec_11_2_assigned_takes_the_whole_amount_range() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let max = 1_000_000_000_000_i64;
    assert_eq!(b.assign(&server, "2026-10", &b.groceries, max)["assigned"], max);
    assert_eq!(b.assign(&server, "2026-10", &b.household, -max)["assigned"], -max);
    assert_matches(&b.month(&server, "2026-10"), &json!({"assigned": 0}), "max and -max");
    assert_eq!(b.entry(&server, "2026-10", &b.groceries)["available"], max);
    let res = server.request_raw("PATCH", &entry_path(&b.plan, "2026-10", &b.groceries), r#"{"assigned": -0}"#);
    assert_status(&res, 200, "PATCH -0");
    assert_eq!(res.json()["assigned"].as_i64(), Some(0), "-0 means 0: {}", res.text());
    assert_eq!(b.entry(&server, "2026-10", &b.groceries)["assigned"].as_i64(), Some(0));
}

#[test]
fn spec_11_2_response_is_the_whole_month_entry() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-09-01", 1000000, "ready_to_assign");
    b.assign(&server, "2026-09", &b.groceries, 50000);
    b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries);
    let e = b.assign(&server, "2026-10", &b.groceries, 400000);
    assert_fields(&e, &MONTH_ENTRY_FIELDS, "PATCH response");
    assert_matches(&e, &entry(&b.groceries, 400000, -120000, 330000, 0), "PATCH 400000");
    assert_matches(&b.assign(&server, "2026-10", &b.groceries, 50000), &entry(&b.groceries, 50000, -120000, -20000, 20000), "PATCH 50000");
}

#[test]
fn spec_11_2_ready_to_assign_is_not_a_category() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, "ready_to_assign");
    let res = server.patch(&entry_path(&b.plan, "2026-10", "ready_to_assign"), &json!({"assigned": 100000}));
    assert_error(&res, 404, "PATCH ready_to_assign");
    assert_matches(&b.month(&server, "2026-10"), &json!({"assigned": 0, "ready_to_assign": 1000000}), "October");
    assert!(!category_ids(&b.month(&server, "2026-10")).contains(&"ready_to_assign".to_owned()));
}

#[test]
fn spec_11_2_unknown_categories_return_404() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    let before = b.month(&server, "2026-10");
    for id in ["no-such-category".to_owned(), b.group.clone(), b.checking.id.clone(), b.checking.payee.clone(), b.plan.clone()] {
        let res = server.patch(&entry_path(&b.plan, "2026-10", &id), &json!({"assigned": 5}));
        assert_error(&res, 404, &format!("PATCH {id}"));
    }
    let res = server.patch(&entry_path("no-such-plan", "2026-10", &b.groceries), &json!({"assigned": 5}));
    assert_error(&res, 404, "PATCH in a missing plan");
    assert_eq!(b.month(&server, "2026-10"), before);
}

// ---- 11.3 ----

#[test]
fn spec_11_3_get_entry_matches_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    server.assign(&plan, "2026-10", &groceries, 400000);
    let res = server.get(&entry_path(&plan, "2026-10", &groceries));
    assert_status(&res, 200, "11.3's GET");
    let body = res.json();
    assert_fields(&body, &MONTH_ENTRY_FIELDS, "11.3's response");
    assert_matches(
        &body,
        &json!({"category_id": groceries, "assigned": 400000, "activity": 0, "available": 400000,
                "cash_overspending": 0, "credit_overspending": 0}),
        "11.3's example",
    );
}

#[test]
fn spec_11_3_entry_of_an_untouched_category_is_zero() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    for month in ["1900-01", "2026-10", "2999-12"] {
        assert_matches(&b.entry(&server, month, &b.groceries), &entry(&b.groceries, 0, 0, 0, 0), month);
    }
}

#[test]
fn spec_11_3_activity_sums_the_months_lines_with_the_category() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries);
    b.txn(&server, &b.wallet, "2026-10-05", -30000, &b.groceries);
    b.txn(&server, &b.savings, "2026-10-06", 20000, &b.groceries);
    b.txn(&server, &b.checking, "2026-10-07", -45000, &b.household);
    b.txn(&server, &b.checking, "2026-09-30", -1000, &b.groceries);
    b.txn(&server, &b.checking, "2026-11-01", -2000, &b.groceries);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-08", "amount": -15000,
                "subtransactions": [{"amount": -10000, "category_id": b.groceries},
                                    {"amount": -5000, "category_id": b.household}]}),
    );
    assert_eq!(b.entry(&server, "2026-10", &b.groceries)["activity"], -140000);
    assert_eq!(b.entry(&server, "2026-10", &b.household)["activity"], -50000);
    assert_eq!(b.entry(&server, "2026-09", &b.groceries)["activity"], -1000);
    assert_eq!(b.entry(&server, "2026-11", &b.groceries)["activity"], -2000);
    assert_eq!(b.entry(&server, "2026-12", &b.groceries)["activity"], 0);
}

#[test]
fn spec_11_3_entry_is_the_same_as_in_the_month() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 100000);
    b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries);
    b.assign(&server, "2026-10", &b.household, 70000);
    b.txn(&server, &b.wallet, "2026-11-04", -30000, &b.household);
    for month in ["2026-09", "2026-10", "2026-11", "2026-12"] {
        let m = b.month(&server, month);
        for category in [&b.groceries, &b.household] {
            let e = b.entry(&server, month, category);
            assert_fields(&e, &MONTH_ENTRY_FIELDS, "entry");
            let listed = entry_in(&m, category);
            for field in MONTH_ENTRY_FIELDS {
                assert_eq!(e[field], listed[field], "{month} {category} `{field}`: GET {e}, month {listed}");
            }
        }
    }
}

#[test]
fn spec_11_3_ready_to_assign_and_unknown_categories_return_404() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    for id in ["ready_to_assign".to_owned(), "no-such-category".to_owned(), b.group.clone(), b.wallet.id.clone()] {
        assert_error(&server.get(&entry_path(&b.plan, "2026-10", &id)), 404, &format!("GET {id}"));
    }
    assert_error(&server.get(&entry_path("no-such-plan", "2026-10", &b.groceries)), 404, "GET in a missing plan");
}

// ---- 11.4 ----

#[test]
fn spec_11_4_positive_available_rolls_over() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 400000);
    b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries);
    b.assign(&server, "2026-12", &b.groceries, 100000);
    b.txn(&server, &b.checking, "2026-12-10", -50000, &b.groceries);
    let g = &b.groceries;
    for (month, expected) in [
        ("2026-09", entry(g, 0, 0, 0, 0)),
        ("2026-10", entry(g, 400000, -120000, 280000, 0)),
        ("2026-11", entry(g, 0, 0, 280000, 0)),
        ("2026-12", entry(g, 100000, -50000, 330000, 0)),
        ("2027-01", entry(g, 0, 0, 330000, 0)),
        ("2030-06", entry(g, 0, 0, 330000, 0)),
    ] {
        assert_matches(&b.entry(&server, month, g), &expected, month);
    }
}

#[test]
fn spec_11_4_overspent_category_starts_next_month_at_zero() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 50000);
    b.txn(&server, &b.checking, "2026-10-11", -80000, &b.groceries);
    b.assign(&server, "2026-11", &b.groceries, 10000);
    b.txn(&server, &b.checking, "2026-12-11", -4000, &b.groceries);
    let g = &b.groceries;
    for (month, expected) in [
        ("2026-10", entry(g, 50000, -80000, -30000, 30000)),
        ("2026-11", entry(g, 10000, 0, 10000, 0)),
        ("2026-12", entry(g, 0, -4000, 6000, 0)),
        ("2027-01", entry(g, 0, 0, 6000, 0)),
    ] {
        assert_matches(&b.entry(&server, month, g), &expected, month);
    }
    // Overspent two months running: each month starts at 0.
    b.txn(&server, &b.checking, "2027-02-01", -9000, &b.household);
    b.txn(&server, &b.checking, "2027-03-01", -1000, &b.household);
    let h = &b.household;
    assert_matches(&b.entry(&server, "2027-02", h), &entry(h, 0, -9000, -9000, 9000), "2027-02");
    assert_matches(&b.entry(&server, "2027-03", h), &entry(h, 0, -1000, -1000, 1000), "2027-03");
    assert_matches(&b.entry(&server, "2027-04", h), &entry(h, 0, 0, 0, 0), "2027-04");
}

#[test]
fn spec_11_4_inflows_and_negative_assignments_change_available() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.txn(&server, &b.checking, "2026-10-12", 50000, &b.groceries);
    b.assign(&server, "2026-11", &b.groceries, 250000);
    b.assign(&server, "2026-12", &b.groceries, -100000);
    b.assign(&server, "2027-01", &b.groceries, -300000);
    let g = &b.groceries;
    for (month, expected) in [
        ("2026-10", entry(g, 0, 50000, 50000, 0)),
        ("2026-11", entry(g, 250000, 0, 300000, 0)),
        ("2026-12", entry(g, -100000, 0, 200000, 0)),
        ("2027-01", entry(g, -300000, 0, -100000, 100000)),
        ("2027-02", entry(g, 0, 0, 0, 0)),
    ] {
        assert_matches(&b.entry(&server, month, g), &expected, month);
    }
}

#[test]
fn spec_11_4_changes_to_earlier_months_reach_later_ones() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let g = &b.groceries;
    b.assign(&server, "2026-10", g, 100000);
    assert_eq!(b.entry(&server, "2026-12", g)["available"], 100000);
    let spend = b.txn(&server, &b.checking, "2026-09-15", -30000, g);
    // September's overspending doesn't reach October.
    assert_matches(&b.entry(&server, "2026-10", g), &entry(g, 100000, 0, 100000, 0), "October");
    b.assign(&server, "2026-09", g, 70000);
    assert_matches(&b.entry(&server, "2026-12", g), &entry(g, 0, 0, 140000, 0), "after assigning in September");
    server.patch_transaction_ok(&b.plan, &id_of(&spend), &json!({"amount": -100000}));
    assert_matches(&b.entry(&server, "2026-12", g), &entry(g, 0, 0, 100000, 0), "after overspending September");
    b.assign(&server, "2026-10", g, 0);
    assert_matches(&b.entry(&server, "2026-12", g), &entry(g, 0, 0, 0, 0), "after unassigning October");
}

#[test]
fn spec_11_4_carryover_spans_the_whole_range() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let g = &b.groceries;
    b.assign(&server, "1900-01", g, 100000);
    b.txn(&server, &b.checking, "1900-01-31", -30000, g);
    b.txn(&server, &b.checking, "1900-01-01", -40000, &b.household);
    assert_matches(&b.entry(&server, "1900-01", g), &entry(g, 100000, -30000, 70000, 0), "1900-01");
    assert_matches(&b.entry(&server, "1900-01", &b.household), &entry(&b.household, 0, -40000, -40000, 40000), "1900-01");
    for month in ["1900-02", "1999-12", "2026-10", "2999-12"] {
        assert_matches(&b.entry(&server, month, g), &entry(g, 0, 0, 70000, 0), month);
        assert_matches(&b.entry(&server, month, &b.household), &entry(&b.household, 0, 0, 0, 0), month);
    }
}

// ---- 11.5 ----

#[test]
fn spec_11_5_cash_overspending_is_the_negative_available() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 50000);
    b.txn(&server, &b.checking, "2026-10-11", -60000, &b.groceries);
    b.txn(&server, &b.wallet, "2026-10-12", -20000, &b.groceries);
    b.assign(&server, "2026-10", &b.household, 30000);
    b.txn(&server, &b.savings, "2026-10-13", -30000, &b.household);
    let m = b.month(&server, "2026-10");
    assert_matches(&entry_in(&m, &b.groceries), &entry(&b.groceries, 50000, -80000, -30000, 30000), "groceries");
    assert_matches(&entry_in(&m, &b.household), &entry(&b.household, 30000, -30000, 0, 0), "household at exactly 0");
    assert_matches(&b.entry(&server, "2026-10", &b.groceries), &entry(&b.groceries, 50000, -80000, -30000, 30000), "GET");
}

#[test]
fn spec_11_5_negative_assigned_alone_overspends() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let h = &b.household;
    assert_matches(&b.assign(&server, "2026-10", h, -40000), &entry(h, -40000, 0, -40000, 40000), "PATCH");
    assert_matches(&b.entry(&server, "2026-10", h), &entry(h, -40000, 0, -40000, 40000), "GET");
    assert_eq!(b.month(&server, "2026-11")["cash_overspending_last_month"], 40000);
}

#[test]
fn spec_11_5_credit_overspending_is_zero_without_credit_accounts() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries);
    b.txn(&server, &b.wallet, "2026-10-05", -20000, &b.household);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.savings.id, "date": "2026-10-06", "amount": -5000, "payee_id": b.mortgage.payee,
                "category_id": b.household}),
    );
    for month in ["2026-10", "2026-11"] {
        for e in categories_of(&b.month(&server, month)) {
            assert_eq!(e["credit_overspending"], 0, "{month}: {e}");
        }
        for category in [&b.groceries, &b.household] {
            assert_eq!(b.entry(&server, month, category)["credit_overspending"], 0, "{month} {category}");
        }
    }
    assert_eq!(b.entry(&server, "2026-10", &b.household)["cash_overspending"], 25000);
}

// ---- 11.6 ----

#[test]
fn spec_11_6_month_has_every_field() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 2000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 100000);
    for month in ["2026-09", "2026-10"] {
        let res = server.get(&month_path(&b.plan, month));
        assert_status(&res, 200, &format!("GET {month}"));
        let body = res.json();
        assert_fields(&body, &MONTH_FIELDS, month);
        assert_eq!(body["month"], month);
        assert_eq!(body["cash_advances"], 0, "{month}: no credit accounts");
        for e in categories_of(&body) {
            assert_fields(&e, &MONTH_ENTRY_FIELDS, &format!("{month} entry"));
        }
    }
}

#[test]
fn spec_11_6_empty_plan_has_empty_months() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for month in ["1900-01", "2026-10", "2999-12"] {
        assert_matches(&server.month(&plan, month), &empty_month(month, vec![]), month);
    }
    server.create_group(&plan, "Empty");
    server.add_account(&plan, "Checking", "checking", 0);
    assert_matches(&server.month(&plan, "2026-10"), &empty_month("2026-10", vec![]), "with an empty group");
}

#[test]
fn spec_11_6_income_is_ready_to_assign_lines_in_cash_accounts() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    let account = |name: &str, ty: &str, balance: i64, date: &str| {
        id_of(&server.create_account(
            &plan,
            &json!({"name": name, "type": ty, "starting_balance": balance, "starting_date": date}),
        ))
    };
    let checking = account("Checking", "checking", 1000000, "2026-10-01");
    account("Savings", "savings", 200000, "2026-10-15");
    let wallet = account("Wallet", "cash", -5000, "2026-10-20");
    account("Old Savings", "savings", 70000, "2026-09-30");
    account("Brokerage", "otherAsset", 9000000, "2026-10-01");
    account("Loan", "otherLiability", -500000, "2026-10-01");
    let txn = |account: &str, date: &str, amount: i64, category: &str| {
        server.create_transaction(
            &plan,
            &json!({"account_id": account, "date": date, "amount": amount, "category_id": category}),
        );
    };
    txn(&checking, "2026-10-15", 1800000, "ready_to_assign");
    txn(&checking, "2026-10-20", -100000, "ready_to_assign");
    txn(&checking, "2026-10-21", 15000, &groceries);
    txn(&checking, "2026-11-15", 1800000, "ready_to_assign");
    server.create_transaction(
        &plan,
        &json!({"account_id": wallet, "date": "2026-10-22", "amount": 60000,
                "subtransactions": [{"amount": 80000, "category_id": "ready_to_assign"},
                                    {"amount": -20000, "category_id": groceries}]}),
    );
    assert_matches(&server.month(&plan, "2026-08"), &json!({"income": 0}), "August");
    assert_matches(&server.month(&plan, "2026-09"), &json!({"income": 70000}), "September");
    assert_matches(
        &server.month(&plan, "2026-10"),
        &json!({"income": 1000000 + 200000 - 5000 + 1800000 - 100000 + 80000, "activity": -5000}),
        "October",
    );
    assert_matches(&server.month(&plan, "2026-11"), &json!({"income": 1800000, "activity": 0}), "November");
}

#[test]
fn spec_11_6_assigned_and_activity_sum_the_categories() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let bills = id_of(&server.create_group(&b.plan, "Bills"));
    let rent = id_of(&server.create_category(&b.plan, &bills, "Rent"));
    b.txn(&server, &b.checking, "2026-10-01", 5000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 400000);
    b.assign(&server, "2026-10", &b.household, -50000);
    b.assign(&server, "2026-10", &rent, 1200000);
    b.assign(&server, "2026-11", &rent, 1200000);
    b.txn(&server, &b.checking, "2026-10-02", -1200000, &rent);
    b.txn(&server, &b.wallet, "2026-10-03", -70000, &b.groceries);
    b.txn(&server, &b.savings, "2026-10-04", 9000, &b.household);
    assert_matches(
        &b.month(&server, "2026-10"),
        &json!({"income": 5000000, "assigned": 1550000, "activity": -1261000,
                "categories": [entry(&b.groceries, 400000, -70000, 330000, 0),
                               entry(&b.household, -50000, 9000, -41000, 41000),
                               entry(&rent, 1200000, -1200000, 0, 0)]}),
        "October",
    );
    assert_matches(&b.month(&server, "2026-11"), &json!({"assigned": 1200000, "activity": 0}), "November");
}

#[test]
fn spec_11_6_cash_overspending_last_month_sums_the_previous_month() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, "ready_to_assign");
    b.assign(&server, "2026-10", &b.groceries, 50000);
    b.txn(&server, &b.checking, "2026-10-11", -80000, &b.groceries);
    b.txn(&server, &b.wallet, "2026-10-12", -20000, &b.household);
    b.txn(&server, &b.checking, "2026-11-12", -5000, &b.household);
    for (month, last) in [("2026-10", 0), ("2026-11", 50000), ("2026-12", 5000), ("2027-01", 0)] {
        assert_eq!(b.month(&server, month)["cash_overspending_last_month"], last, "{month}");
    }
}

#[test]
fn spec_11_6_categories_go_group_by_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let power = id_of(&server.create_category(&plan, &bills, "Power"));
    let fun = id_of(&server.create_group(&plan, "Fun"));
    let games = id_of(&server.create_category(&plan, &fun, "Games"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    server.create_group(&plan, "Empty");
    let later = id_of(&server.create_group(&plan, "Later"));
    let books = id_of(&server.create_category(&plan, &later, "Books"));
    server.patch_ok(&format!("/plans/{plan}/categories/{power}"), &json!({"hidden": true}));
    server.patch_ok(&format!("/plans/{plan}/category_groups/{fun}"), &json!({"hidden": true}));
    let ids = |month: &str| category_ids(&server.month(&plan, month));
    assert_eq!(ids("2026-10"), vec![rent.clone(), power.clone(), water.clone(), games.clone(), books.clone()]);
    server.patch_ok(&format!("/plans/{plan}/categories/{rent}"), &json!({"group_id": fun}));
    assert_eq!(ids("2026-10"), vec![power.clone(), water.clone(), games.clone(), rent.clone(), books.clone()]);
    server.patch_ok(&format!("/plans/{plan}/categories/{water}"), &json!({"group_id": later}));
    assert_eq!(ids("2026-11"), vec![power, games, rent, books, water]);
}

#[test]
fn spec_11_6_new_categories_join_the_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 1000);
    let late = id_of(&server.create_category(&b.plan, &b.group, "Late"));
    assert_matches(
        &b.month(&server, "2026-10"),
        &json!({"categories": [entry(&b.groceries, 1000, 0, 1000, 0), entry(&b.household, 0, 0, 0, 0),
                               entry(&late, 0, 0, 0, 0)]}),
        "October",
    );
}

#[test]
fn spec_11_6_plans_have_separate_months() {
    let server = Server::start();
    let a = CashBudget::new(&server);
    let b = CashBudget::new(&server);
    a.txn(&server, &a.checking, "2026-10-01", 1000000, "ready_to_assign");
    a.assign(&server, "2026-10", &a.groceries, 100000);
    a.txn(&server, &a.checking, "2026-10-02", -150000, &a.groceries);
    assert_matches(
        &b.month(&server, "2026-10"),
        &empty_month("2026-10", vec![entry(&b.groceries, 0, 0, 0, 0), entry(&b.household, 0, 0, 0, 0)]),
        "the other plan",
    );
    assert_matches(&b.month(&server, "2026-11"), &json!({"cash_overspending_last_month": 0, "ready_to_assign": 0}), "November");
    assert_matches(&a.month(&server, "2026-10"), &json!({"income": 1000000, "activity": -150000, "ready_to_assign": 900000}), "first plan");
}
