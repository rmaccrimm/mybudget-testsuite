//! SPEC section 7: categories and category groups.

use mybudget_testsuite::{
    Server, Value, assert_error, assert_matches, assert_status, id_of, ids_of, json, names_of, spec_fields, str_field,
};

fn categories_url(plan: &str) -> String {
    format!("/plans/{plan}/categories")
}

fn category_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/categories/{id}")
}

fn groups_url(plan: &str) -> String {
    format!("/plans/{plan}/category_groups")
}

fn group_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/category_groups/{id}")
}

fn account_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/accounts/{id}")
}

/// The categories a group object lists.
fn categories_of(group: &Value) -> Vec<Value> {
    group["categories"].as_array().unwrap_or_else(|| panic!("`categories` is not an array: {group}")).clone()
}

/// The internal `Credit Card Payments` group, listed first (7.1).
fn internal_group(server: &Server, plan: &str) -> Value {
    let groups = server.groups(plan);
    let first = groups.first().unwrap_or_else(|| panic!("GET categories listed no groups")).clone();
    assert_eq!(first["internal"], true, "first group isn't internal: {first}");
    first
}

/// The group with id `id` from `GET categories`.
fn listed_group(server: &Server, plan: &str, id: &str) -> Value {
    let groups = server.groups(plan);
    groups.iter().find(|g| g["id"] == id).cloned().unwrap_or_else(|| panic!("group {id} not listed: {groups:?}"))
}

// ---- 7.1 ----

#[test]
fn spec_7_1_new_plan_has_only_credit_card_payments_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let groups = server.get_json(&categories_url(&plan));
    let id = id_of(&groups[0]);
    assert_matches(
        &groups,
        &json!([{"id": id, "name": "Credit Card Payments", "hidden": false, "internal": true, "categories": []}]),
        "GET categories of a new plan",
    );
}

#[test]
fn spec_7_1_lists_categories_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", -350000);
    let bills = server.create_group(&plan, "Bills");
    let rent = server.create_category(&plan, &id_of(&bills), "Rent");
    let groups = server.get_json(&categories_url(&plan));
    let ccp = id_of(&groups[0]);
    assert_matches(
        &groups,
        &json!([
            {"id": ccp, "name": "Credit Card Payments", "hidden": false, "internal": true,
             "categories": [{"id": visa["payment_category_id"], "group_id": ccp, "name": "Visa",
                             "hidden": false, "note": null, "payment_account_id": id_of(&visa)}]},
            {"id": id_of(&bills), "name": "Bills", "hidden": false, "internal": false,
             "categories": [{"id": id_of(&rent), "group_id": id_of(&bills), "name": "Rent",
                             "hidden": false, "note": null, "payment_account_id": null}]}
        ]),
        "GET categories",
    );
}

#[test]
fn spec_7_1_groups_follow_internal_group_oldest_first() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let names = ["Zeta", "Alpha", "Credit Card Payments", "mid", "Alpha"];
    let mut created: Vec<String> = names.iter().map(|n| id_of(&server.create_group(&plan, n))).collect();
    server.add_account(&plan, "Visa", "creditCard", 0);
    let groups = server.groups(&plan);
    assert_eq!(groups[0]["internal"], true, "first group: {}", groups[0]);
    assert_eq!(groups[0]["name"], "Credit Card Payments");
    assert_eq!(ids_of(&groups[1..]), created, "own groups are not oldest first");
    for group in &groups[1..] {
        assert_eq!(group["internal"], false, "{group}");
    }

    server.patch_ok(&group_url(&plan, &created[0]), &json!({"name": "AAA", "hidden": true}));
    assert_status(&server.delete(&group_url(&plan, &created[1])), 204, "DELETE Alpha");
    created.remove(1);
    created.push(id_of(&server.create_group(&plan, "Newest")));
    let groups = server.groups(&plan);
    assert_eq!(groups[0]["internal"], true);
    assert_eq!(ids_of(&groups[1..]), created);
}

#[test]
fn spec_7_1_categories_are_in_the_order_they_joined() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let other = id_of(&server.create_group(&plan, "Other"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    let gas = id_of(&server.create_category(&plan, &other, "Gas"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let power = id_of(&server.create_category(&plan, &bills, "Power"));
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), [&*water, &*rent, &*power]);

    server.patch_ok(&category_url(&plan, &water), &json!({"group_id": other}));
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), [&*rent, &*power]);
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &other))), [&*gas, &*water]);

    server.patch_ok(&category_url(&plan, &water), &json!({"group_id": bills}));
    server.patch_ok(&category_url(&plan, &rent), &json!({"name": "AAA Rent", "hidden": true}));
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), [&*rent, &*power, &*water]);
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &other))), [&*gas]);
}

