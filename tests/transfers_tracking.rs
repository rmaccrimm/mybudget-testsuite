//! SPEC 9.3 and 9.5: PATCHes to transfers between on-budget and tracking accounts, and
//! especially to their tracking halves.

use mybudget_testsuite::{
    Acct, Budget, Server, Value, assert_error, assert_matches, id_of, ids_of, json, spec_fields, str_field,
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

// ---- 9.3: a PATCH to the tracking half that leaves category_id out ----

#[test]
fn spec_9_3_tracking_half_patch_without_category_keeps_the_on_budget_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    for change in [
        json!({"memo": "tracking memo"}),
        json!({"amount": 700}),
        json!({"date": "2026-11-02", "cleared": "cleared", "flag_color": "red", "approved": false}),
        json!({"payee_id": b.checking.payee}),
    ] {
        let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &change);
        assert_eq!(got["category_id"], Value::Null, "the tracking half after {change}");
        assert_eq!(got["transfer_transaction_id"], json!(id_of(&out)), "the transfer after {change}");
        let on_budget = server.transaction(&b.plan, &id_of(&out));
        assert_eq!(on_budget["category_id"], json!(b.groceries), "the on-budget half after {change} on the tracking half");
    }
}

#[test]
fn spec_9_3_tracking_half_moving_to_another_on_budget_account_keeps_the_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    // The tracking half is the one the transfer was created from, and the one it went to.
    let (inn, out) = transfer(&server, &b, &b.house, &b.checking, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"payee_id": b.wallet.payee}));
    let new_id = str_field(&got, "transfer_transaction_id");
    assert_ne!(new_id, id_of(&out));
    assert_matches(&got, &json!({"category_id": null, "transfer_account_id": b.wallet.id}), "the tracking half");
    gone(&server, &b, &id_of(&out), "GET the old on-budget half");
    assert_matches(
        &server.transaction(&b.plan, &new_id),
        &json!({"account_id": b.wallet.id, "amount": 100, "category_id": b.groceries,
                "transfer_account_id": b.house.id, "transfer_transaction_id": id_of(&inn)}),
        "the new on-budget half",
    );
    assert_eq!(server.balances(&b.plan, &b.checking.id).0, 0);

    let (out, inn) = transfer(&server, &b, &b.visa, &b.mortgage, 5000, json!({"category_id": "ready_to_assign"}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"payee_id": b.loc.payee, "amount": 300}));
    let new_id = str_field(&got, "transfer_transaction_id");
    gone(&server, &b, &id_of(&out), "GET the old Visa half");
    assert_matches(
        &server.transaction(&b.plan, &new_id),
        &json!({"account_id": b.loc.id, "amount": -300, "category_id": "ready_to_assign"}),
        "the new Credit Line half",
    );
    assert_matches(&got, &json!({"amount": 300, "category_id": null, "transfer_account_id": b.loc.id}), "the tracking half");
}

#[test]
fn spec_9_3_tracking_half_moving_with_a_category_sets_the_new_on_budget_half() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&inn),
        &json!({"payee_id": b.savings.payee, "category_id": b.household}),
    );
    assert_eq!(got["category_id"], Value::Null);
    gone(&server, &b, &id_of(&out), "GET the old on-budget half");
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&new_half, &json!({"account_id": b.savings.id, "category_id": b.household}), "the new on-budget half");
}

