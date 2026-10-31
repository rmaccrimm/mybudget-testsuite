//! SPEC section 6: payees.

use mybudget_testsuite::{
    Server, Value, assert_error, assert_matches, assert_status, id_of, ids_of, json, names_of, str_field,
};

fn payees_url(plan: &str) -> String {
    format!("/plans/{plan}/payees")
}

fn payee_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/payees/{id}")
}

fn account_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/accounts/{id}")
}

/// The plan's payees named exactly `name`.
fn named(server: &Server, plan: &str, name: &str) -> Vec<Value> {
    server.payees(plan).into_iter().filter(|p| p["name"] == name).collect()
}

// ---- 6.1 ----

#[test]
fn spec_6_1_lists_payees_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 2500000);
    let listed = server.get_json(&payees_url(&plan));
    let starting_balance_id = listed[1]["id"].clone();
    assert!(starting_balance_id.is_string(), "payees: {listed}");
    assert_matches(
        &listed,
        &json!([{"id": checking["transfer_payee_id"], "name": "Transfer : Checking", "transfer_account_id": id_of(&checking)},
                {"id": starting_balance_id, "name": "Starting Balance", "transfer_account_id": null}]),
        "GET payees",
    );
}

#[test]
fn spec_6_1_new_plan_has_no_payees() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.get(&payees_url(&plan));
    assert_status(&res, 200, "GET payees");
    assert_eq!(res.json(), json!([]));
}

#[test]
fn spec_6_1_lists_payees_oldest_first() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let zed = id_of(&server.create_payee(&plan, "Zed"));
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let alpha = id_of(&server.create_payee(&plan, "Alpha"));
    let visa = server.add_account(&plan, "Visa", "creditCard", -350000);
    let mid = id_of(&server.create_payee(&plan, "mid"));
    let starting_balance = id_of(&named(&server, &plan, "Starting Balance")[0]);
    let mut want = vec![
        zed.clone(),
        str_field(&checking, "transfer_payee_id"),
        alpha.clone(),
        str_field(&visa, "transfer_payee_id"),
        starting_balance,
        mid,
    ];
    assert_eq!(ids_of(&server.payees(&plan)), want, "list is not oldest first");

    server.patch_ok(&payee_url(&plan, &zed), &json!({"name": "Aardvark"}));
    server.patch_ok(&account_url(&plan, &id_of(&checking)), &json!({"name": "AAA Checking"}));
    assert_eq!(ids_of(&server.payees(&plan)), want, "renames reordered the list");

    assert_status(&server.delete(&payee_url(&plan, &alpha)), 204, "DELETE Alpha");
    want.retain(|id| *id != alpha);
    let newest = id_of(&server.create_payee(&plan, "Alpha"));
    want.push(newest);
    assert_eq!(ids_of(&server.payees(&plan)), want);
}

#[test]
fn spec_6_1_gets_one_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let got = server.get_json(&payee_url(&plan, &id_of(&shop)));
    assert_matches(&got, &json!({"id": id_of(&shop), "name": "Corner Shop", "transfer_account_id": null}), "GET payee");
    let transfer_id = str_field(&checking, "transfer_payee_id");
    let got = server.get_json(&payee_url(&plan, &transfer_id));
    assert_matches(
        &got,
        &json!({"id": transfer_id, "name": "Transfer : Checking", "transfer_account_id": id_of(&checking)}),
        "GET transfer payee",
    );
}

#[test]
fn spec_6_1_missing_payee_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Visa", "creditCard", 0);
    for id in [
        "no-such-payee".to_owned(),
        "CORNER_SHOP_ID".to_owned(),
        plan.clone(),
        id_of(&account),
        str_field(&account, "payment_category_id"),
    ] {
        assert_error(&server.get(&payee_url(&plan, &id)), 404, &format!("GET payees/{id}"));
    }
}

#[test]
fn spec_6_1_payees_of_missing_plan_return_404() {
    let server = Server::start();
    server.plan_id("Household");
    assert_error(&server.get("/plans/no-such-plan/payees"), 404, "GET payees of a missing plan");
    assert_error(&server.get("/plans/no-such-plan/payees/x"), 404, "GET payee of a missing plan");
}

// ---- 6.2 ----

