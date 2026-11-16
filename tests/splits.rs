//! SPEC section 10: split transactions.

use mybudget_testsuite::{
    Budget, Server, Value, assert_error, assert_matches, assert_status, id_of, json, spec_fields, str_field, with,
};

/// A split of -90000 in Checking: -60000 Groceries, -30000 Household.
fn split_body(b: &Budget) -> Value {
    json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -90000,
           "subtransactions": [{"amount": -60000, "category_id": b.groceries},
                               {"amount": -30000, "category_id": b.household, "memo": "light bulbs"}]})
}

fn subs_of(txn: &Value) -> Vec<Value> {
    txn["subtransactions"].as_array().unwrap_or_else(|| panic!("`subtransactions` isn't an array: {txn}")).clone()
}

// ---- 10.1 ----

#[test]
fn spec_10_1_creates_split_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = id_of(&server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "starting_balance": 2500000, "starting_date": "2026-10-01"}),
    ));
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    let household = id_of(&server.create_category(&plan, &group, "Household"));
    let res = server.post_transaction(
        &plan,
        &json!({"account_id": checking, "date": "2026-10-06", "amount": -90000, "payee_name": "Megastore",
                "subtransactions": [{"amount": -60000, "category_id": groceries},
                                    {"amount": -30000, "category_id": household, "memo": "light bulbs"}]}),
    );
    assert_status(&res, 201, "POST 10.1's example");
    let txn = res.json();
    let megastore: Vec<Value> = server.payees(&plan).into_iter().filter(|p| p["name"] == "Megastore").collect();
    assert_eq!(megastore.len(), 1);
    assert_matches(
        &txn,
        &json!({"id": id_of(&txn), "account_id": checking, "date": "2026-10-06", "amount": -90000,
                "payee_id": megastore[0]["id"], "category_id": null, "memo": null,
                "cleared": "uncleared", "approved": true, "flag_color": null,
                "transfer_account_id": null, "transfer_transaction_id": null,
                "subtransactions": [{"amount": -60000, "category_id": groceries, "memo": null},
                                    {"amount": -30000, "category_id": household, "memo": "light bulbs"}]}),
        "10.1's example",
    );
}

#[test]
fn spec_10_1_subtransaction_amounts_must_add_up() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (amount, subs) in [(-90000, [-60000, -29999]), (-90000, [-60000, -30001]), (-90000, [60000, 30000]), (0, [-1, 0]), (90000, [-60000, -30000])] {
        let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": amount,
                          "subtransactions": [{"amount": subs[0], "category_id": b.groceries},
                                              {"amount": subs[1], "category_id": b.household}]});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST {amount} split as {subs:?}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_10_1_accepts_any_amounts_that_add_up() {
    let server = Server::start();
    let b = Budget::new(&server);
    let max = 1_000_000_000_000_i64;
    for (amount, subs) in [
        (-90000, vec![-90000]),
        (0, vec![-5000, 5000]),
        (0, vec![0, 0]),
        (100, vec![-50, 0, 150]),
        (max, vec![max, max, -max]),
        (-max, vec![-max, 0]),
    ] {
        let items: Vec<Value> = subs.iter().map(|a| json!({"amount": a, "category_id": b.groceries})).collect();
        let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": amount, "subtransactions": items});
        let txn = server.create_transaction(&b.plan, &body);
        let got: Vec<Value> = subs_of(&txn).iter().map(|s| s["amount"].clone()).collect();
        assert_eq!(got, subs.iter().map(|a| json!(a)).collect::<Vec<_>>(), "POST {amount} split as {subs:?}");
    }
}

#[test]
fn spec_10_1_split_category_must_be_null_or_absent() {
    let server = Server::start();
    let b = Budget::new(&server);
    for category in [json!(b.groceries), json!(b.household), json!("ready_to_assign"), json!(b.visa_payment), json!("nope")] {
        let body = with(&split_body(&b), "category_id", Some(category.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST split with category {category}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
    let txn = server.create_transaction(&b.plan, &with(&split_body(&b), "category_id", Some(Value::Null)));
    assert_eq!(txn["category_id"], Value::Null);
}

#[test]
fn spec_10_1_subtransaction_requires_amount_and_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    for sub in [
        json!({"category_id": b.groceries}),
        json!({"amount": -90000}),
        json!({"amount": -90000, "category_id": null}),
        json!({"memo": "x"}),
        json!({}),
    ] {
        let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -90000, "subtransactions": [sub]});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST split with {sub}"));
    }
}

#[test]
fn spec_10_1_rejects_invalid_subtransaction_amount() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [json!(null), json!(-90000.0), json!(-9e4), json!("-90000"), json!(true), json!(-1000000000001_i64), json!([-90000])] {
        let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -90000,
                          "subtransactions": [{"amount": bad, "category_id": b.groceries}]});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST sub amount {bad}"));
    }
}