#[test]
fn spec_9_3_tracking_half_moving_with_an_invalid_category_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (_out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let before = server.plan_snapshot(&b.plan);
    for (payee, category) in [
        (&b.savings.payee, json!(null)),
        (&b.visa.payee, json!(b.visa_payment)),
        (&b.savings.payee, json!(b.visa_payment)),
    ] {
        let change = json!({"payee_id": payee, "category_id": category});
        assert_error(&server.patch_transaction(&b.plan, &id_of(&inn), &change), 400, &format!("PATCH tracking half {change}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
}

#[test]
fn spec_9_3_tracking_transfer_becoming_on_budget_needs_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (house, mortgage) = transfer(&server, &b, &b.house, &b.mortgage, -100, json!({}));
    let before = server.plan_snapshot(&b.plan);
    for change in [
        json!({"payee_id": b.checking.payee}),
        json!({"payee_id": b.checking.payee, "category_id": null}),
        json!({"payee_id": b.visa.payee, "memo": "now on budget"}),
    ] {
        let res = server.patch_transaction(&b.plan, &id_of(&house), &change);
        assert_error(&res, 400, &format!("PATCH tracking-to-tracking half with {change}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);

    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&house),
        &json!({"payee_id": b.checking.payee, "category_id": b.groceries}),
    );
    assert_matches(&got, &json!({"category_id": null, "transfer_account_id": b.checking.id}), "the tracking half");
    gone(&server, &b, &id_of(&mortgage), "GET the old Mortgage half");
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&new_half, &json!({"account_id": b.checking.id, "amount": 100, "category_id": b.groceries}), "Checking half");
}

#[test]
fn spec_9_3_tracking_transaction_with_no_on_budget_half_needs_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let txn = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.mortgage.id, "date": "2026-10-07", "amount": -900, "payee_id": shop}),
    );
    let before = server.plan_snapshot(&b.plan);
    for change in [json!({"payee_id": b.wallet.payee}), json!({"payee_id": b.wallet.payee, "category_id": null})] {
        assert_error(&server.patch_transaction(&b.plan, &id_of(&txn), &change), 400, &format!("PATCH {change}"));
    }
    assert_eq!(server.plan_snapshot(&b.plan), before);
    let got = server.patch_transaction_ok(
        &b.plan,
        &id_of(&txn),
        &json!({"payee_id": b.wallet.payee, "category_id": "ready_to_assign"}),
    );
    assert_eq!(got["category_id"], Value::Null);
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&new_half, &json!({"account_id": b.wallet.id, "amount": 900, "category_id": "ready_to_assign"}), "Wallet half");
}

#[test]
fn spec_9_3_tracking_half_moving_to_a_tracking_account_has_no_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let res = server.patch_transaction(&b.plan, &id_of(&inn), &json!({"payee_id": b.mortgage.payee, "category_id": b.groceries}));
    assert_error(&res, 400, "PATCH to a tracking-to-tracking transfer with a category");
    let got = server.patch_transaction_ok(&b.plan, &id_of(&inn), &json!({"payee_id": b.mortgage.payee}));
    assert_matches(&got, &json!({"category_id": null, "transfer_account_id": b.mortgage.id}), "the House half");
    gone(&server, &b, &id_of(&out), "GET the old Checking half");
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(&new_half, &json!({"account_id": b.mortgage.id, "amount": -100, "category_id": null}), "Mortgage half");
    assert_eq!(server.transactions_where(&b.plan, &format!("category_id={}", b.groceries)), Vec::<Value>::new());
}

// ---- 9.5 ----

#[test]
fn spec_9_5_on_budget_half_moving_to_another_tracking_account_keeps_its_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.checking, &b.house, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.mortgage.payee}));
    assert_matches(&got, &json!({"category_id": b.groceries, "transfer_account_id": b.mortgage.id}), "the Checking half");
    gone(&server, &b, &id_of(&inn), "GET the old House half");
    let new_half = server.transaction(&b.plan, &str_field(&got, "transfer_transaction_id"));
    assert_matches(
        &new_half,
        &json!({"account_id": b.mortgage.id, "amount": 100, "payee_id": b.checking.payee, "category_id": null,
                "transfer_account_id": b.checking.id, "transfer_transaction_id": id_of(&out)}),
        "the new Mortgage half",
    );
    assert_eq!(server.balances(&b.plan, &b.house.id), (0, 0, 0));
    assert_eq!(server.balances(&b.plan, &b.mortgage.id), (100, 0, 100));
}

#[test]
fn spec_9_5_new_other_half_is_created_after_existing_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, _) = transfer(&server, &b, &b.checking, &b.savings, -100, json!({}));
    let later = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.wallet.id, "date": "2026-10-05", "amount": -5, "category_id": b.groceries}),
    );
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.wallet.payee}));
    let new_id = str_field(&got, "transfer_transaction_id");
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![id_of(&out), id_of(&later), new_id.clone()]);
    assert_eq!(
        ids_of(&server.transactions_where(&b.plan, &format!("account_id={}", b.wallet.id))),
        vec![id_of(&later), new_id],
    );
}

#[test]
fn spec_9_5_ordinary_payee_on_tracking_half_keeps_nothing_on_budget() {
    let server = Server::start();
    let b = Budget::new(&server);
    let (out, inn) = transfer(&server, &b, &b.house, &b.visa, -100, json!({"category_id": b.groceries}));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_name": "Builder"}));
    assert_matches(
        &got,
        &json!({"category_id": null, "transfer_account_id": null, "transfer_transaction_id": null}),
        "the House half",
    );
    gone(&server, &b, &id_of(&inn), "GET the deleted Visa half");
    assert_eq!(server.balances(&b.plan, &b.visa.id), (0, 0, 0));
    assert_eq!(spec_fields(&server.transactions(&b.plan)[0]), spec_fields(&got));
}
