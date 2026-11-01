//! More checks for SPEC sections 5 to 7: combinations the other files don't cover.

use mybudget_testsuite::{Server, Value, assert_error, assert_matches, assert_status, id_of, ids_of, json, spec_fields, str_field};

fn path(plan: &str, rest: &str) -> String {
    format!("/plans/{plan}{rest}")
}

const TYPES: [&str; 7] = ["checking", "savings", "cash", "creditCard", "lineOfCredit", "otherAsset", "otherLiability"];

// ---- 5.4 ----

#[test]
fn spec_5_4_starting_balance_of_either_sign_is_balance_and_cleared_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for ty in TYPES {
        for amount in [1, -1, 123_450, -987_650] {
            let account = server.add_account(&plan, "A", ty, amount);
            let want = json!({"balance": amount, "cleared_balance": amount, "uncleared_balance": 0});
            assert_matches(&account, &want, &format!("POST {ty} {amount}"));
            let got = server.get_json(&path(&plan, &format!("/accounts/{}", id_of(&account))));
            assert_matches(&got, &want, &format!("GET {ty} {amount}"));
        }
    }
}

// ---- 5.6 ----

#[test]
fn spec_5_6_get_account_matches_its_list_entry() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 2_500_000);
    let visa = server.add_account(&plan, "Visa", "creditCard", -350_000);
    let house = server.add_account(&plan, "House", "otherAsset", 0);
    server.patch_ok(&path(&plan, &format!("/accounts/{}", id_of(&house))), &json!({"note": "n", "closed": true}));
    server.patch_ok(&path(&plan, &format!("/accounts/{}", id_of(&visa))), &json!({"name": "Visa Gold"}));
    let listed = server.accounts(&plan);
    assert_eq!(ids_of(&listed), [id_of(&checking), id_of(&visa), id_of(&house)]);
    for entry in &listed {
        let got = server.get_json(&path(&plan, &format!("/accounts/{}", id_of(entry))));
        assert_eq!(spec_fields(&got), spec_fields(entry), "GET account differs from its list entry");
    }
}

// ---- 5.9 ----

#[test]
fn spec_5_9_delete_keeps_user_payee_and_category_named_like_the_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let payee = server.create_payee(&plan, "Transfer : Visa");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let category = server.create_category(&plan, &bills, "Visa");
    assert_status(&server.delete(&path(&plan, &format!("/accounts/{}", id_of(&visa)))), 204, "DELETE Visa");
    assert_eq!(ids_of(&server.payees(&plan)), [id_of(&payee)]);
    let got = server.get_json(&path(&plan, &format!("/payees/{}", id_of(&payee))));
    assert_matches(&got, &json!({"name": "Transfer : Visa", "transfer_account_id": null}), "user payee");
    let got = server.get_json(&path(&plan, &format!("/categories/{}", id_of(&category))));
    assert_matches(&got, &json!({"name": "Visa", "group_id": bills, "payment_account_id": null}), "user category");
    let groups = server.groups(&plan);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["categories"], json!([]));
    assert_eq!(ids_of(groups[1]["categories"].as_array().unwrap()), [id_of(&category)]);
}

#[test]
fn spec_5_9_delete_of_one_account_keeps_the_others() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let a = server.add_account(&plan, "Visa", "creditCard", 0);
    let b = server.add_account(&plan, "Visa", "creditCard", 0);
    let c = server.add_account(&plan, "Visa", "checking", 0);
    assert_status(&server.delete(&path(&plan, &format!("/accounts/{}", id_of(&b)))), 204, "DELETE second Visa");
    assert_eq!(ids_of(&server.accounts(&plan)), [id_of(&a), id_of(&c)]);
    let payees: Vec<String> = [&a, &c].iter().map(|x| str_field(x, "transfer_payee_id")).collect();
    assert_eq!(ids_of(&server.payees(&plan)), payees);
    let internal = &server.groups(&plan)[0];
    assert_eq!(ids_of(internal["categories"].as_array().unwrap()), [str_field(&a, "payment_category_id")]);
}

// ---- 6.1 ----

#[test]
fn spec_6_1_get_payee_matches_its_list_entry() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 1000);
    server.create_payee(&plan, "Corner Shop");
    let listed = server.payees(&plan);
    assert_eq!(listed.len(), 3);
    for entry in &listed {
        let got = server.get_json(&path(&plan, &format!("/payees/{}", id_of(entry))));
        assert_eq!(spec_fields(&got), spec_fields(entry), "GET payee differs from its list entry");
    }
}

// ---- 6.3 ----

#[test]
fn spec_6_3_rename_to_starting_balance_payee_name_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 1000);
    let shop = server.create_payee(&plan, "Corner Shop");
    let url = path(&plan, &format!("/payees/{}", id_of(&shop)));
    assert_error(&server.patch(&url, &json!({"name": "Starting Balance"})), 409, "rename to Starting Balance");
    assert_eq!(server.get_json(&url)["name"], "Corner Shop");
}

