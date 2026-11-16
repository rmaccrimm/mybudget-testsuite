//! SPEC section 9: transfers.

use mybudget_testsuite::{
    Acct, Budget, Server, Value, assert_error, assert_matches, assert_status, id_of, ids_of, json, spec_fields,
    str_field,
};

/// Creates a transfer of `amount` from `from` to `to` dated 2026-10-05, with `extra` fields;
/// returns (this half, other half) as `GET` shows them.
fn transfer(server: &Server, b: &Budget, from: &Acct, to: &Acct, amount: i64, extra: Value) -> (Value, Value) {
    let mut body = json!({"account_id": from.id, "date": "2026-10-05", "amount": amount, "payee_id": to.payee});
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let txn = server.create_transaction(&b.plan, &body);
    let other = str_field(&txn, "transfer_transaction_id");
    (server.transaction(&b.plan, &id_of(&txn)), server.transaction(&b.plan, &other))
}

fn gone(server: &Server, b: &Budget, id: &str, what: &str) {
    assert_error(&server.get_transaction(&b.plan, id), 404, what);
}

// ---- 9.1 ----

#[test]
fn spec_9_1_creates_transfer_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "starting_balance": 2500000, "starting_date": "2026-10-01"}),
    );
    let savings = server.add_account(&plan, "Savings", "savings", 0);
    let (checking_id, checking_payee) = (id_of(&checking), str_field(&checking, "transfer_payee_id"));
    let (savings_id, savings_payee) = (id_of(&savings), str_field(&savings, "transfer_payee_id"));
    let res = server.post_transaction(
        &plan,
        &json!({"account_id": checking_id, "date": "2026-10-05", "amount": -500000, "payee_id": savings_payee}),
    );
    assert_status(&res, 201, "POST 9.1's example");
    let out = res.json();
    let out_id = id_of(&out);
    let in_id = str_field(&out, "transfer_transaction_id");
    assert_ne!(in_id, out_id);
    assert_matches(
        &out,
        &json!({"id": out_id, "account_id": checking_id, "date": "2026-10-05", "amount": -500000,
                "payee_id": savings_payee, "category_id": null, "memo": null,
                "cleared": "uncleared", "approved": true, "flag_color": null,
                "transfer_account_id": savings_id, "transfer_transaction_id": in_id, "subtransactions": []}),
        "9.1's POST",
    );
    let res = server.get_transaction(&plan, &in_id);
    assert_status(&res, 200, "GET the other half");
    assert_matches(
        &res.json(),
        &json!({"id": in_id, "account_id": savings_id, "date": "2026-10-05", "amount": 500000,
                "payee_id": checking_payee, "category_id": null, "memo": null,
                "cleared": "uncleared", "approved": true, "flag_color": null,
                "transfer_account_id": checking_id, "transfer_transaction_id": out_id, "subtransactions": []}),
        "9.1's GET",
    );
}

#[test]
fn spec_9_1_other_half_is_created_after_it() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.savings, &b.checking, -500, json!({}));
    let before = server.create_transaction(&b.plan, &json!({"account_id": b.checking.id, "date": "2026-10-05",
                                                             "amount": -1, "category_id": b.groceries}));
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&out), id_of(&inn), id_of(&before)]);
}

#[test]
fn spec_9_1_other_half_copies_date_amount_and_memo_only() {
    let server = Server::start();
    let b = Budget::new(&server);
    let extra = json!({"date": "2027-03-04", "memo": "move it", "cleared": "reconciled", "approved": false, "flag_color": "red"});
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, 12345, extra);
    assert_matches(
        &out,
        &json!({"date": "2027-03-04", "amount": 12345, "memo": "move it", "cleared": "reconciled",
                "approved": false, "flag_color": "red"}),
        "this half",
    );
    assert_matches(
        &inn,
        &json!({"account_id": b.savings.id, "date": "2027-03-04", "amount": -12345, "payee_id": b.checking.payee,
                "memo": "move it", "cleared": "uncleared", "approved": true, "flag_color": null}),
        "other half",
    );
}

