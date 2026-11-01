//! Edge cases for sections 3 to 7 that the other files don't reach: HEAD outside plans,
//! requests answered 405 changing nothing, and side effects of renames and deletes.

use mybudget_testsuite::*;

fn account_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/accounts/{id}")
}

fn payee_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/payees/{id}")
}

fn category_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/categories/{id}")
}

fn group_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/category_groups/{id}")
}

/// Everything a plan holds, as its lists show it.
fn snapshot(server: &Server, plan: &str) -> Value {
    json!({
        "plan": server.get_json(&format!("/plans/{plan}")),
        "accounts": server.accounts(plan),
        "payees": server.payees(plan),
        "categories": server.groups(plan),
    })
}

/// A plan with a funded checking account, a credit account, a payee, a group and a category.
fn populated_plan(server: &Server, name: &str) -> (String, Value, Value, Value, Value, Value) {
    let plan = server.plan_id(name);
    let checking = server.add_account(&plan, "Checking", "checking", 2500000);
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let payee = server.create_payee(&plan, "Corner Shop");
    let group = server.create_group(&plan, "Bills");
    let category = server.create_category(&plan, &id_of(&group), "Rent");
    (plan, checking, visa, payee, group, category)
}

// ---- 3.2 / 3.4: HEAD ----

#[test]
fn spec_3_2_head_responses_never_have_a_body() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (path, status) in [
        ("/health".to_owned(), 200),
        ("/plans".to_owned(), 200),
        (format!("/plans/{plan}"), 200),
        ("/no-such-path".to_owned(), 404),
        ("/plans/no-such-plan".to_owned(), 404),
        (format!("/plans/{plan}/no-such-thing"), 404),
        (format!("/plans/{plan}/category_groups"), 405),
        ("/plans/no-such-plan/category_groups".to_owned(), 405),
    ] {
        let res = server.head(&path);
        assert_status(&res, status, &format!("HEAD {path}"));
        assert!(res.body.is_empty(), "HEAD {path} ({status}) returned a body: {:?}", res.text());
    }
}

#[test]
fn spec_3_4_head_gets_the_get_status_outside_plans() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for path in [
        "/health".to_owned(),
        "/plans".to_owned(),
        format!("/plans/{plan}"),
        "/plans/no-such-plan".to_owned(),
        "/no-such-path".to_owned(),
        "/health/".to_owned(),
    ] {
        let get = server.get(&path);
        let head = server.head(&path);
        assert_eq!(head.status, get.status, "HEAD {path} got {}, GET got {}", head.status, get.status);
    }
}

#[test]
fn spec_3_4_long_ids_that_name_nothing_return_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let long = "a".repeat(8000);
    for (method, path) in [
        ("GET", format!("/plans/{long}")),
        ("DELETE", format!("/plans/{long}")),
        ("GET", format!("/plans/{long}/accounts")),
        ("GET", account_url(&plan, &long)),
        ("PATCH", payee_url(&plan, &long)),
        ("GET", category_url(&plan, &long)),
        ("DELETE", group_url(&plan, &long)),
    ] {
        let res = server.request_raw(method, &path, r#"{"name": "x"}"#);
        assert_error(&res, 404, &format!("{method} {} chars", path.len()));
    }
    assert_eq!(server.list("/plans").len(), 1);
}

// ---- 3.6 ----

#[test]
fn spec_3_6_ignores_odd_but_valid_query_strings() {
    let server = Server::start();
    let (plan, checking, ..) = populated_plan(&server, "Household");
    let before = snapshot(&server, &plan);
    for query in ["?", "?&&", "?=", "?x", "?x=%FF%FE", "?x=%00&y=", "?name=Other&name=&type=savings", "?a=1;b=2"] {
        let plans = server.list(&format!("/plans{query}"));
        assert_eq!(plans.len(), 1, "GET /plans{query}");
        assert_eq!(server.list(&format!("/plans/{plan}/accounts{query}")), before["accounts"].as_array().unwrap().clone());
        let got = server.get_json(&format!("{}{query}", account_url(&plan, &id_of(&checking))));
        assert_eq!(got["id"], checking["id"], "GET account{query}");
        let res = server.patch(&format!("{}{query}", account_url(&plan, &id_of(&checking))), &json!({}));
        assert_status(&res, 200, &format!("PATCH account{query}"));
    }
    assert_eq!(snapshot(&server, &plan), before);
}