#[test]
fn spec_7_1_payment_categories_are_in_account_order() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    server.add_account(&plan, "Checking", "checking", 0);
    let amex = server.add_account(&plan, "Amex", "creditCard", -1000);
    let line = server.add_account(&plan, "A line", "lineOfCredit", 0);
    let category = |a: &Value| str_field(a, "payment_category_id");
    let listed = categories_of(&internal_group(&server, &plan));
    assert_eq!(ids_of(&listed), [category(&visa), category(&amex), category(&line)]);
    assert_eq!(names_of(&listed), ["Visa", "Amex", "A line"]);

    assert_status(&server.delete(&account_url(&plan, &id_of(&visa))), 204, "DELETE Visa");
    server.patch_ok(&account_url(&plan, &id_of(&amex)), &json!({"name": "AAA Amex"}));
    let newest = server.add_account(&plan, "Newest", "creditCard", 0);
    let listed = categories_of(&internal_group(&server, &plan));
    assert_eq!(ids_of(&listed), [category(&amex), category(&line), category(&newest)]);
}

#[test]
fn spec_7_1_internal_group_keeps_its_id() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let before = id_of(&internal_group(&server, &plan));
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    server.create_group(&plan, "Bills");
    let after = id_of(&internal_group(&server, &plan));
    assert_eq!(before, after, "the internal group's id changed");
    let category = server.get_json(&category_url(&plan, &str_field(&visa, "payment_category_id")));
    assert_eq!(category["group_id"], before.as_str());
}

// ---- 7.2 ----

#[test]
fn spec_7_2_each_credit_account_has_one_payment_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&internal_group(&server, &plan));
    let accounts = [
        server.add_account(&plan, "Visa", "creditCard", -350000),
        server.add_account(&plan, "Visa", "creditCard", 0),
        server.add_account(&plan, "Line", "lineOfCredit", 0),
    ];
    let listed = categories_of(&internal_group(&server, &plan));
    assert_eq!(listed.len(), 3, "{listed:?}");
    for (account, category) in accounts.iter().zip(&listed) {
        assert_eq!(category["id"], account["payment_category_id"]);
        assert_matches(
            category,
            &json!({"group_id": group, "name": account["name"], "payment_account_id": account["id"]}),
            "payment category",
        );
    }
    let closed = id_of(&accounts[1]);
    server.patch_ok(&account_url(&plan, &closed), &json!({"closed": true}));
    server.patch_ok(&account_url(&plan, &closed), &json!({"closed": false}));
    assert_eq!(categories_of(&internal_group(&server, &plan)), listed, "closing and reopening changed the categories");
}

#[test]
fn spec_7_2_payment_category_name_follows_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let url = category_url(&plan, &str_field(&visa, "payment_category_id"));
    for name in ["Visa Gold", " Visa ", "Карта"] {
        server.patch_ok(&account_url(&plan, &id_of(&visa)), &json!({"name": name}));
        assert_eq!(server.get_json(&url)["name"], name);
        assert_eq!(categories_of(&internal_group(&server, &plan))[0]["name"], name);
    }
}

#[test]
fn spec_7_2_user_categories_have_null_payment_account_id() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Visa", "creditCard", 0);
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = server.create_category(&plan, &bills, "Rent");
    assert_matches(&rent, &json!({"payment_account_id": null}), "POST category");
    let got = server.get_json(&category_url(&plan, &id_of(&rent)));
    assert_matches(&got, &json!({"payment_account_id": null}), "GET category");
    let listed = categories_of(&listed_group(&server, &plan, &bills));
    assert_matches(&listed[0], &json!({"payment_account_id": null}), "listed category");
}

#[test]
fn spec_7_2_payment_category_may_share_a_user_categorys_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let user = server.create_category(&plan, &bills, "Visa");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let payment = server.get_json(&category_url(&plan, &str_field(&visa, "payment_category_id")));
    assert_eq!(payment["name"], "Visa");
    assert_ne!(payment["id"], user["id"]);
    assert_eq!(server.get_json(&category_url(&plan, &id_of(&user)))["payment_account_id"], Value::Null);
}

// ---- 7.3 ----