#[test]
fn spec_10_1_subtransaction_memo_defaults_to_null() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -3,
                      "subtransactions": [{"amount": -1, "category_id": b.groceries},
                                          {"amount": -1, "category_id": b.groceries, "memo": null},
                                          {"amount": -1, "category_id": b.groceries, "memo": "  x  "}]});
    let txn = server.create_transaction(&b.plan, &body);
    let memos: Vec<Value> = subs_of(&txn).iter().map(|s| s["memo"].clone()).collect();
    assert_eq!(memos, vec![json!(null), json!(null), json!("  x  ")]);
}

#[test]
fn spec_10_1_rejects_invalid_subtransaction_memo() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [json!("x".repeat(501)), json!(5), json!(false), json!(["x"])] {
        let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -1,
                          "subtransactions": [{"amount": -1, "category_id": b.groceries, "memo": bad}]});
        assert_error(&server.post_transaction(&b.plan, &body), 400, "POST sub with invalid memo");
    }
    let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -1,
                      "subtransactions": [{"amount": -1, "category_id": b.groceries, "memo": "x".repeat(500)}]});
    server.create_transaction(&b.plan, &body);
}

#[test]
fn spec_10_1_subtransactions_must_be_an_array_of_objects() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [
        json!(null),
        json!({}),
        json!({"amount": -1000, "category_id": b.groceries}),
        json!("[]"),
        json!(5),
        json!(true),
        json!([5]),
        json!([null]),
        json!(["x"]),
        json!([[-1000, b.groceries]]),
        json!([{"amount": -1000, "category_id": b.groceries}, null]),
    ] {
        let body = with(&b.spend(-1000), "subtransactions", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST subtransactions {bad}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_10_1_empty_subtransactions_is_not_a_split() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-1000), "subtransactions", Some(json!([]))));
    assert_matches(&txn, &json!({"category_id": b.groceries, "subtransactions": []}), "POST with []");
    let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -1000, "subtransactions": []});
    assert_error(&server.post_transaction(&b.plan, &body), 400, "POST with [] and no category");
}

// ---- 10.2 ----

#[test]
fn spec_10_2_subtransaction_categories_follow_8_2() {
    let server = Server::start();
    let b = Budget::new(&server);
    let loc_payment = str_field(&server.account(&b.plan, &b.loc.id), "payment_category_id");
    for account in [&b.checking, &b.wallet, &b.visa, &b.loc] {
        for bad in [json!(b.visa_payment), json!(loc_payment), json!("nope"), json!(b.group), json!(b.checking.id), json!(5), json!("")] {
            let body = json!({"account_id": account.id, "date": "2026-10-06", "amount": -3,
                              "subtransactions": [{"amount": -1, "category_id": b.groceries},
                                                  {"amount": -2, "category_id": bad}]});
            assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST sub category {bad} in {account:?}"));
        }
        let body = json!({"account_id": account.id, "date": "2026-10-06", "amount": -3,
                          "subtransactions": [{"amount": -1, "category_id": b.groceries},
                                              {"amount": -2, "category_id": "ready_to_assign"}]});
        let txn = server.create_transaction(&b.plan, &body);
        assert_eq!(subs_of(&txn)[1]["category_id"], "ready_to_assign");
    }
}

#[test]
fn spec_10_2_splits_are_not_allowed_in_tracking_accounts() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.house, &b.mortgage] {
        for (c1, c2) in [(json!(null), json!(null)), (json!(b.groceries), json!(b.household))] {
            let body = json!({"account_id": account.id, "date": "2026-10-06", "amount": -3,
                              "subtransactions": [{"amount": -1, "category_id": c1}, {"amount": -2, "category_id": c2}]});
            assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST split in {account:?}"));
        }
        let body = json!({"account_id": account.id, "date": "2026-10-06", "amount": -3,
                          "subtransactions": [{"amount": -3}]});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST split without categories in {account:?}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_10_2_patch_cannot_split_a_tracking_transaction() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-06", "amount": -3}));
    let res = server.patch_transaction(
        &b.plan,
        &id_of(&txn),
        &json!({"subtransactions": [{"amount": -1, "category_id": null}, {"amount": -2, "category_id": null}]}),
    );
    assert_error(&res, 400, "PATCH split onto a tracking transaction");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
}

// ---- 10.3 ----

#[test]
fn spec_10_3_subtransactions_keep_their_order() {
    let server = Server::start();
    let b = Budget::new(&server);
    let given = vec![
        json!({"amount": -30, "category_id": b.household, "memo": "c"}),
        json!({"amount": -10, "category_id": b.groceries, "memo": null}),
        json!({"amount": -20, "category_id": "ready_to_assign", "memo": "a"}),
        json!({"amount": -10, "category_id": b.groceries, "memo": "b"}),
    ];
    let body = json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -70, "subtransactions": given});
    let txn = server.create_transaction(&b.plan, &body);
    let want = Value::Array(given);
    assert_matches(&txn["subtransactions"], &want, "POST");
    assert_matches(&server.transaction(&b.plan, &id_of(&txn))["subtransactions"], &want, "GET");
    let listed = server.transactions(&b.plan);
    assert_matches(&listed[0]["subtransactions"], &want, "list");
}