#[test]
fn spec_6_2_creates_payee_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.post(&payees_url(&plan), &json!({"name": "Corner Shop"}));
    assert_status(&res, 201, "POST payee");
    let payee = res.json();
    assert_matches(&payee, &json!({"id": id_of(&payee), "name": "Corner Shop", "transfer_account_id": null}), "POST");
    assert_eq!(ids_of(&server.payees(&plan)), vec![id_of(&payee)]);
}

#[test]
fn spec_6_2_duplicate_name_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    let res = server.post(&payees_url(&plan), &json!({"name": "Corner Shop"}));
    assert_error(&res, 409, "POST duplicate payee");
    assert_eq!(ids_of(&server.payees(&plan)), vec![id_of(&shop)], "duplicate POST created a payee");
}

#[test]
fn spec_6_2_starting_balance_payee_name_is_taken() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 1000);
    let before = server.payees(&plan);
    let res = server.post(&payees_url(&plan), &json!({"name": "Starting Balance"}));
    assert_error(&res, 409, "POST Starting Balance");
    assert_eq!(server.payees(&plan), before);
}

#[test]
fn spec_6_2_names_are_compared_exactly() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let names = [
        "Corner Shop",
        "corner shop",
        "CORNER SHOP",
        "Corner Shop ",
        " Corner Shop",
        "Corner  Shop",
        "Corner\u{a0}Shop",
        "Café",
        "Cafe\u{301}",
        "ﬁsh",
        "fish",
    ];
    for name in names {
        let res = server.post(&payees_url(&plan), &json!({"name": name}));
        assert_status(&res, 201, &format!("POST {name:?}"));
        assert_eq!(res.json()["name"], name);
    }
    assert_eq!(names_of(&server.payees(&plan)), names);
}

#[test]
fn spec_6_2_name_is_free_again_after_delete_or_rename() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    assert_status(&server.delete(&payee_url(&plan, &id_of(&shop))), 204, "DELETE payee");
    let again = server.create_payee(&plan, "Corner Shop");
    server.patch_ok(&payee_url(&plan, &id_of(&again)), &json!({"name": "Corner Shop Ltd"}));
    let third = server.create_payee(&plan, "Corner Shop");
    assert_eq!(names_of(&server.payees(&plan)), ["Corner Shop Ltd", "Corner Shop"]);
    assert_ne!(id_of(&third), id_of(&again));
}

#[test]
fn spec_6_2_requires_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for body in [json!({}), json!({"Name": "Corner Shop"}), json!({"payee_name": "Corner Shop"})] {
        assert_status(&server.post(&payees_url(&plan), &body), 400, &format!("POST {body}"));
    }
    assert!(server.payees(&plan).is_empty());
}

#[test]
fn spec_6_2_rejects_invalid_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for name in [
        json!(""),
        json!(" "),
        json!("\n\t"),
        json!("\u{2003}"),
        json!("a".repeat(201)),
        json!(5),
        json!(false),
        json!(["Corner Shop"]),
        json!({"name": "Corner Shop"}),
    ] {
        let res = server.post(&payees_url(&plan), &json!({"name": name}));
        assert_status(&res, 400, &format!("POST name {name}"));
    }
    assert!(server.payees(&plan).is_empty());
}

// ---- 6.3 ----

#[test]
fn spec_6_3_renames_payee_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    let id = id_of(&shop);
    let res = server.patch(&payee_url(&plan, &id), &json!({"name": "Corner Shop Ltd"}));
    assert_status(&res, 200, "PATCH payee");
    let want = json!({"id": id, "name": "Corner Shop Ltd", "transfer_account_id": null});
    assert_matches(&res.json(), &want, "PATCH response");
    assert_matches(&server.get_json(&payee_url(&plan, &id)), &want, "GET after PATCH");
    assert_matches(&server.get_json(&payees_url(&plan)), &json!([want]), "list after PATCH");
}

#[test]
fn spec_6_3_rename_to_taken_name_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 1000);
    server.create_payee(&plan, "Bakery");
    let shop = server.create_payee(&plan, "Corner Shop");
    let url = payee_url(&plan, &id_of(&shop));
    for name in ["Bakery", "Starting Balance"] {
        assert_error(&server.patch(&url, &json!({"name": name})), 409, &format!("rename to {name}"));
        assert_eq!(server.get_json(&url)["name"], "Corner Shop", "after rename to {name}");
    }
    assert_eq!(named(&server, &plan, "Bakery").len(), 1);
}