#[test]
fn spec_7_3_creates_group_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.post(&groups_url(&plan), &json!({"name": "Bills"}));
    assert_status(&res, 201, "POST category group");
    let group = res.json();
    let want = json!({"id": id_of(&group), "name": "Bills", "hidden": false, "internal": false, "categories": []});
    assert_matches(&group, &want, "POST response");
    assert_matches(&server.get_json(&group_url(&plan, &id_of(&group))), &want, "GET group");
    let groups = server.groups(&plan);
    assert_eq!(groups.len(), 2);
    assert_matches(&groups[1], &want, "listed group");
}

#[test]
fn spec_7_3_requires_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for body in [json!({}), json!({"Name": "Bills"}), json!({"hidden": false})] {
        assert_status(&server.post(&groups_url(&plan), &body), 400, &format!("POST {body}"));
    }
    assert_eq!(server.groups(&plan).len(), 1);
}

#[test]
fn spec_7_3_rejects_invalid_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for name in [json!(""), json!("  "), json!("\u{2028}"), json!("a".repeat(101)), json!(3), json!(["Bills"])] {
        assert_status(&server.post(&groups_url(&plan), &json!({"name": name})), 400, &format!("POST name {name}"));
    }
    assert_eq!(server.groups(&plan).len(), 1);
}

#[test]
fn spec_7_3_groups_may_share_names() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let a = server.create_group(&plan, "Bills");
    let b = server.create_group(&plan, "Bills");
    let c = server.create_group(&plan, "Credit Card Payments");
    assert_ne!(a["id"], b["id"]);
    assert_eq!(c["internal"], false, "{c}");
    let groups = server.groups(&plan);
    assert_eq!(names_of(&groups), ["Credit Card Payments", "Bills", "Bills", "Credit Card Payments"]);
    assert_eq!(groups[0]["internal"], true);
    assert_eq!(groups[3]["internal"], false);
}

// ---- 7.4 ----

#[test]
fn spec_7_4_gets_group_with_categories() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = server.create_category(&plan, &bills, "Rent");
    let water = server.create_category(&plan, &bills, "Water");
    let got = server.get_json(&group_url(&plan, &bills));
    assert_matches(
        &got,
        &json!({"id": bills, "name": "Bills", "hidden": false, "internal": false,
                "categories": [spec_fields(&rent), spec_fields(&water)]}),
        "GET group",
    );
}

#[test]
fn spec_7_4_gets_internal_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let listed = internal_group(&server, &plan);
    let got = server.get_json(&group_url(&plan, &id_of(&listed)));
    assert_matches(
        &got,
        &json!({"id": id_of(&listed), "name": "Credit Card Payments", "hidden": false, "internal": true,
                "categories": [{"id": visa["payment_category_id"], "name": "Visa", "payment_account_id": visa["id"]}]}),
        "GET internal group",
    );
}

#[test]
fn spec_7_4_hides_group_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let res = server.patch(&group_url(&plan, &bills), &json!({"hidden": true}));
    assert_status(&res, 200, "PATCH hidden");
    let want = json!({"id": bills, "name": "Bills", "hidden": true, "internal": false,
                      "categories": [{"id": rent, "group_id": bills, "name": "Rent",
                                      "hidden": false, "note": null, "payment_account_id": null}]});
    assert_matches(&res.json(), &want, "PATCH response");
    assert_matches(&server.get_json(&group_url(&plan, &bills)), &want, "GET after PATCH");
    assert_matches(&listed_group(&server, &plan, &bills), &want, "listed after PATCH");
}

#[test]
fn spec_7_4_patches_name_and_hidden() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = server.create_category(&plan, &bills, "Rent");
    let url = group_url(&plan, &bills);
    for (body, name, hidden) in [
        (json!({"name": "Monthly Bills"}), "Monthly Bills", false),
        (json!({"hidden": true}), "Monthly Bills", true),
        (json!({"name": " Bills ", "hidden": false}), " Bills ", false),
        (json!({"hidden": false}), " Bills ", false),
    ] {
        let patched = server.patch_ok(&url, &body);
        let want = json!({"id": bills, "name": name, "hidden": hidden, "internal": false, "categories": [spec_fields(&rent)]});
        assert_matches(&patched, &want, &format!("PATCH {body}"));
        assert_matches(&server.get_json(&url), &want, &format!("GET after PATCH {body}"));
    }
}