#[test]
fn spec_9_1_other_half_cleared_defaults_for_its_account() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.wallet, -2000, json!({}));
    assert_eq!(out["cleared"], "uncleared");
    assert_eq!(inn["cleared"], "cleared", "a cash account's half defaults to cleared");
    let (out, inn) = transfer(&server, &b, &b.wallet, &b.savings, -2000, json!({}));
    assert_eq!(out["cleared"], "cleared");
    assert_eq!(inn["cleared"], "uncleared");
    let (_, inn) = transfer(&server, &b, &b.checking, &b.wallet, -2000, json!({"cleared": "uncleared"}));
    assert_eq!(inn["cleared"], "cleared");
}

#[test]
fn spec_9_1_halves_name_each_other() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (from, to) in [(&b.checking, &b.visa), (&b.visa, &b.loc), (&b.house, &b.mortgage), (&b.wallet, &b.savings)] {
        let (out, inn) = transfer(&server, &b, from, to, -300, json!({}));
        assert_matches(
            &out,
            &json!({"account_id": from.id, "payee_id": to.payee, "transfer_account_id": to.id,
                    "transfer_transaction_id": inn["id"], "subtransactions": []}),
            "this half",
        );
        assert_matches(
            &inn,
            &json!({"account_id": to.id, "amount": 300, "payee_id": from.payee, "transfer_account_id": from.id,
                    "transfer_transaction_id": out["id"], "subtransactions": []}),
            "other half",
        );
    }
}

#[test]
fn spec_9_1_transfer_changes_both_balances() {
    let server = Server::start();
    let b = Budget::new(&server);
    transfer(&server, &b, &b.checking, &b.wallet, -7000, json!({"cleared": "cleared"}));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (-7000, -7000, 0));
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (7000, 7000, 0));
}

#[test]
fn spec_9_1_transfer_may_go_into_a_closed_account() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.patch_ok(&b.path(&format!("/accounts/{}", b.savings.id)), &json!({"closed": true}));
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    assert_eq!(out["transfer_account_id"], json!(b.savings.id));
    assert_eq!(inn["account_id"], json!(b.savings.id));
}

#[test]
fn spec_9_1_non_transfers_have_null_transfer_fields() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    assert_matches(&txn, &json!({"transfer_account_id": null, "transfer_transaction_id": null}), "POST");
    assert_matches(
        &server.transaction(&b.plan, &id_of(&txn)),
        &json!({"transfer_account_id": null, "transfer_transaction_id": null}),
        "GET",
    );
}

#[test]
fn spec_9_1_transfer_payee_of_renamed_account_still_transfers() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.patch_ok(&b.path(&format!("/accounts/{}", b.savings.id)), &json!({"name": "Rainy Day"}));
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    assert_eq!(out["payee_id"], json!(b.savings.payee));
    assert_eq!(inn["payee_id"], json!(b.checking.payee));
}

// ---- 9.2 ----

#[test]
fn spec_9_2_transfer_to_the_same_account_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.checking, &b.visa, &b.house] {
        let body = json!({"account_id": account.id, "date": "2026-10-05", "amount": -100, "payee_id": account.payee});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST self-transfer in {account:?}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_9_2_patch_into_a_self_transfer_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"payee_id": b.checking.payee}));
    assert_error(&res, 400, "PATCH payee to own transfer payee");
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let res = server.patch_transaction(&b.plan, &id_of(&out), &json!({"payee_id": b.checking.payee}));
    assert_error(&res, 400, "PATCH transfer payee to own transfer payee");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&inn))), spec_fields(&inn));
}