#[test]
fn spec_6_3_keeping_own_name_is_not_a_conflict() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    let url = payee_url(&plan, &id_of(&shop));
    let res = server.patch(&url, &json!({"name": "Corner Shop"}));
    assert_status(&res, 200, "PATCH own name");
    assert_eq!(res.json()["name"], "Corner Shop");
    server.add_account(&plan, "Checking", "checking", 1000);
    let starting_balance = named(&server, &plan, "Starting Balance");
    let res = server.patch(&payee_url(&plan, &id_of(&starting_balance[0])), &json!({"name": "Starting Balance"}));
    assert_status(&res, 200, "PATCH Starting Balance to its own name");
}

#[test]
fn spec_6_3_rename_may_change_only_case() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    let patched = server.patch_ok(&payee_url(&plan, &id_of(&shop)), &json!({"name": "corner shop"}));
    assert_eq!(patched["name"], "corner shop");
    let res = server.post(&payees_url(&plan), &json!({"name": "Corner Shop"}));
    assert_status(&res, 201, "old name after a case-only rename");
}

#[test]
fn spec_6_3_renaming_transfer_payee_returns_400() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let url = payee_url(&plan, &str_field(&checking, "transfer_payee_id"));
    for name in ["Main", "Transfer : Main", "Transfer : Checking", "Corner Shop"] {
        assert_error(&server.patch(&url, &json!({"name": name})), 400, &format!("PATCH transfer payee name {name:?}"));
    }
    let got = server.get_json(&url);
    assert_matches(&got, &json!({"name": "Transfer : Checking", "transfer_account_id": id_of(&checking)}), "after");
    assert_eq!(server.get_json(&account_url(&plan, &id_of(&checking)))["name"], "Checking");
}

#[test]
fn spec_6_3_renaming_transfer_payee_of_closed_account_returns_400() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    server.patch_ok(&account_url(&plan, &id_of(&checking)), &json!({"closed": true}));
    let url = payee_url(&plan, &str_field(&checking, "transfer_payee_id"));
    assert_error(&server.patch(&url, &json!({"name": "Old"})), 400, "rename transfer payee of a closed account");
    assert_eq!(server.get_json(&url)["name"], "Transfer : Checking");
}

#[test]
fn spec_6_3_rejects_invalid_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let shop = server.create_payee(&plan, "Corner Shop");
    let url = payee_url(&plan, &id_of(&shop));
    for name in [json!(""), json!("   "), json!("\u{85}"), json!("a".repeat(201)), json!(5), json!(["x"])] {
        assert_status(&server.patch(&url, &json!({"name": name})), 400, &format!("PATCH name {name}"));
    }
    assert_eq!(server.get_json(&url)["name"], "Corner Shop");
}

#[test]
fn spec_6_3_patch_missing_payee_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    for id in ["no-such-payee".to_owned(), id_of(&checking)] {
        let res = server.patch(&payee_url(&plan, &id), &json!({"name": "x"}));
        assert_error(&res, 404, &format!("PATCH payees/{id}"));
    }
    assert_eq!(server.payees(&plan).len(), 1);
}

// ---- 6.4 ----

#[test]
fn spec_6_4_deleting_transfer_payee_returns_400() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let closed = server.add_account(&plan, "Closed", "savings", 0);
    server.patch_ok(&account_url(&plan, &id_of(&closed)), &json!({"closed": true}));
    for account in [&checking, &closed] {
        let url = payee_url(&plan, &str_field(account, "transfer_payee_id"));
        assert_error(&server.delete(&url), 400, &format!("DELETE transfer payee of {}", account["name"]));
        assert_eq!(server.get_json(&url)["transfer_account_id"], account["id"]);
    }
    assert_eq!(server.payees(&plan).len(), 2);
}

#[test]
fn spec_6_4_deletes_unused_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let shop = server.create_payee(&plan, "Corner Shop");
    let bakery = server.create_payee(&plan, "Bakery");
    let url = payee_url(&plan, &id_of(&shop));
    let res = server.delete(&url);
    assert_status(&res, 204, "DELETE payee");
    assert!(res.body.is_empty(), "DELETE returned a body: {:?}", res.text());
    assert_status(&server.get(&url), 404, "GET deleted payee");
    assert_eq!(ids_of(&server.payees(&plan)), vec![str_field(&checking, "transfer_payee_id"), id_of(&bakery)]);
}

