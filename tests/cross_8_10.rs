//! SPEC sections 5 to 10 where transfers, splits, filters and balances meet: splits that
//! become transfers and back, transfers moved from their other half, and lists and balances
//! over both halves.

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

/// A split of -100 in Checking: -60 Groceries, -40 Household.
fn split(server: &Server, b: &Budget) -> Value {
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -100, "payee_name": "Megastore",
                "subtransactions": [{"amount": -60, "category_id": b.groceries},
                                    {"amount": -40, "category_id": b.household}]}),
    )
}

fn gone(server: &Server, b: &Budget, id: &str, what: &str) {
    assert_error(&server.get_transaction(&b.plan, id), 404, what);
}

fn payee_names(server: &Server, plan: &str) -> Vec<String> {
    server.payees(plan).iter().map(|p| str_field(p, "name")).collect()
}

// ---- 10.4 with 9.5: a split becoming a transfer in one PATCH ----

#[test]
fn spec_10_4_split_becomes_a_transfer_with_an_empty_array() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = split(&server, &b);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_id": b.savings.payee, "subtransactions": []}));
    assert_matches(
        &got,
        &json!({"amount": -100, "payee_id": b.savings.payee, "category_id": null, "subtransactions": [],
                "transfer_account_id": b.savings.id}),
        "the former split",
    );
    let other = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(
        &other,
        &json!({"account_id": b.savings.id, "date": "2026-10-06", "amount": 100, "payee_id": b.checking.payee,
                "category_id": null, "subtransactions": [], "transfer_account_id": b.checking.id,
                "transfer_transaction_id": id_of(&txn)}),
        "the new other half",
    );
    assert_eq!(server.balances(&b.plan, &b.savings.id).0, 100);
}

#[test]
fn spec_10_4_split_becoming_a_tracking_transfer_needs_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = split(&server, &b);
    let before = server.plan_snapshot(&b.plan);
    let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"payee_id": b.house.payee, "subtransactions": []}));
    assert_error(&res, 400, "PATCH split into a transfer to a tracking account without category_id");
    assert_eq!(server.plan_snapshot(&b.plan), before, "the failed PATCH changed something");
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&txn),
        &json!({"payee_id": b.house.payee, "subtransactions": [], "category_id": b.household}),
    );
    assert_matches(
        &got,
        &json!({"category_id": b.household, "subtransactions": [], "transfer_account_id": b.house.id}),
        "the on-budget half",
    );
    let other = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&other, &json!({"account_id": b.house.id, "amount": 100, "category_id": null}), "the tracking half");
}

// ---- 9.5 with 10.4: a transfer becoming a split ----

#[test]
fn spec_9_5_transfer_becomes_a_split() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let subs = json!([{"amount": -30, "category_id": b.groceries}, {"amount": -70, "category_id": "ready_to_assign"}]);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_name": "Megastore", "subtransactions": subs}));
    assert_matches(
        &got,
        &json!({"category_id": null, "transfer_account_id": null, "transfer_transaction_id": null,
                "subtransactions": [{"amount": -30, "category_id": b.groceries, "memo": null},
                                    {"amount": -70, "category_id": "ready_to_assign", "memo": null}]}),
        "the former transfer",
    );
    gone(&server, &b, &id_of(&inn), "GET the deleted other half");
    assert_eq!(server.balances(&b.plan, &b.savings.id), (0, 0, 0));
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&out)]);
}

#[test]
fn spec_9_5_on_budget_half_of_a_tracking_transfer_becomes_a_split() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let subs = json!([{"amount": -60, "category_id": b.groceries}, {"amount": -40, "category_id": b.household}]);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": null, "subtransactions": subs}));
    assert_matches(
        &got,
        &json!({"payee_id": null, "category_id": null, "transfer_account_id": null, "transfer_transaction_id": null}),
        "the former on-budget half",
    );
    assert_eq!(got["subtransactions"].as_array().map(Vec::len), Some(2));
    gone(&server, &b, &id_of(&inn), "GET the deleted tracking half");
    assert_eq!(server.balances(&b.plan, &b.house.id), (0, 0, 0));
}

#[test]
fn spec_10_2_tracking_half_of_a_transfer_cannot_become_a_split() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (_, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let before = server.plan_snapshot(&b.plan);
    let subs = json!([{"amount": 60, "category_id": b.groceries}, {"amount": 40, "category_id": b.household}]);
    for body in [json!({"payee_id": null, "subtransactions": subs}), json!({"payee_name": "Builder", "subtransactions": subs})] {
        let res = server.patch_transaction(&b.plan, &id_of(&inn), &body);
        assert_error(&res, 400, &format!("PATCH tracking half {body}"));
        assert_eq!(server.plan_snapshot(&b.plan), before, "after PATCH {body}");
    }
}

// ---- 9.5: moving a transfer ----