#[test]
fn spec_9_2_transfer_with_subtransactions_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -100, "payee_id": b.savings.payee,
                      "subtransactions": [{"amount": -60, "category_id": b.groceries}, {"amount": -40, "category_id": b.household}]});
    assert_error(&server.post_transaction(&b.plan, &body), 400, "POST transfer with subtransactions");
    let body = json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -100, "payee_id": b.house.payee,
                      "subtransactions": [{"amount": -100, "category_id": b.groceries}]});
    assert_error(&server.post_transaction(&b.plan, &body), 400, "POST transfer to tracking with subtransactions");
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_9_2_transfer_with_empty_subtransactions_is_allowed() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -100, "payee_id": b.savings.payee,
                      "subtransactions": []});
    let txn = server.create_transaction(&b.plan, &body);
    assert_eq!(txn["transfer_account_id"], json!(b.savings.id));
}

#[test]
fn spec_9_2_patch_adding_subtransactions_to_a_transfer_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let subs = json!([{"amount": -60, "category_id": b.groceries}, {"amount": -40, "category_id": b.household}]);
    let res = server.patch_transaction(&b.plan, &id_of(&out), &json!({"subtransactions": subs}));
    assert_error(&res, 400, "PATCH subtransactions onto a transfer");
    let subs = json!([{"amount": 60, "category_id": b.groceries}, {"amount": 40, "category_id": b.household}]);
    let res = server.patch_transaction(&b.plan, &id_of(&inn), &json!({"subtransactions": subs}));
    assert_error(&res, 400, "PATCH subtransactions onto the other half");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&out))), spec_fields(&out));
}

#[test]
fn spec_9_2_patch_split_payee_to_transfer_payee_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let split = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -100, "subtransactions": [
            {"amount": -60, "category_id": b.groceries}, {"amount": -40, "category_id": b.household}]}),
    );
    let res = server.patch_transaction(&b.plan, &id_of(&split), &json!({"payee_id": b.savings.payee}));
    assert_error(&res, 400, "PATCH split's payee to a transfer payee");
    assert_eq!(server.transactions(&b.plan).len(), 1);
}

// ---- 9.3 ----

#[test]
fn spec_9_3_on_budget_transfers_have_no_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (from, to) in [(&b.checking, &b.savings), (&b.checking, &b.visa), (&b.visa, &b.loc), (&b.wallet, &b.checking)] {
        let (out, inn) = transfer(&server, &b, from, to, -100, json!({}));
        assert_eq!(out["category_id"], Value::Null);
        assert_eq!(inn["category_id"], Value::Null);
        let (out, _) = transfer(&server, &b, from, to, -100, json!({"category_id": null}));
        assert_eq!(out["category_id"], Value::Null);
    }
}

#[test]
fn spec_9_3_on_budget_transfer_with_category_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (from, to) in [(&b.checking, &b.savings), (&b.visa, &b.checking), (&b.house, &b.mortgage)] {
        for category in [json!(b.groceries), json!("ready_to_assign"), json!(b.visa_payment)] {
            let body = json!({"account_id": from.id, "date": "2026-10-05", "amount": -100, "payee_id": to.payee,
                              "category_id": category});
            assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST {from:?} -> {to:?} with {category}"));
        }
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_9_3_tracking_transfers_have_no_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.house, &b.mortgage, -100, json!({}));
    assert_eq!(out["category_id"], Value::Null);
    assert_eq!(inn["category_id"], Value::Null);
}