#[test]
fn spec_7_4_rejects_invalid_values() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let url = group_url(&plan, &bills);
    for body in [
        json!({"name": ""}),
        json!({"name": "   "}),
        json!({"name": "a".repeat(101)}),
        json!({"name": 5}),
        json!({"hidden": "true"}),
        json!({"hidden": 1}),
        json!({"hidden": [true]}),
        json!({"name": "Ok", "hidden": "no"}),
    ] {
        assert_status(&server.patch(&url, &body), 400, &format!("PATCH {body}"));
    }
    assert_matches(&server.get_json(&url), &json!({"name": "Bills", "hidden": false}), "after rejected PATCHes");
}

#[test]
fn spec_7_4_deletes_empty_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let other = id_of(&server.create_group(&plan, "Other"));
    let res = server.delete(&group_url(&plan, &bills));
    assert_status(&res, 204, "DELETE empty group");
    assert!(res.body.is_empty(), "DELETE returned a body: {:?}", res.text());
    assert_status(&server.get(&group_url(&plan, &bills)), 404, "GET deleted group");
    let groups = server.groups(&plan);
    assert_eq!(ids_of(&groups[1..]), [other]);
}

#[test]
fn spec_7_4_delete_group_with_categories_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = server.create_category(&plan, &bills, "Rent");
    let url = group_url(&plan, &bills);
    assert_error(&server.delete(&url), 409, "DELETE group with a category");
    let got = server.get_json(&url);
    assert_matches(&got, &json!({"name": "Bills", "categories": [spec_fields(&rent)]}), "GET after 409");
    server.get_json(&category_url(&plan, &id_of(&rent)));
}

#[test]
fn spec_7_4_deletes_group_after_its_categories_move_out() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let housing = id_of(&server.create_group(&plan, "Housing"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    for category in [&rent, &water] {
        assert_error(&server.delete(&group_url(&plan, &bills)), 409, "DELETE group still holding categories");
        server.patch_ok(&category_url(&plan, category), &json!({"group_id": housing}));
    }
    assert_status(&server.delete(&group_url(&plan, &bills)), 204, "DELETE emptied group");
    let got = server.get_json(&group_url(&plan, &housing));
    assert_eq!(ids_of(&categories_of(&got)), [&*rent, &*water]);
    for category in [&rent, &water] {
        assert_eq!(server.get_json(&category_url(&plan, category))["group_id"], housing.as_str());
    }
}

#[test]
fn spec_7_4_patching_internal_group_name_or_hidden_returns_400() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Visa", "creditCard", 0);
    let before = internal_group(&server, &plan);
    let url = group_url(&plan, &id_of(&before));
    let got_before = server.get_json(&url);
    for body in [
        json!({"name": "Cards"}),
        json!({"name": "Credit Card Payments"}),
        json!({"hidden": true}),
        json!({"hidden": false}),
        json!({"name": "Cards", "hidden": true}),
    ] {
        assert_error(&server.patch(&url, &body), 400, &format!("PATCH internal group {body}"));
    }
    assert_eq!(server.get_json(&url), got_before);
    assert_eq!(internal_group(&server, &plan), before);
}

#[test]
fn spec_7_4_deleting_internal_group_returns_400() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = group_url(&plan, &id_of(&internal_group(&server, &plan)));
    assert_error(&server.delete(&url), 400, "DELETE empty internal group");
    server.add_account(&plan, "Visa", "creditCard", 0);
    assert_error(&server.delete(&url), 400, "DELETE internal group holding a payment category");
    let group = internal_group(&server, &plan);
    assert_eq!(categories_of(&group).len(), 1);
    server.get_json(&url);
}

#[test]
fn spec_7_4_missing_group_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    for id in ["no-such-group".to_owned(), "BILLS_ID".to_owned(), rent, id_of(&visa), plan.clone(), "ready_to_assign".to_owned()] {
        let url = group_url(&plan, &id);
        assert_error(&server.get(&url), 404, &format!("GET category_groups/{id}"));
        assert_error(&server.patch(&url, &json!({"name": "x"})), 404, &format!("PATCH category_groups/{id}"));
        assert_error(&server.delete(&url), 404, &format!("DELETE category_groups/{id}"));
    }
    assert_eq!(server.groups(&plan).len(), 2);
}

#[test]
fn spec_7_4_deleted_group_is_gone_for_every_method() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = group_url(&plan, &id_of(&server.create_group(&plan, "Bills")));
    assert_status(&server.delete(&url), 204, "first DELETE");
    assert_error(&server.delete(&url), 404, "second DELETE");
    assert_error(&server.patch(&url, &json!({"name": "Back"})), 404, "PATCH after DELETE");
    assert_eq!(server.groups(&plan).len(), 1, "PATCH after DELETE resurrected the group");
}