#[test]
fn spec_10_3_non_split_has_empty_subtransactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1));
    assert_eq!(txn["subtransactions"], json!([]));
    let tracking = server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-06", "amount": 1}));
    assert_eq!(tracking["subtransactions"], json!([]));
    for t in server.transactions(&b.plan) {
        assert_eq!(t["subtransactions"], json!([]), "{t}");
    }
}

// ---- 10.4 ----

#[test]
fn spec_10_4_patch_replaces_all_subtransactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    let new = json!([{"amount": -10000, "category_id": "ready_to_assign"},
                     {"amount": -50000, "category_id": b.household, "memo": "x"},
                     {"amount": -30000, "category_id": b.groceries}]);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"subtransactions": new}));
    let want = json!([{"amount": -10000, "category_id": "ready_to_assign", "memo": null},
                      {"amount": -50000, "category_id": b.household, "memo": "x"},
                      {"amount": -30000, "category_id": b.groceries, "memo": null}]);
    assert_matches(&got["subtransactions"], &want, "PATCH");
    assert_matches(&server.transaction(&b.plan, &id_of(&txn))["subtransactions"], &want, "GET");
}

#[test]
fn spec_10_4_empty_array_makes_an_ordinary_transaction() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"subtransactions": [], "category_id": b.household}));
    assert_matches(&got, &json!({"category_id": b.household, "subtransactions": [], "amount": -90000}), "PATCH");
    assert_matches(
        &server.transaction(&b.plan, &id_of(&txn)),
        &json!({"category_id": b.household, "subtransactions": []}),
        "GET",
    );
}

#[test]
fn spec_10_4_empty_array_needs_a_category_on_budget() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    for body in [json!({"subtransactions": []}), json!({"subtransactions": [], "category_id": null})] {
        let res = server.patch_transaction(&b.plan, &id_of(&txn), &body);
        assert_error(&res, 400, &format!("PATCH {body}"));
    }
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
}

#[test]
fn spec_10_4_changing_amount_needs_matching_subtransactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"amount": -100000}));
    assert_error(&res, 400, "PATCH a split's amount alone");
    let res = server.patch_transaction(
        &b.plan,
        &id_of(&txn),
        &json!({"amount": -100000, "subtransactions": [{"amount": -60000, "category_id": b.groceries},
                                                        {"amount": -30000, "category_id": b.household}]}),
    );
    assert_error(&res, 400, "PATCH a split's amount with subtransactions that don't match");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&txn),
        &json!({"amount": -100000, "subtransactions": [{"amount": -70000, "category_id": b.groceries},
                                                        {"amount": -30000, "category_id": b.household}]}),
    );
    assert_eq!(got["amount"], -100000);
}

#[test]
fn spec_10_4_new_subtransactions_must_match_the_amount() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    let res = server.patch_transaction(
        &b.plan,
        &id_of(&txn),
        &json!({"subtransactions": [{"amount": -60000, "category_id": b.groceries}]}),
    );
    assert_error(&res, 400, "PATCH subtransactions that don't add up to the amount");
    let plain = server.create_transaction(&b.plan, &b.spend(-1000));
    let res = server.patch_transaction(
        &b.plan,
        &id_of(&plain),
        &json!({"subtransactions": [{"amount": -600, "category_id": b.groceries}, {"amount": -300, "category_id": b.household}]}),
    );
    assert_error(&res, 400, "PATCH an ordinary transaction into a split that doesn't add up");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&plain))), spec_fields(&plain));
}

#[test]
fn spec_10_4_other_patches_keep_the_subtransactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&txn),
        &json!({"memo": "big shop", "date": "2026-10-07", "cleared": "cleared", "payee_name": "Megastore"}),
    );
    assert_eq!(got["subtransactions"], txn["subtransactions"]);
    assert_eq!(got["category_id"], Value::Null);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"category_id": null}));
    assert_eq!(got["subtransactions"], txn["subtransactions"]);
}

#[test]
fn spec_10_4_split_cannot_take_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    for category in [json!(b.groceries), json!("ready_to_assign")] {
        let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"category_id": category}));
        assert_error(&res, 400, &format!("PATCH split category {category}"));
    }
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
}

#[test]
fn spec_10_4_replacement_subtransactions_are_checked() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &split_body(&b));
    for bad in [
        json!([{"amount": -90000, "category_id": b.visa_payment}]),
        json!([{"amount": -90000, "category_id": null}]),
        json!([{"amount": -90000}]),
        json!([{"category_id": b.groceries}]),
        json!([{"amount": -90000, "category_id": b.groceries, "memo": 5}]),
        json!(null),
        json!({}),
    ] {
        let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"subtransactions": bad}));
        assert_error(&res, 400, &format!("PATCH subtransactions {bad}"));
    }
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
}