#[test]
fn spec_9_3_on_budget_half_of_transfer_to_tracking_needs_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (from, to) in [(&b.checking, &b.house), (&b.house, &b.checking), (&b.visa, &b.mortgage), (&b.mortgage, &b.wallet)] {
        let body = json!({"account_id": from.id, "date": "2026-10-05", "amount": -100, "payee_id": to.payee});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST {from:?} -> {to:?} without category"));
        let body = json!({"account_id": from.id, "date": "2026-10-05", "amount": -100, "payee_id": to.payee, "category_id": null});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST {from:?} -> {to:?} with null category"));
        let body = json!({"account_id": from.id, "date": "2026-10-05", "amount": -100, "payee_id": to.payee,
                          "category_id": b.visa_payment});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST {from:?} -> {to:?} with payment category"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_9_3_category_goes_on_the_on_budget_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    assert_eq!(out["category_id"], json!(b.groceries));
    assert_eq!(inn["category_id"], Value::Null);
    let (out, inn) = transfer(&server, &b, &b.house, &b.checking, -100, json!({"category_id": b.household}));
    assert_eq!(out["category_id"], Value::Null, "the tracking half has no category");
    assert_eq!(inn["category_id"], json!(b.household), "the on-budget half gets the request's category");
    let (out, inn) = transfer(&server, &b, &b.mortgage, &b.visa, 5000, json!({"category_id": "ready_to_assign"}));
    assert_eq!(out["category_id"], Value::Null);
    assert_eq!(inn["category_id"], "ready_to_assign");
}

#[test]
fn spec_9_3_patch_category_on_either_half_sets_the_on_budget_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"category_id": b.household}));
    assert_eq!(got["category_id"], Value::Null);
    assert_eq!(server.transaction(&b.plan, &id_of(&out))["category_id"], json!(b.household));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"category_id": "ready_to_assign"}));
    assert_eq!(got["category_id"], "ready_to_assign");
    assert_eq!(server.transaction(&b.plan, &id_of(&inn))["category_id"], Value::Null);
}

#[test]
fn spec_9_3_patch_cannot_remove_the_on_budget_halfs_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    for id in [id_of(&out), id_of(&inn)] {
        for bad in [json!(null), json!(b.visa_payment)] {
            let res = server.patch_transaction(&b.plan, &id, &json!({"category_id": bad}));
            assert_error(&res, 400, &format!("PATCH category {bad} on a half"));
        }
    }
    assert_eq!(server.transaction(&b.plan, &id_of(&out))["category_id"], json!(b.groceries));
}

#[test]
fn spec_9_3_patch_category_on_on_budget_transfer_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    for id in [id_of(&out), id_of(&inn)] {
        let res = server.patch_transaction(&b.plan, &id, &json!({"category_id": b.groceries}));
        assert_error(&res, 400, "PATCH category onto an on-budget transfer");
    }
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"category_id": null}));
    assert_eq!(got["category_id"], Value::Null);
}

// ---- 9.4 ----

#[test]
fn spec_9_4_date_and_amount_follow_on_the_other_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"date": "2026-11-11"}));
    assert_eq!(server.transaction(&b.plan, &id_of(&inn))["date"], "2026-11-11");
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"amount": 2500}));
    assert_eq!(server.transaction(&b.plan, &id_of(&out))["amount"], -2500);
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"amount": -40, "date": "2025-01-02"}));
    assert_matches(&server.transaction(&b.plan, &id_of(&out)), &json!({"amount": 40, "date": "2025-01-02"}), "OUT");
    assert_matches(&server.transaction(&b.plan, &id_of(&inn)), &json!({"amount": -40, "date": "2025-01-02"}), "IN");
    assert_eq!(server.balances(&b.plan, &b.checking.id).0, 40);
    assert_eq!(server.balances(&b.plan, &b.savings.id).0, -40);
}

#[test]
fn spec_9_4_date_and_amount_follow_across_tracking_transfers() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"amount": 0, "date": "2026-12-01"}));
    assert_matches(&server.transaction(&b.plan, &id_of(&out)), &json!({"amount": 0, "date": "2026-12-01"}), "OUT");
}

#[test]
fn spec_9_4_other_fields_belong_to_their_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({"memo": "shared"}));
    let change = json!({"memo": "only out", "cleared": "reconciled", "approved": false, "flag_color": "blue"});
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &change);
    assert_matches(&got, &change, "the patched half");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&inn))), spec_fields(&inn), "the other half changed");
    let change = json!({"memo": null, "cleared": "cleared", "flag_color": "red"});
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &change);
    assert_matches(
        &server.transaction(&b.plan, &id_of(&out)),
        &json!({"memo": "only out", "cleared": "reconciled", "approved": false, "flag_color": "blue"}),
        "OUT after patching IN",
    );
}