// ---- 7.5 ----

#[test]
fn spec_7_5_creates_category_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let res = server.post(&categories_url(&plan), &json!({"group_id": bills, "name": "Rent"}));
    assert_status(&res, 201, "POST category");
    let rent = res.json();
    let want = json!({"id": id_of(&rent), "group_id": bills, "name": "Rent", "hidden": false, "note": null,
                      "payment_account_id": null});
    assert_matches(&rent, &want, "POST response");
    assert_matches(&server.get_json(&category_url(&plan, &id_of(&rent))), &want, "GET category");
    assert_matches(&listed_group(&server, &plan, &bills)["categories"], &json!([want]), "listed");
}

#[test]
fn spec_7_5_adds_category_at_end_of_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let fun = id_of(&server.create_group(&plan, "Fun"));
    let mut want_bills = Vec::new();
    let mut want_fun = Vec::new();
    for (group, name) in [(&bills, "Water"), (&fun, "Games"), (&bills, "Rent"), (&bills, "Apples"), (&fun, "Art")] {
        let id = id_of(&server.create_category(&plan, group, name));
        if *group == bills { want_bills.push(id) } else { want_fun.push(id) }
        assert_eq!(ids_of(&categories_of(&server.get_json(&group_url(&plan, &bills)))), want_bills);
        assert_eq!(ids_of(&categories_of(&server.get_json(&group_url(&plan, &fun)))), want_fun);
    }
}

#[test]
fn spec_7_5_requires_group_id_and_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    for body in [json!({"name": "Rent"}), json!({"group_id": bills}), json!({}), json!({"group": bills, "name": "Rent"})] {
        assert_status(&server.post(&categories_url(&plan), &body), 400, &format!("POST {body}"));
    }
    assert!(categories_of(&listed_group(&server, &plan, &bills)).is_empty());
}

#[test]
fn spec_7_5_rejects_internal_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Visa", "creditCard", 0);
    let before = internal_group(&server, &plan);
    let res = server.post(&categories_url(&plan), &json!({"group_id": id_of(&before), "name": "Extra"}));
    assert_error(&res, 400, "POST category into the internal group");
    assert_eq!(internal_group(&server, &plan), before);
}

#[test]
fn spec_7_5_rejects_unknown_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let gone = id_of(&server.create_group(&plan, "Gone"));
    assert_status(&server.delete(&group_url(&plan, &gone)), 204, "DELETE group");
    for id in [
        "no-such-group".to_owned(),
        "".to_owned(),
        "ready_to_assign".to_owned(),
        gone,
        rent,
        id_of(&visa),
        str_field(&visa, "payment_category_id"),
        plan.clone(),
    ] {
        let res = server.post(&categories_url(&plan), &json!({"group_id": id, "name": "Extra"}));
        assert_error(&res, 400, &format!("POST category with group_id {id:?}"));
    }
    let groups = server.groups(&plan);
    assert_eq!(groups.len(), 2);
    assert_eq!(categories_of(&groups[1]).len(), 1, "a rejected POST created a category");
}

#[test]
fn spec_7_5_rejects_group_id_of_wrong_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    for group_id in [json!(5), json!(true), json!([bills]), json!({"id": bills})] {
        let res = server.post(&categories_url(&plan), &json!({"group_id": group_id, "name": "Rent"}));
        assert_status(&res, 400, &format!("group_id {group_id}"));
    }
    assert!(categories_of(&listed_group(&server, &plan, &bills)).is_empty());
}

#[test]
fn spec_7_5_note_is_optional_and_nullable() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let url = categories_url(&plan);
    let absent = server.create(&url, &json!({"group_id": bills, "name": "A"}));
    assert_matches(&absent, &json!({"note": null}), "no note");
    let null = server.create(&url, &json!({"group_id": bills, "name": "B", "note": null}));
    assert_matches(&null, &json!({"note": null}), "note null");
    let given = server.create(&url, &json!({"group_id": bills, "name": "C", "note": "due on the 1st"}));
    assert_eq!(given["note"], "due on the 1st");
    assert_eq!(server.get_json(&category_url(&plan, &id_of(&given)))["note"], "due on the 1st");
}

#[test]
fn spec_7_5_rejects_invalid_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    for name in [json!(""), json!(" \t "), json!("\u{a0}"), json!("a".repeat(101)), json!(7), json!(["Rent"])] {
        let res = server.post(&categories_url(&plan), &json!({"group_id": bills, "name": name}));
        assert_status(&res, 400, &format!("name {name}"));
    }
    assert!(categories_of(&listed_group(&server, &plan, &bills)).is_empty());
}