#[test]
fn spec_6_3_payee_may_take_a_name_another_payee_gave_up() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let a = server.create_payee(&plan, "Shop");
    let b = server.create_payee(&plan, "Market");
    let url_a = path(&plan, &format!("/payees/{}", id_of(&a)));
    let url_b = path(&plan, &format!("/payees/{}", id_of(&b)));
    assert_error(&server.patch(&url_b, &json!({"name": "Shop"})), 409, "rename to a taken name");
    server.patch_ok(&url_a, &json!({"name": "Old Shop"}));
    let patched = server.patch_ok(&url_b, &json!({"name": "Shop"}));
    assert_matches(&patched, &json!({"id": id_of(&b), "name": "Shop", "transfer_account_id": null}), "PATCH");
}

// ---- 7.4 ----

#[test]
fn spec_7_4_get_group_matches_its_list_entry() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Visa", "creditCard", 0);
    let bills = id_of(&server.create_group(&plan, "Bills"));
    server.create_category(&plan, &bills, "Rent");
    server.create_category(&plan, &bills, "Water");
    server.create_group(&plan, "Empty");
    let listed = server.groups(&plan);
    assert_eq!(listed.len(), 3);
    for entry in &listed {
        let got = server.get_json(&path(&plan, &format!("/category_groups/{}", id_of(entry))));
        assert_eq!(spec_fields(&got), spec_fields(entry), "GET group differs from its list entry");
    }
}

#[test]
fn spec_7_4_deleting_a_group_keeps_the_others_in_order() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let a = id_of(&server.create_group(&plan, "A"));
    let b = id_of(&server.create_group(&plan, "B"));
    let c = id_of(&server.create_group(&plan, "C"));
    let rent = id_of(&server.create_category(&plan, &c, "Rent"));
    assert_status(&server.delete(&path(&plan, &format!("/category_groups/{b}"))), 204, "DELETE B");
    let d = id_of(&server.create_group(&plan, "D"));
    let groups = server.groups(&plan);
    assert_eq!(ids_of(&groups[1..]), [a, c, d]);
    assert_eq!(ids_of(groups[2]["categories"].as_array().unwrap()), [rent]);
}

#[test]
fn spec_7_4_patch_returns_the_group_as_get_does() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    server.create_category(&plan, &bills, "Rent");
    let url = path(&plan, &format!("/category_groups/{bills}"));
    let patched = server.patch_ok(&url, &json!({"name": "Monthly bills", "hidden": true}));
    assert_eq!(spec_fields(&patched), spec_fields(&server.get_json(&url)));
    assert_matches(&patched, &json!({"name": "Monthly bills", "hidden": true, "internal": false}), "PATCH group");
}

// ---- 7.6 ----

#[test]
fn spec_7_6_moving_keeps_the_categorys_other_fields() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let housing = id_of(&server.create_group(&plan, "Housing"));
    let rent = server.create(&path(&plan, "/categories"), &json!({"group_id": bills, "name": "Rent", "note": "n"}));
    let url = path(&plan, &format!("/categories/{}", id_of(&rent)));
    server.patch_ok(&url, &json!({"hidden": true}));
    let moved = server.patch_ok(&url, &json!({"group_id": housing}));
    let want = json!({"id": id_of(&rent), "group_id": housing, "name": "Rent", "hidden": true, "note": "n",
                      "payment_account_id": null});
    assert_matches(&moved, &want, "PATCH group_id");
    assert_matches(&server.get_json(&url), &want, "GET moved category");
    let listed = server.groups(&plan);
    assert_eq!(listed[1]["categories"], json!([]));
    assert_matches(&listed[2]["categories"], &json!([want]), "listed in new group");
}

#[test]
fn spec_7_6_rejected_move_leaves_category_in_place() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let housing = id_of(&server.create_group(&plan, "Housing"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    let url = path(&plan, &format!("/categories/{rent}"));
    for body in [
        json!({"group_id": housing, "name": ""}),
        json!({"group_id": housing, "hidden": "yes"}),
        json!({"group_id": housing, "note": 5}),
        json!({"group_id": bills, "name": 5}),
        json!({"group_id": "no-such-group", "name": "Rent 2"}),
    ] {
        assert_status(&server.patch(&url, &body), 400, &format!("PATCH {body}"));
    }
    let groups = server.groups(&plan);
    assert_eq!(ids_of(groups[1]["categories"].as_array().unwrap()), [rent.clone(), water]);
    assert_eq!(groups[2]["categories"], json!([]));
    assert_matches(&server.get_json(&url), &json!({"group_id": bills, "name": "Rent"}), "GET");
}

#[test]
fn spec_7_6_get_category_matches_its_list_entry() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Visa", "creditCard", 0);
    let bills = id_of(&server.create_group(&plan, "Bills"));
    server.create(&path(&plan, "/categories"), &json!({"group_id": bills, "name": "Rent", "note": "n"}));
    let entries: Vec<Value> =
        server.groups(&plan).iter().flat_map(|g| g["categories"].as_array().unwrap().clone()).collect();
    assert_eq!(entries.len(), 2);
    for entry in &entries {
        let got = server.get_json(&path(&plan, &format!("/categories/{}", id_of(entry))));
        assert_eq!(spec_fields(&got), spec_fields(entry), "GET category differs from its list entry");
    }
}