// ---- 9.5 ----

#[test]
fn spec_9_5_ordinary_payee_deletes_the_other_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": shop, "category_id": b.groceries}));
    assert_matches(
        &got,
        &json!({"payee_id": shop, "category_id": b.groceries, "transfer_account_id": null, "transfer_transaction_id": null}),
        "PATCH",
    );
    gone(&server, &b, &id_of(&inn), "GET the deleted other half");
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&out)]);
    assert_eq!(server.balances(&b.plan, &b.savings.id), (0, 0, 0));
}

#[test]
fn spec_9_5_null_payee_deletes_the_other_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.savings, &b.checking, -100, json!({}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"payee_id": null, "category_id": "ready_to_assign"}));
    assert_matches(&got, &json!({"payee_id": null, "transfer_account_id": null, "transfer_transaction_id": null}), "PATCH");
    gone(&server, &b, &id_of(&out), "GET the deleted other half");
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&inn)]);
}

#[test]
fn spec_9_5_payee_name_never_keeps_a_transfer() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&out),
        &json!({"payee_name": "Transfer : Savings", "category_id": b.groceries}),
    );
    assert_ne!(got["payee_id"], json!(b.savings.payee));
    assert_eq!(got["transfer_account_id"], Value::Null);
    gone(&server, &b, &id_of(&inn), "GET the deleted other half");
}

#[test]
fn spec_9_5_on_budget_half_left_without_transfer_needs_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    for change in [json!({"payee_id": shop}), json!({"payee_id": null}), json!({"payee_name": "Shop"})] {
        let res = server.patch_transaction(&b.plan, &id_of(&out), &change);
        assert_error(&res, 400, &format!("PATCH {change} without category"));
    }
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&out))), spec_fields(&out));
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&inn))), spec_fields(&inn));
}

#[test]
fn spec_9_5_tracking_half_left_without_transfer_needs_no_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"payee_id": null}));
    assert_matches(&got, &json!({"category_id": null, "transfer_account_id": null}), "tracking half");
    gone(&server, &b, &id_of(&out), "GET the deleted on-budget half");
}

#[test]
fn spec_9_5_on_budget_half_of_tracking_transfer_keeps_its_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_name": "Shop"}));
    assert_matches(&got, &json!({"category_id": b.groceries, "transfer_account_id": null}), "on-budget half");
    gone(&server, &b, &id_of(&inn), "GET the deleted tracking half");
}

#[test]
fn spec_9_5_another_transfer_payee_moves_the_other_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({"memo": "m"}));
    server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"memo": "new memo", "flag_color": "red"}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.wallet.payee}));
    let new_id = str_field(&got, "transfer_transaction_id");
    assert_ne!(new_id, id_of(&inn));
    assert_eq!(got["transfer_account_id"], json!(b.wallet.id));
    gone(&server, &b, &id_of(&inn), "GET the old other half");
    let new_half = server.transaction(&b.plan, &new_id);
    assert_matches(
        &new_half,
        &json!({"account_id": b.wallet.id, "date": "2026-10-05", "amount": 100, "payee_id": b.checking.payee,
                "category_id": null, "memo": "new memo", "cleared": "cleared", "approved": true, "flag_color": null,
                "transfer_account_id": b.checking.id, "transfer_transaction_id": id_of(&out), "subtransactions": []}),
        "the new other half",
    );
    assert_eq!(server.balances(&b.plan, &b.savings.id), (0, 0, 0));
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (100, 100, 0));
}