#[test]
fn spec_7_5_rejects_note_of_wrong_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    for note in [json!(1), json!(false), json!(["x"]), json!({"text": "x"})] {
        let res = server.post(&categories_url(&plan), &json!({"group_id": bills, "name": "Rent", "note": note}));
        assert_status(&res, 400, &format!("note {note}"));
    }
    assert!(categories_of(&listed_group(&server, &plan, &bills)).is_empty());
}

// ---- 7.6 ----

#[test]
fn spec_7_6_gets_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = server.create(&categories_url(&plan), &json!({"group_id": bills, "name": "Rent", "note": "n"}));
    let water = server.create_category(&plan, &bills, "Water");
    for category in [&rent, &water] {
        assert_matches(&server.get_json(&category_url(&plan, &id_of(category))), &spec_fields(category), "GET category");
    }
}

#[test]
fn spec_7_6_moves_category_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let housing = id_of(&server.create_group(&plan, "Housing"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let res = server.patch(&category_url(&plan, &rent), &json!({"group_id": housing, "note": "due on the 1st"}));
    assert_status(&res, 200, "PATCH category");
    let want = json!({"id": rent, "group_id": housing, "name": "Rent", "hidden": false,
                      "note": "due on the 1st", "payment_account_id": null});
    assert_matches(&res.json(), &want, "PATCH response");
    assert_matches(&server.get_json(&category_url(&plan, &rent)), &want, "GET after PATCH");
    assert!(categories_of(&listed_group(&server, &plan, &bills)).is_empty(), "Rent still listed in Bills");
    assert_matches(&listed_group(&server, &plan, &housing)["categories"], &json!([want]), "Housing");
    assert!(categories_of(&server.get_json(&group_url(&plan, &bills))).is_empty(), "GET Bills still has Rent");
}

#[test]
fn spec_7_6_moved_category_goes_to_end_of_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let housing = id_of(&server.create_group(&plan, "Housing"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    let a = id_of(&server.create_category(&plan, &housing, "Insurance"));
    let b = id_of(&server.create_category(&plan, &housing, "Repairs"));
    server.patch_ok(&category_url(&plan, &rent), &json!({"group_id": housing}));
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &housing))), [&*a, &*b, &*rent]);
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), [&*water]);
    server.patch_ok(&category_url(&plan, &a), &json!({"group_id": housing, "name": "Home insurance"}));
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &housing))), [&*a, &*b, &*rent]);
    server.patch_ok(&category_url(&plan, &a), &json!({"group_id": bills}));
    server.patch_ok(&category_url(&plan, &a), &json!({"group_id": housing}));
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &housing))), [&*b, &*rent, &*a]);
}

#[test]
fn spec_7_6_own_group_id_does_not_move_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let ids: Vec<String> =
        ["Rent", "Water", "Power"].iter().map(|n| id_of(&server.create_category(&plan, &bills, n))).collect();
    let patched = server.patch_ok(&category_url(&plan, &ids[0]), &json!({"group_id": bills}));
    assert_eq!(patched["group_id"], bills.as_str());
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), ids, "PATCH to own group moved it");
    let patched = server.patch_ok(&category_url(&plan, &ids[1]), &json!({"group_id": bills, "name": "Water bill"}));
    assert_eq!(patched["name"], "Water bill");
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), ids, "PATCH to own group moved it");
}

#[test]
fn spec_7_6_patches_name_note_and_hidden() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let url = category_url(&plan, &rent);
    for (body, name, note, hidden) in [
        (json!({"name": "Rent & rates"}), "Rent & rates", json!(null), false),
        (json!({"note": "due on the 1st"}), "Rent & rates", json!("due on the 1st"), false),
        (json!({"hidden": true}), "Rent & rates", json!("due on the 1st"), true),
        (json!({"note": null, "hidden": false, "name": "Rent"}), "Rent", json!(null), false),
    ] {
        let want = json!({"id": rent, "group_id": bills, "name": name, "note": note, "hidden": hidden,
                          "payment_account_id": null});
        assert_matches(&server.patch_ok(&url, &body), &want, &format!("PATCH {body}"));
        assert_matches(&server.get_json(&url), &want, &format!("GET after PATCH {body}"));
    }
}