#[test]
fn spec_6_4_deletes_unused_payee_named_starting_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let payee = server.create_payee(&plan, "Starting Balance");
    assert_status(&server.delete(&payee_url(&plan, &id_of(&payee))), 204, "DELETE unused Starting Balance payee");
    assert!(server.payees(&plan).is_empty());
}

#[test]
fn spec_6_4_deleting_payee_used_by_starting_balance_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 2500000);
    let payee = named(&server, &plan, "Starting Balance").remove(0);
    let url = payee_url(&plan, &id_of(&payee));
    assert_error(&server.delete(&url), 409, "DELETE Starting Balance payee in use");
    assert_eq!(server.get_json(&url)["name"], "Starting Balance");
    assert_eq!(server.payees(&plan).len(), 2);
}

#[test]
fn spec_6_4_delete_missing_payee_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let shop = server.create_payee(&plan, "Corner Shop");
    let url = payee_url(&plan, &id_of(&shop));
    assert_error(&server.delete(&payee_url(&plan, "no-such-payee")), 404, "DELETE missing payee");
    assert_error(&server.delete(&payee_url(&plan, &id_of(&checking))), 404, "DELETE payees/<account id>");
    assert_status(&server.delete(&url), 204, "first DELETE");
    assert_error(&server.delete(&url), 404, "second DELETE");
    assert_error(&server.patch(&url, &json!({"name": "Back"})), 404, "PATCH after DELETE");
    assert_eq!(server.payees(&plan).len(), 1, "PATCH after DELETE resurrected the payee");
}

// ---- 6.6 ----

#[test]
fn spec_6_6_payee_may_take_a_transfer_payee_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let res = server.post(&payees_url(&plan), &json!({"name": "Transfer : Checking"}));
    assert_status(&res, 201, "POST payee named like a transfer payee");
    assert_matches(&res.json(), &json!({"name": "Transfer : Checking", "transfer_account_id": null}), "POST");
    let found = named(&server, &plan, "Transfer : Checking");
    assert_eq!(found.len(), 2, "payees: {found:?}");
    assert_eq!(found[0]["transfer_account_id"], checking["id"]);
    // A second user payee with that name conflicts with the first.
    let res = server.post(&payees_url(&plan), &json!({"name": "Transfer : Checking"}));
    assert_error(&res, 409, "second user payee named Transfer : Checking");
}

#[test]
fn spec_6_6_payee_may_be_renamed_to_a_transfer_payee_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 0);
    let shop = server.create_payee(&plan, "Corner Shop");
    let patched = server.patch_ok(&payee_url(&plan, &id_of(&shop)), &json!({"name": "Transfer : Checking"}));
    assert_eq!(patched["name"], "Transfer : Checking");
    assert_eq!(named(&server, &plan, "Transfer : Checking").len(), 2);
}

#[test]
fn spec_6_6_account_may_be_named_after_a_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let user = server.create_payee(&plan, "Transfer : Savings");
    let savings = server.add_account(&plan, "Savings", "savings", 0);
    let transfer = server.get_json(&payee_url(&plan, &str_field(&savings, "transfer_payee_id")));
    assert_eq!(transfer["name"], "Transfer : Savings");
    assert_ne!(transfer["id"], user["id"]);

    server.create_payee(&plan, "Transfer : Gold");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    server.patch_ok(&account_url(&plan, &id_of(&visa)), &json!({"name": "Gold"}));
    let transfer = server.get_json(&payee_url(&plan, &str_field(&visa, "transfer_payee_id")));
    assert_eq!(transfer["name"], "Transfer : Gold");
    assert_eq!(named(&server, &plan, "Transfer : Gold").len(), 2);
    assert_eq!(named(&server, &plan, "Transfer : Savings").len(), 2);
}

#[test]
fn spec_6_6_transfer_payees_may_share_a_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let a = server.add_account(&plan, "Checking", "checking", 0);
    let b = server.add_account(&plan, "Checking", "savings", 0);
    let c = server.add_account(&plan, "Other", "cash", 0);
    server.patch_ok(&account_url(&plan, &id_of(&c)), &json!({"name": "Checking"}));
    let found = named(&server, &plan, "Transfer : Checking");
    let want: Vec<Value> = [&a, &b, &c].iter().map(|acct| acct["id"].clone()).collect();
    let got: Vec<Value> = found.iter().map(|p| p["transfer_account_id"].clone()).collect();
    assert_eq!(got, want, "payees: {found:?}");
}