#[test]
fn spec_9_5_moving_to_a_tracking_account_needs_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let res = server.patch_transaction(&b.plan, &id_of(&out), &json!({"payee_id": b.house.payee}));
    assert_error(&res, 400, "PATCH to a tracking account's transfer payee without category");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&inn))), spec_fields(&inn));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.house.payee, "category_id": b.groceries}));
    assert_matches(&got, &json!({"category_id": b.groceries, "transfer_account_id": b.house.id}), "PATCH");
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&new_half, &json!({"account_id": b.house.id, "amount": 100, "category_id": null}), "new half");
    gone(&server, &b, &id_of(&inn), "GET the old other half");
}

#[test]
fn spec_9_5_moving_from_a_tracking_account_clears_the_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, _) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.savings.payee}));
    assert_matches(&got, &json!({"category_id": null, "transfer_account_id": b.savings.id}), "PATCH");
    let res = server.patch_transaction(&b.plan, &id_of(&out), &json!({"payee_id": b.wallet.payee, "category_id": b.groceries}));
    assert_error(&res, 400, "PATCH to an on-budget transfer with a category");
}

#[test]
fn spec_9_5_transfer_payee_makes_an_ordinary_transaction_a_transfer() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-07", "amount": -900, "category_id": b.groceries,
                "memo": "rent share", "cleared": "cleared", "approved": false, "flag_color": "green"}),
    );
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_id": b.wallet.payee}));
    let other = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(
        &other,
        &json!({"account_id": b.wallet.id, "date": "2026-10-07", "amount": 900, "payee_id": b.checking.payee,
                "category_id": null, "memo": "rent share", "cleared": "cleared", "approved": true, "flag_color": null,
                "transfer_account_id": b.checking.id, "transfer_transaction_id": id_of(&txn), "subtransactions": []}),
        "the new other half",
    );
    assert_matches(
        &got,
        &json!({"cleared": "cleared", "approved": false, "flag_color": "green", "transfer_account_id": b.wallet.id}),
        "the patched half",
    );
}

#[test]
fn spec_9_5_ordinary_tracking_transaction_becomes_a_transfer() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-07", "amount": 900}));
    let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"payee_id": b.checking.payee}));
    assert_error(&res, 400, "PATCH tracking transaction into a transfer to Checking without category");
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_id": b.checking.payee, "category_id": b.groceries}));
    assert_eq!(got["category_id"], Value::Null);
    let other = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&other, &json!({"account_id": b.checking.id, "amount": -900, "category_id": b.groceries}), "on-budget half");
}

#[test]
fn spec_9_5_same_transfer_payee_keeps_the_other_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"memo": "kept", "flag_color": "red", "cleared": "cleared"}));
    let inn = server.transaction(&b.plan, &id_of(&inn));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.savings.payee}));
    assert_eq!(got["transfer_transaction_id"], inn["id"]);
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&inn))), spec_fields(&inn));
}

// ---- 9.6 ----

#[test]
fn spec_9_6_deleting_either_half_deletes_both() {
    let server = Server::start();
    let b = Budget::new(&server);
    let keep = server.create_transaction(&b.plan, &b.spend(-1));
    for first in [true, false] {
        let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
        let (del, other) = if first { (&out, &inn) } else { (&inn, &out) };
        assert_status(&server.delete_transaction(&b.plan, &id_of(del)), 204, "DELETE a half");
        gone(&server, &b, &id_of(other), "GET the other half");
        assert_error(&server.delete_transaction(&b.plan, &id_of(other)), 404, "DELETE the other half");
        assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&keep)]);
        assert_eq!(server.balances(&b.plan, &b.savings.id), (0, 0, 0));
    }
}

#[test]
fn spec_9_6_deleting_a_tracking_transfer_deletes_both() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.house, &b.checking, -100, json!({"category_id": b.groceries}));
    assert_status(&server.delete_transaction(&b.plan, &id_of(&out)), 204, "DELETE tracking half");
    gone(&server, &b, &id_of(&inn), "GET the on-budget half");
    assert!(server.transactions(&b.plan).is_empty());
}