#[test]
fn spec_7_6_rejects_moving_to_internal_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let internal = id_of(&internal_group(&server, &plan));
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let res = server.patch(&category_url(&plan, &rent), &json!({"group_id": internal}));
    assert_error(&res, 400, "move a user category into the internal group");
    assert_eq!(server.get_json(&category_url(&plan, &rent))["group_id"], bills.as_str());
    assert!(categories_of(&internal_group(&server, &plan)).is_empty());
}

#[test]
fn spec_7_6_rejects_moving_to_unknown_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    let gone = id_of(&server.create_group(&plan, "Gone"));
    assert_status(&server.delete(&group_url(&plan, &gone)), 204, "DELETE group");
    let url = category_url(&plan, &rent);
    for group_id in [
        json!("no-such-group"),
        json!(""),
        json!("ready_to_assign"),
        json!(gone),
        json!(water),
        json!(plan),
        json!(5),
        json!([bills]),
    ] {
        assert_status(&server.patch(&url, &json!({"group_id": group_id})), 400, &format!("group_id {group_id}"));
    }
    assert_eq!(server.get_json(&url)["group_id"], bills.as_str());
    assert_eq!(ids_of(&categories_of(&listed_group(&server, &plan, &bills))), [&*rent, &*water]);
}

#[test]
fn spec_7_6_rejects_invalid_values() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = server.create(&categories_url(&plan), &json!({"group_id": bills, "name": "Rent", "note": "n"}));
    let url = category_url(&plan, &id_of(&rent));
    for body in [
        json!({"name": ""}),
        json!({"name": "\u{3000}"}),
        json!({"name": "a".repeat(101)}),
        json!({"name": 1}),
        json!({"note": 1}),
        json!({"note": ["n"]}),
        json!({"hidden": "true"}),
        json!({"hidden": 0}),
    ] {
        assert_status(&server.patch(&url, &body), 400, &format!("PATCH {body}"));
    }
    assert_matches(&server.get_json(&url), &spec_fields(&rent), "after rejected PATCHes");
}

#[test]
fn spec_7_6_payment_category_rejects_name_and_group_id() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let internal = id_of(&internal_group(&server, &plan));
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let url = category_url(&plan, &str_field(&visa, "payment_category_id"));
    let before = server.get_json(&url);
    for body in [
        json!({"name": "Card"}),
        json!({"name": "Visa"}),
        json!({"group_id": bills}),
        json!({"group_id": internal}),
        json!({"name": "Visa", "group_id": internal}),
        json!({"note": "pay in full", "name": "Visa"}),
        json!({"hidden": true, "group_id": internal}),
    ] {
        assert_error(&server.patch(&url, &body), 400, &format!("PATCH payment category {body}"));
    }
    assert_eq!(server.get_json(&url), before);
    assert!(categories_of(&listed_group(&server, &plan, &bills)).is_empty());
    assert_eq!(server.get_json(&account_url(&plan, &id_of(&visa)))["name"], "Visa");
}

#[test]
fn spec_7_6_payment_category_accepts_note_and_hidden() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let internal = id_of(&internal_group(&server, &plan));
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let category_id = str_field(&visa, "payment_category_id");
    let url = category_url(&plan, &category_id);
    let patched = server.patch_ok(&url, &json!({"note": "pay in full", "hidden": true}));
    let want = json!({"id": category_id, "group_id": internal, "name": "Visa", "hidden": true,
                      "note": "pay in full", "payment_account_id": id_of(&visa)});
    assert_matches(&patched, &want, "PATCH payment category");
    assert_matches(&server.get_json(&url), &want, "GET payment category");
    let patched = server.patch_ok(&url, &json!({"note": null}));
    assert_matches(&patched, &json!({"note": null, "hidden": true, "name": "Visa"}), "clear note");
}

#[test]
fn spec_7_6_missing_category_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    for id in ["no-such-category".to_owned(), "RENT_ID".to_owned(), bills.clone(), id_of(&checking), plan.clone()] {
        let url = category_url(&plan, &id);
        assert_error(&server.get(&url), 404, &format!("GET categories/{id}"));
        assert_error(&server.patch(&url, &json!({"name": "x"})), 404, &format!("PATCH categories/{id}"));
    }
}

// ---- 7.7 ----

#[test]
fn spec_7_7_ready_to_assign_is_not_a_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    server.create_category(&plan, &bills, "Rent");
    server.add_account(&plan, "Visa", "creditCard", -1000);
    server.add_account(&plan, "Checking", "checking", 1000);
    assert_error(&server.get(&category_url(&plan, "ready_to_assign")), 404, "GET categories/ready_to_assign");
    let res = server.patch(&category_url(&plan, "ready_to_assign"), &json!({"name": "x"}));
    assert_error(&res, 404, "PATCH categories/ready_to_assign");
    for group in server.groups(&plan) {
        assert_ne!(group["id"], "ready_to_assign");
        for category in categories_of(&group) {
            assert_ne!(category["id"], "ready_to_assign", "a group lists ready_to_assign: {group}");
        }
    }
}