// ---- 3.11 ----

#[test]
fn spec_3_11_method_not_allowed_requests_change_nothing() {
    let server = Server::start();
    let (plan, checking, _visa, payee, group, category) = populated_plan(&server, "Household");
    let other = server.plan_id("Other");
    let before = snapshot(&server, &plan);
    let plans_before = server.list("/plans");
    let p = format!("/plans/{plan}");
    for (method, path, body) in [
        ("DELETE", "/plans".to_owned(), json!({})),
        ("PATCH", "/plans".to_owned(), json!({"name": "Renamed", "currency_code": "EUR"})),
        ("PUT", p.clone(), json!({"name": "Renamed"})),
        ("POST", p.clone(), json!({"name": "Renamed"})),
        ("DELETE", format!("{p}/accounts"), json!({})),
        ("PATCH", format!("{p}/accounts"), json!({"name": "Renamed", "closed": true})),
        ("PUT", account_url(&plan, &id_of(&checking)), json!({"name": "Renamed"})),
        ("POST", account_url(&plan, &id_of(&checking)), json!({"name": "Renamed", "type": "checking"})),
        ("DELETE", format!("{p}/payees"), json!({})),
        ("PUT", payee_url(&plan, &id_of(&payee)), json!({"name": "Renamed"})),
        ("POST", payee_url(&plan, &id_of(&payee)), json!({"name": "Renamed"})),
        ("DELETE", format!("{p}/categories"), json!({})),
        ("PATCH", format!("{p}/categories"), json!({"name": "Renamed"})),
        ("DELETE", category_url(&plan, &id_of(&category)), json!({})),
        ("POST", category_url(&plan, &id_of(&category)), json!({"group_id": id_of(&group), "name": "Renamed"})),
        ("DELETE", format!("{p}/category_groups"), json!({})),
        ("PATCH", format!("{p}/category_groups"), json!({"hidden": true})),
        ("PUT", group_url(&plan, &id_of(&group)), json!({"name": "Renamed"})),
        ("POST", group_url(&plan, &id_of(&group)), json!({"name": "Renamed"})),
    ] {
        let res = server.request_raw(method, &path, &body.to_string());
        assert_error(&res, 405, &format!("{method} {path}"));
    }
    assert_eq!(server.list("/plans"), plans_before, "a 405 request changed the plans");
    assert_eq!(snapshot(&server, &plan), before, "a 405 request changed the plan's contents");
    assert!(server.accounts(&other).is_empty());
}

// ---- 4.4 ----

#[test]
fn spec_4_4_deleting_a_plan_leaves_other_plans_whole() {
    let server = Server::start();
    let (doomed, ..) = populated_plan(&server, "Doomed");
    let (kept, checking, visa, payee, group, category) = populated_plan(&server, "Kept");
    let before = snapshot(&server, &kept);
    assert_status(&server.delete(&format!("/plans/{doomed}")), 204, "DELETE plan");
    assert_eq!(snapshot(&server, &kept), before, "deleting one plan changed another");
    for path in [
        account_url(&kept, &id_of(&checking)),
        account_url(&kept, &id_of(&visa)),
        payee_url(&kept, &id_of(&payee)),
        payee_url(&kept, &str_field(&checking, "transfer_payee_id")),
        group_url(&kept, &id_of(&group)),
        category_url(&kept, &id_of(&category)),
        category_url(&kept, &str_field(&visa, "payment_category_id")),
    ] {
        server.get_json(&path);
    }
    // The kept plan's Starting Balance payee is still the one its starting balances use.
    let res = server.delete(&payee_url(
        &kept,
        &id_of(server.payees(&kept).iter().find(|p| p["name"] == "Starting Balance").expect("Starting Balance payee")),
    ));
    assert_error(&res, 409, "DELETE the kept plan's Starting Balance payee, still in use");
}

// ---- 5.7 ----