#[test]
fn spec_9_5_other_half_moves_the_transfer_from_its_own_side() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({"memo": "m"}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"payee_id": b.wallet.payee}));
    let new_id = str_field(&got, "transfer_transaction_id");
    assert_ne!(new_id, id_of(&out));
    assert_matches(
        &got,
        &json!({"account_id": b.savings.id, "amount": 100, "payee_id": b.wallet.payee, "transfer_account_id": b.wallet.id}),
        "the patched Savings half",
    );
    gone(&server, &b, &id_of(&out), "GET the old Checking half");
    assert_matches(
        &server.transaction(&b.plan, &new_id),
        &json!({"account_id": b.wallet.id, "date": "2026-10-05", "amount": -100, "payee_id": b.savings.payee,
                "category_id": null, "memo": "m", "cleared": "cleared", "approved": true, "flag_color": null,
                "transfer_account_id": b.savings.id, "transfer_transaction_id": id_of(&inn), "subtransactions": []}),
        "the new Wallet half",
    );
    assert_eq!(server.balances(&b.plan, &b.checking.id), (0, 0, 0));
    assert_eq!(server.balances(&b.plan, &b.savings.id).0, 100);
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (-100, -100, 0));
}

#[test]
fn spec_9_5_moved_transfer_takes_the_new_date_and_amount() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, _) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&out),
        &json!({"payee_id": b.visa.payee, "date": "2026-12-24", "amount": -777, "memo": "pay card"}),
    );
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(
        &new_half,
        &json!({"account_id": b.visa.id, "date": "2026-12-24", "amount": 777, "memo": "pay card", "category_id": null}),
        "the new Visa half",
    );
    assert_eq!(server.balances(&b.plan, &b.visa.id).0, 777);
    assert_eq!(server.balances(&b.plan, &b.savings.id).0, 0);
}

#[test]
fn spec_9_5_moving_back_creates_a_new_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let moved = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.wallet.payee}));
    let back = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.savings.payee}));
    let back_id = str_field(&back, "transfer_transaction_id");
    assert_ne!(back_id, id_of(&inn), "a new half needs a new id (3.5)");
    gone(&server, &b, &str_field(&moved, "transfer_transaction_id"), "GET the Wallet half");
    gone(&server, &b, &id_of(&inn), "GET the first Savings half");
    assert_matches(
        &server.transaction(&b.plan, &back_id),
        &json!({"account_id": b.savings.id, "amount": 100, "transfer_transaction_id": id_of(&out)}),
        "the new Savings half",
    );
    assert_eq!(server.transactions(&b.plan).len(), 2);
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (0, 0, 0));
}

#[test]
fn spec_9_3_on_budget_half_moving_between_tracking_accounts_takes_a_new_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&out),
        &json!({"payee_id": b.mortgage.payee, "category_id": b.household}),
    );
    assert_matches(&got, &json!({"category_id": b.household, "transfer_account_id": b.mortgage.id}), "the on-budget half");
    gone(&server, &b, &id_of(&inn), "GET the old House half");
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&new_half, &json!({"account_id": b.mortgage.id, "category_id": null}), "the new Mortgage half");
}

#[test]
fn spec_3_11_failed_move_of_a_transfer_changes_nothing() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let before = server.plan_snapshot(&b.plan);
    for (id, body) in [
        (id_of(&out), json!({"payee_id": b.wallet.payee, "memo": "x".repeat(501)})),
        (id_of(&out), json!({"payee_id": b.wallet.payee, "flag_color": "pink"})),
        (id_of(&inn), json!({"payee_id": b.visa.payee, "category_id": b.groceries})),
        (id_of(&inn), json!({"payee_id": b.house.payee, "category_id": b.visa_payment})),
        (id_of(&out), json!({"payee_id": b.wallet.payee, "payee_name": " "})),
    ] {
        let res = server.patch_transaction(&b.plan, &id, &body);
        assert_error(&res, 400, &format!("PATCH {body}"));
        assert_eq!(server.plan_snapshot(&b.plan), before, "after PATCH {body}");
    }
}

#[test]
fn spec_9_6_deleting_a_moved_transfer_deletes_its_new_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, _) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.wallet.payee}));
    let new_id = str_field(&got, "transfer_transaction_id");
    assert_status(&server.delete_transaction(&b.plan, &new_id), 204, "DELETE the new half");
    gone(&server, &b, &id_of(&out), "GET the moved half");
    assert!(server.transactions(&b.plan).is_empty());
    assert_eq!(server.balances(&b.plan, &b.checking.id), (0, 0, 0));
}

// ---- 6.5 on transfers ----