#[test]
fn spec_7_7_categories_may_be_named_ready_to_assign() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&server.create_group(&plan, "ready_to_assign"));
    assert_ne!(group, "ready_to_assign");
    for name in ["ready_to_assign", "Ready to Assign", "Inflow: Ready to Assign"] {
        let category = server.create_category(&plan, &group, name);
        assert_ne!(id_of(&category), "ready_to_assign", "a generated id equals ready_to_assign");
        assert_eq!(category["name"], name);
    }
    let visa = server.add_account(&plan, "ready_to_assign", "creditCard", 0);
    assert_ne!(visa["payment_category_id"], "ready_to_assign");
    assert_error(&server.get(&category_url(&plan, "ready_to_assign")), 404, "GET categories/ready_to_assign");
}

// ---- 7.8 ----

#[test]
fn spec_7_8_hidden_group_works_like_visible() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let hidden = id_of(&server.create_group(&plan, "Hidden"));
    let last = id_of(&server.create_group(&plan, "Last"));
    server.patch_ok(&group_url(&plan, &hidden), &json!({"hidden": true}));
    assert_eq!(ids_of(&server.groups(&plan)[1..]), [&*bills, &*hidden, &*last], "hidden group not listed in place");

    let a = id_of(&server.create_category(&plan, &hidden, "A"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    server.patch_ok(&category_url(&plan, &rent), &json!({"group_id": hidden}));
    let got = server.get_json(&group_url(&plan, &hidden));
    assert_matches(&got, &json!({"hidden": true}), "GET hidden group");
    assert_eq!(ids_of(&categories_of(&got)), [&*a, &*rent]);
    server.patch_ok(&group_url(&plan, &hidden), &json!({"name": "Still hidden"}));
    assert_error(&server.delete(&group_url(&plan, &hidden)), 409, "DELETE hidden group with categories");
    server.patch_ok(&category_url(&plan, &a), &json!({"group_id": last}));
    server.patch_ok(&category_url(&plan, &rent), &json!({"group_id": last}));
    assert_status(&server.delete(&group_url(&plan, &hidden)), 204, "DELETE empty hidden group");
}

#[test]
fn spec_7_8_hidden_category_works_like_visible() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let other = id_of(&server.create_group(&plan, "Other"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let water = id_of(&server.create_category(&plan, &bills, "Water"));
    server.patch_ok(&category_url(&plan, &rent), &json!({"hidden": true}));
    let listed = categories_of(&listed_group(&server, &plan, &bills));
    assert_eq!(ids_of(&listed), [&*rent, &*water], "hidden category not listed in place");
    assert_eq!(listed[0]["hidden"], true);
    assert_eq!(server.get_json(&category_url(&plan, &rent))["hidden"], true);
    server.patch_ok(&category_url(&plan, &water), &json!({"group_id": other}));
    assert_error(&server.delete(&group_url(&plan, &bills)), 409, "DELETE group holding only a hidden category");
    let patched = server.patch_ok(&category_url(&plan, &rent), &json!({"name": "Rent (old)", "group_id": other}));
    assert_matches(&patched, &json!({"name": "Rent (old)", "group_id": other, "hidden": true}), "PATCH hidden category");
    assert_status(&server.delete(&group_url(&plan, &bills)), 204, "DELETE emptied group");
}

#[test]
fn spec_7_8_hidden_payment_category_follows_its_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let url = category_url(&plan, &str_field(&visa, "payment_category_id"));
    server.patch_ok(&url, &json!({"hidden": true}));
    server.patch_ok(&account_url(&plan, &id_of(&visa)), &json!({"name": "Visa Gold"}));
    assert_matches(&server.get_json(&url), &json!({"name": "Visa Gold", "hidden": true}), "hidden payment category");
    assert_eq!(categories_of(&internal_group(&server, &plan)).len(), 1);
    assert_status(&server.delete(&account_url(&plan, &id_of(&visa))), 204, "DELETE Visa");
    assert_status(&server.get(&url), 404, "GET hidden payment category of a deleted account");
    assert!(categories_of(&internal_group(&server, &plan)).is_empty());
}