#[test]
fn spec_5_7_rename_keeps_payment_category_note_hidden_and_place() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let amex = server.add_account(&plan, "Amex", "creditCard", 0);
    let visa = server.add_account(&plan, "Visa", "creditCard", -350000);
    let line = server.add_account(&plan, "Line", "lineOfCredit", 0);
    let visa_category = str_field(&visa, "payment_category_id");
    let url = category_url(&plan, &visa_category);
    server.patch_ok(&url, &json!({"note": "pay in full", "hidden": true}));
    let before = server.get_json(&url);

    let renamed = server.patch_ok(&account_url(&plan, &id_of(&visa)), &json!({"name": "Visa Gold", "note": "gold"}));
    assert_matches(
        &renamed,
        &json!({"name": "Visa Gold", "note": "gold", "payment_category_id": visa_category, "balance": -350000}),
        "PATCH Visa",
    );
    let after = server.get_json(&url);
    assert_matches(
        &after,
        &json!({"id": visa_category, "name": "Visa Gold", "note": "pay in full", "hidden": true,
                "group_id": before["group_id"], "payment_account_id": id_of(&visa)}),
        "payment category after the account's rename",
    );
    let internal = &server.groups(&plan)[0];
    let listed = internal["categories"].as_array().unwrap();
    assert_eq!(
        ids_of(listed),
        [str_field(&amex, "payment_category_id"), visa_category.clone(), str_field(&line, "payment_category_id")],
        "renaming moved the payment category"
    );
    assert_eq!(names_of(listed), ["Amex", "Visa Gold", "Line"]);
}

#[test]
fn spec_5_7_rename_back_and_forth_keeps_names_in_step() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let url = account_url(&plan, &id_of(&visa));
    let payee = payee_url(&plan, &str_field(&visa, "transfer_payee_id"));
    let category = category_url(&plan, &str_field(&visa, "payment_category_id"));
    for name in ["Visa Gold", "visa", "Visa", " Visa ", "💳"] {
        server.patch_ok(&url, &json!({"name": name}));
        assert_eq!(server.get_json(&url)["name"], name);
        assert_eq!(server.get_json(&payee)["name"], format!("Transfer : {name}"), "transfer payee after rename to {name:?}");
        assert_eq!(server.get_json(&category)["name"], name, "payment category after rename to {name:?}");
    }
}

// ---- 6.2 ----

#[test]
fn spec_6_2_uniqueness_ignores_transfer_payees_after_account_renames() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let shop = server.create_payee(&plan, "Transfer : Savings");
    // The account's transfer payee takes the name the user payee has; neither conflicts.
    server.patch_ok(&account_url(&plan, &id_of(&checking)), &json!({"name": "Savings"}));
    let transfer = server.get_json(&payee_url(&plan, &str_field(&checking, "transfer_payee_id")));
    assert_eq!(transfer["name"], "Transfer : Savings");
    assert_eq!(server.get_json(&payee_url(&plan, &id_of(&shop)))["name"], "Transfer : Savings");
    // The user payee still holds the name against other user payees.
    let res = server.post(&format!("/plans/{plan}/payees"), &json!({"name": "Transfer : Savings"}));
    assert_error(&res, 409, "POST a second user payee named Transfer : Savings");
    let other = server.create_payee(&plan, "Other");
    let res = server.patch(&payee_url(&plan, &id_of(&other)), &json!({"name": "Transfer : Savings"}));
    assert_error(&res, 409, "rename a user payee to Transfer : Savings");
    // The name the transfer payee gave up was never taken.
    server.create_payee(&plan, "Transfer : Checking");
    // And the user payee keeps its name without conflict.
    server.patch_ok(&payee_url(&plan, &id_of(&shop)), &json!({"name": "Transfer : Savings"}));
}

// ---- 7.4 ----

#[test]
fn spec_7_4_group_may_be_renamed_to_a_name_in_use() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let fun = id_of(&server.create_group(&plan, "Fun"));
    server.create_category(&plan, &fun, "Games");
    for name in ["Bills", "Credit Card Payments", "Games"] {
        let patched = server.patch_ok(&group_url(&plan, &fun), &json!({"name": name}));
        assert_eq!(patched["name"], name);
        assert_eq!(patched["internal"], false);
    }
    let groups = server.groups(&plan);
    assert_eq!(names_of(&groups), ["Credit Card Payments", "Bills", "Games"]);
    assert_eq!(ids_of(&groups[1..]), [bills, fun]);
    assert_eq!(groups[0]["internal"], true);
}