#[test]
fn spec_6_5_transfer_payee_id_leaves_payee_name_unused() {
    let server = Server::start();
    let b = Budget::new(&server);
    let names_before = payee_names(&server, &b.plan);
    let txn = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -100, "payee_id": b.savings.payee,
                "payee_name": "Ghost"}),
    );
    assert_matches(&txn, &json!({"payee_id": b.savings.payee, "transfer_account_id": b.savings.id}), "POST");
    let plain = server.create_transaction(&b.plan, &b.spend(-5));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&plain),
        &json!({"payee_id": b.wallet.payee, "payee_name": "Ghost Two"}),
    );
    assert_matches(&got, &json!({"payee_id": b.wallet.payee, "transfer_account_id": b.wallet.id, "category_id": null}), "PATCH");
    assert_eq!(payee_names(&server, &b.plan), names_before, "an unused payee_name created a payee");
}

// ---- 8.5 over transfers ----

#[test]
fn spec_8_5_category_filter_finds_the_on_budget_half_of_a_tracking_transfer() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, _) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let (_, visa_half) = transfer(&server, &b, &b.mortgage, &b.visa, 5000, json!({"category_id": "ready_to_assign"}));
    let by = |category: &str| ids_of(&server.transactions_where(&b.plan, &format!("category_id={category}")));
    assert_eq!(by(&b.groceries), vec![id_of(&out)]);
    assert_eq!(by("ready_to_assign"), vec![id_of(&visa_half)]);
    assert!(by(&b.household).is_empty());
}

#[test]
fn spec_8_5_approved_filter_sees_each_half_on_its_own() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({"approved": false}));
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "approved=false")), vec![id_of(&out)]);
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "approved=true")), vec![id_of(&inn)]);
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"approved": false}));
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "approved=false")), vec![id_of(&out), id_of(&inn)]);
    assert!(server.transactions_where(&b.plan, "approved=true").is_empty());
}

#[test]
fn spec_8_5_date_change_on_one_half_reorders_both() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let middle = server.create_transaction(&b.plan, &json!({"account_id": b.wallet.id, "date": "2026-10-06", "amount": -5,
                                                             "category_id": b.groceries}));
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"date": "2026-10-07"}));
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&middle), id_of(&out), id_of(&inn)]);
    assert_eq!(
        ids_of(&server.transactions_where(&b.plan, "since_date=2026-10-07")),
        vec![id_of(&out), id_of(&inn)],
    );
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "until_date=2026-10-06")), vec![id_of(&middle)]);
}

// ---- 5.4, 5.8, 5.9 over transfers ----

#[test]
fn spec_5_4_transfer_halves_clear_on_their_own() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.wallet, -5000, json!({}));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (-5000, 0, -5000));
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (5000, 5000, 0));
    server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"cleared": "uncleared"}));
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (5000, 0, 5000));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (-5000, 0, -5000));
    server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"cleared": "reconciled"}));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (-5000, -5000, 0));
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (5000, 0, 5000));
}

#[test]
fn spec_5_4_split_becoming_a_transfer_moves_the_balance() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = split(&server, &b);
    server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_id": b.visa.payee, "subtransactions": []}));
    assert_eq!(server.balances(&b.plan, &b.checking.id).0, -100);
    assert_eq!(server.balances(&b.plan, &b.visa.id), (100, 0, 100));
}

#[test]
fn spec_5_8_account_emptied_by_a_transfer_can_close() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = Acct::from(&server.add_account(&plan, "Checking", "checking", 1000));
    let savings = Acct::from(&server.add_account(&plan, "Savings", "savings", 0));
    server.create_transaction(
        &plan,
        &json!({"account_id": checking.id, "date": "2026-10-05", "amount": -1000, "payee_id": savings.payee}),
    );
    let got = server.patch_ok(&format!("/plans/{plan}/accounts/{}", checking.id), &json!({"closed": true}));
    assert_eq!(got["closed"], true);
    let res = server.patch(&format!("/plans/{plan}/accounts/{}", savings.id), &json!({"closed": true}));
    assert_error(&res, 409, "close Savings holding the transfer");
}

#[test]
fn spec_5_9_accounts_can_be_deleted_once_their_transfer_is_deleted() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, _) = transfer(&server, &b, &b.visa, &b.loc, -100, json!({}));
    for account in [&b.visa, &b.loc] {
        let res = server.delete(&b.path(&format!("/accounts/{}", account.id)));
        assert_error(&res, 409, &format!("DELETE {account:?} with a transfer half"));
    }
    assert_status(&server.delete_transaction(&b.plan, &id_of(&out)), 204, "DELETE the transfer");
    for account in [&b.visa, &b.loc] {
        let res = server.delete(&b.path(&format!("/accounts/{}", account.id)));
        assert_status(&res, 204, &format!("DELETE {account:?} after its transfer is gone"));
        assert_error(&server.get(&b.path(&format!("/payees/{}", account.payee))), 404, "GET the deleted transfer payee");
    }
    let payees: Vec<Value> = server.payees(&b.plan).iter().map(spec_fields).collect();
    assert!(
        payees.iter().all(|p| p["id"] != json!(b.visa.payee) && p["id"] != json!(b.loc.payee)),
        "transfer payees of deleted accounts are still listed: {payees:?}"
    );
}
