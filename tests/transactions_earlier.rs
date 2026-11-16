//! Clauses in sections 5 to 7 that transactions make testable.

use mybudget_testsuite::{
    Budget, START_DATE, Server, Value, assert_error, assert_matches, assert_status, id_of, ids_of, json, merged,
    spec_fields, str_field, with,
};

fn starting_balance_payee(server: &Server, plan: &str) -> String {
    let found: Vec<Value> = server.payees(plan).into_iter().filter(|p| p["name"] == "Starting Balance").collect();
    assert_eq!(found.len(), 1, "payees: {:?}", server.payees(plan));
    id_of(&found[0])
}

fn account_path(b: &Budget, id: &str) -> String {
    b.path(&format!("/accounts/{id}"))
}

fn payee_path(b: &Budget, id: &str) -> String {
    b.path(&format!("/payees/{id}"))
}

/// The plan's ordinary payees named exactly `name`.
fn ordinary_payees_named(server: &Server, plan: &str, name: &str) -> Vec<Value> {
    server.payees(plan).into_iter().filter(|p| p["name"] == name && p["transfer_account_id"].is_null()).collect()
}

// ---- 5.3 ----

#[test]
fn spec_5_3_starting_balance_transaction_fields() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "starting_balance": 2500000, "starting_date": "2026-10-01"}),
    );
    let txns = server.transactions(&plan);
    assert_eq!(txns.len(), 1, "{txns:?}");
    let payee = starting_balance_payee(&server, &plan);
    let want = json!({"account_id": id_of(&checking), "date": "2026-10-01", "amount": 2500000, "payee_id": payee,
                      "category_id": "ready_to_assign", "memo": null, "cleared": "cleared", "approved": true,
                      "flag_color": null, "transfer_account_id": null, "transfer_transaction_id": null,
                      "subtransactions": []});
    assert_matches(&txns[0], &want, "starting balance in the list");
    assert_matches(&server.transaction(&plan, &id_of(&txns[0])), &want, "GET starting balance");
}

#[test]
fn spec_5_3_starting_balance_category_follows_on_budget() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, balance, category) in [
        ("savings", 1000_i64, json!("ready_to_assign")),
        ("cash", 5, json!("ready_to_assign")),
        ("creditCard", -350000, json!("ready_to_assign")),
        ("lineOfCredit", -1, json!("ready_to_assign")),
        ("otherAsset", 300000000, json!(null)),
        ("otherLiability", -200000000, json!(null)),
    ] {
        let account = server.create_account(
            &plan,
            &json!({"name": ty, "type": ty, "starting_balance": balance, "starting_date": "2025-06-30"}),
        );
        let txns = server.transactions_where(&plan, &format!("account_id={}", id_of(&account)));
        assert_eq!(txns.len(), 1, "{ty}: {txns:?}");
        assert_matches(
            &txns[0],
            &json!({"date": "2025-06-30", "amount": balance, "category_id": category, "cleared": "cleared",
                    "approved": true, "memo": null, "flag_color": null, "subtransactions": []}),
            ty,
        );
    }
}

#[test]
fn spec_5_3_starting_balance_uses_the_starting_balance_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let existing = id_of(&server.create_payee(&plan, "Starting Balance"));
    server.add_account(&plan, "A", "checking", 100);
    server.add_account(&plan, "B", "otherAsset", 200);
    for t in server.transactions(&plan) {
        assert_eq!(t["payee_id"], json!(existing), "{t}");
    }
}

#[test]
fn spec_5_3_zero_starting_balance_lists_no_transaction() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.create_account(&plan, &json!({"name": "A", "type": "checking", "starting_balance": 0, "starting_date": START_DATE}));
    server.create_account(&plan, &json!({"name": "B", "type": "creditCard", "starting_balance": -0}));
    server.create_account(&plan, &json!({"name": "C", "type": "otherAsset"}));
    assert!(server.transactions(&plan).is_empty());
}

// ---- 5.4 ----

#[test]
fn spec_5_4_balances_follow_cleared_state() {
    let server = Server::start();
    let b = Budget::new(&server);
    let mk = |amount: i64, cleared: &str| {
        id_of(&server.create_transaction(&b.plan, &merged(&b.spend(amount), &json!({"cleared": cleared}))))
    };
    mk(-1000, "uncleared");
    mk(5000, "cleared");
    mk(-200, "reconciled");
    let moving = mk(-30, "uncleared");
    assert_eq!(server.balances(&b.plan, &b.checking.id), (3770, 4800, -1030));
    server.patch_transaction_ok(&b.plan, &moving, &json!({"cleared": "reconciled"}));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (3770, 4770, -1000));
    server.patch_transaction_ok(&b.plan, &moving, &json!({"amount": 70}));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (3870, 4870, -1000));
    server.patch_transaction_ok(&b.plan, &moving, &json!({"cleared": "uncleared"}));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (3870, 4800, -930));
    assert_status(&server.delete_transaction(&b.plan, &moving), 204, "DELETE");
    assert_eq!(server.balances(&b.plan, &b.checking.id), (3800, 4800, -1000));
    let listed = server.accounts(&b.plan);
    let in_list = listed.iter().find(|a| a["id"] == json!(b.checking.id)).unwrap();
    assert_matches(in_list, &json!({"balance": 3800, "cleared_balance": 4800, "uncleared_balance": -1000}), "list");
}

#[test]
fn spec_5_4_balances_include_starting_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = id_of(&server.add_account(&plan, "Checking", "checking", 2500000));
    let group = id_of(&server.create_group(&plan, "G"));
    let cat = id_of(&server.create_category(&plan, &group, "C"));
    server.create_transaction(
        &plan,
        &json!({"account_id": checking, "date": "2026-10-04", "amount": -120000, "category_id": cat}),
    );
    assert_eq!(server.balances(&plan, &checking), (2380000, 2500000, -120000));
}

#[test]
fn spec_5_4_split_counts_once() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -90000, "cleared": "cleared",
                "subtransactions": [{"amount": -60000, "category_id": b.groceries},
                                    {"amount": -30000, "category_id": b.household}]}),
    );
    assert_eq!(server.balances(&b.plan, &b.checking.id), (-90000, -90000, 0));
}

#[test]
fn spec_5_4_tracking_and_credit_balances() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-06", "amount": 7000}));
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-06", "amount": -4000, "category_id": b.groceries}),
    );
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.wallet.id, "date": "2026-10-06", "amount": -15, "category_id": b.groceries}),
    );
    assert_eq!(server.balances(&b.plan, &b.house.id), (7000, 0, 7000));
    assert_eq!(server.balances(&b.plan, &b.visa.id), (-4000, 0, -4000));
    assert_eq!(server.balances(&b.plan, &b.wallet.id), (-15, -15, 0));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (0, 0, 0));
}

#[test]
fn spec_5_4_balances_cover_extreme_amounts() {
    let server = Server::start();
    let b = Budget::new(&server);
    let max = 1_000_000_000_000_i64;
    for _ in 0..5 {
        server.create_transaction(&b.plan, &merged(&b.spend(max), &json!({"cleared": "cleared"})));
    }
    server.create_transaction(&b.plan, &b.spend(-max));
    assert_eq!(server.balances(&b.plan, &b.checking.id), (4 * max, 5 * max, -max));
}

// ---- 5.8 ----

#[test]
fn spec_5_8_closing_with_transactions_needs_zero_balance() {
    let server = Server::start();
    let b = Budget::new(&server);
    let path = account_path(&b, &b.checking.id);
    server.create_transaction(&b.plan, &b.spend(-5000));
    assert_error(&server.patch(&path, &json!({"closed": true})), 409, "close with balance -5000");
    assert_eq!(server.account(&b.plan, &b.checking.id)["closed"], false);
    server.create_transaction(
        &b.plan,
        &merged(&b.spend(5000), &json!({"cleared": "cleared", "category_id": "ready_to_assign"})),
    );
    assert_eq!(server.balances(&b.plan, &b.checking.id), (0, 5000, -5000));
    let got = server.patch_ok(&path, &json!({"closed": true}));
    assert_eq!(got["closed"], true);
}

#[test]
fn spec_5_8_closing_after_starting_balance_is_spent() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = id_of(&server.add_account(&plan, "Checking", "checking", 1000));
    let group = id_of(&server.create_group(&plan, "G"));
    let cat = id_of(&server.create_category(&plan, &group, "C"));
    server.create_transaction(&plan, &json!({"account_id": checking, "date": "2026-10-04", "amount": -1000, "category_id": cat}));
    let got = server.patch_ok(&format!("/plans/{plan}/accounts/{checking}"), &json!({"closed": true}));
    assert_eq!(got["closed"], true);
}

#[test]
fn spec_5_8_closed_account_still_takes_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.patch_ok(&account_path(&b, &b.checking.id), &json!({"closed": true}));
    let txn = server.create_transaction(&b.plan, &b.spend(-700));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"amount": -800, "memo": "still here"}));
    assert_eq!(got["amount"], -800);
    let out = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -50, "payee_id": b.savings.payee}),
    );
    assert_eq!(out["transfer_account_id"], json!(b.savings.id));
    let account = server.account(&b.plan, &b.checking.id);
    assert_matches(&account, &json!({"closed": true, "balance": -850}), "closed account");
    assert_status(&server.delete_transaction(&b.plan, &id_of(&txn)), 204, "DELETE in closed account");
}

#[test]
fn spec_5_8_closed_account_with_balance_can_be_reopened() {
    let server = Server::start();
    let b = Budget::new(&server);
    let path = account_path(&b, &b.checking.id);
    server.patch_ok(&path, &json!({"closed": true}));
    server.create_transaction(&b.plan, &b.spend(-700));
    let got = server.patch_ok(&path, &json!({"closed": false}));
    assert_matches(&got, &json!({"closed": false, "balance": -700}), "reopened");
    assert_error(&server.patch(&path, &json!({"closed": true})), 409, "close again with balance");
}

#[test]
fn spec_5_8_only_an_open_account_needs_zero_balance_to_close() {
    let server = Server::start();
    let b = Budget::new(&server);
    let path = account_path(&b, &b.checking.id);
    server.patch_ok(&path, &json!({"closed": true}));
    server.create_transaction(&b.plan, &b.spend(-700));
    let got = server.patch_ok(&path, &json!({"closed": true, "note": "still closed"}));
    assert_matches(&got, &json!({"closed": true, "note": "still closed", "balance": -700}), "PATCH closed account");
    let got = server.patch_ok(&path, &json!({"name": "Old Checking"}));
    assert_eq!(got["closed"], true);
}

#[test]
fn spec_5_8_open_account_with_balance_can_still_be_patched() {
    let server = Server::start();
    let b = Budget::new(&server);
    let path = account_path(&b, &b.checking.id);
    server.create_transaction(&b.plan, &b.spend(-700));
    let got = server.patch_ok(&path, &json!({"closed": false, "note": "n"}));
    assert_matches(&got, &json!({"closed": false, "note": "n"}), "PATCH open account");
}

// ---- 5.9 ----

#[test]
fn spec_5_9_account_with_transactions_returns_409() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-100));
    let res = server.delete(&account_path(&b, &b.checking.id));
    assert_error(&res, 409, "DELETE account with a transaction");
    assert_eq!(server.account(&b.plan, &b.checking.id)["id"], json!(b.checking.id));
    assert_status(&server.delete_transaction(&b.plan, &id_of(&txn)), 204, "DELETE the transaction");
    assert_status(&server.delete(&account_path(&b, &b.checking.id)), 204, "DELETE the emptied account");
}

#[test]
fn spec_5_9_account_with_tracking_or_split_transactions_returns_409() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-04", "amount": 0}));
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-04", "amount": 0, "subtransactions": [
            {"amount": 10, "category_id": b.groceries}, {"amount": -10, "category_id": b.household}]}),
    );
    assert_error(&server.delete(&account_path(&b, &b.house.id)), 409, "DELETE tracking account with a transaction");
    assert_error(&server.delete(&account_path(&b, &b.visa.id)), 409, "DELETE credit account with a split");
    let category = server.get(&b.path(&format!("/categories/{}", b.visa_payment)));
    assert_status(&category, 200, "Visa's payment category is still there");
}

#[test]
fn spec_5_9_other_half_of_a_transfer_counts() {
    let server = Server::start();
    let b = Budget::new(&server);
    let out = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -500, "payee_id": b.savings.payee}),
    );
    assert_error(&server.delete(&account_path(&b, &b.savings.id)), 409, "DELETE transfer's target account");
    assert_error(&server.delete(&account_path(&b, &b.checking.id)), 409, "DELETE transfer's source account");
    assert_status(&server.delete_transaction(&b.plan, &id_of(&out)), 204, "DELETE the transfer");
    assert_status(&server.delete(&account_path(&b, &b.savings.id)), 204, "DELETE the target account");
    assert_status(&server.delete(&account_path(&b, &b.checking.id)), 204, "DELETE the source account");
}

#[test]
fn spec_5_9_closed_account_with_transactions_returns_409() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.patch_ok(&account_path(&b, &b.checking.id), &json!({"closed": true}));
    server.create_transaction(&b.plan, &b.spend(0));
    assert_error(&server.delete(&account_path(&b, &b.checking.id)), 409, "DELETE closed account with a transaction");
}

#[test]
fn spec_5_9_account_whose_transactions_moved_away_can_be_deleted() {
    let server = Server::start();
    let b = Budget::new(&server);
    let out = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -500, "payee_id": b.savings.payee}),
    );
    server.patch_transaction_ok(&b.plan, &id_of(&out), &json!({"payee_id": b.wallet.payee}));
    assert_status(&server.delete(&account_path(&b, &b.savings.id)), 204, "DELETE the account the transfer left");
}

// ---- 6.4 ----

#[test]
fn spec_6_4_payee_in_use_returns_409() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_id", Some(json!(shop))));
    assert_error(&server.delete(&payee_path(&b, &shop)), 409, "DELETE payee in use");
    assert_eq!(server.get_json(&payee_path(&b, &shop))["name"], "Shop");
    assert_status(&server.delete_transaction(&b.plan, &id_of(&txn)), 204, "DELETE the transaction");
    assert_status(&server.delete(&payee_path(&b, &shop)), 204, "DELETE the payee no longer in use");
}

#[test]
fn spec_6_4_payee_freed_by_patch_can_be_deleted() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let other = id_of(&server.create_payee(&b.plan, "Other"));
    let a = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_id", Some(json!(shop)))));
    let c = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_id", Some(json!(shop)))));
    server.patch_transaction_ok(&b.plan, &a, &json!({"payee_id": other}));
    assert_error(&server.delete(&payee_path(&b, &shop)), 409, "DELETE payee still used once");
    server.patch_transaction_ok(&b.plan, &c, &json!({"payee_id": null}));
    assert_status(&server.delete(&payee_path(&b, &shop)), 204, "DELETE payee no longer used");
    assert_error(&server.delete(&payee_path(&b, &other)), 409, "DELETE payee now in use");
}

#[test]
fn spec_6_4_payee_created_by_payee_name_is_in_use() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("New Shop"))));
    let payee = str_field(&txn, "payee_id");
    assert_error(&server.delete(&payee_path(&b, &payee)), 409, "DELETE payee created by payee_name");
}

#[test]
fn spec_6_4_payee_used_in_other_accounts_counts() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-04", "amount": 1, "payee_id": shop}));
    assert_error(&server.delete(&payee_path(&b, &shop)), 409, "DELETE payee used in a tracking account");
}

#[test]
fn spec_6_4_transfer_payee_in_use_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -500, "payee_id": b.savings.payee}),
    );
    for payee in [&b.savings.payee, &b.checking.payee] {
        assert_error(&server.delete(&payee_path(&b, payee)), 400, "DELETE transfer payee in use");
    }
}

#[test]
fn spec_6_4_starting_balance_payee_is_free_once_unused() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 1000);
    let payee = starting_balance_payee(&server, &plan);
    let txn = id_of(&server.transactions(&plan)[0]);
    assert_status(&server.delete_transaction(&plan, &txn), 204, "DELETE starting balance transaction");
    assert_status(&server.delete(&format!("/plans/{plan}/payees/{payee}")), 204, "DELETE unused Starting Balance payee");
}

// ---- 6.5 ----

#[test]
fn spec_6_5_payee_name_creates_a_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("Corner Shop"))));
    let found = ordinary_payees_named(&server, &b.plan, "Corner Shop");
    assert_eq!(found.len(), 1, "{:?}", server.payees(&b.plan));
    assert_eq!(txn["payee_id"], found[0]["id"]);
    assert_matches(&found[0], &json!({"name": "Corner Shop", "transfer_account_id": null}), "created payee");
    let ids = ids_of(&server.payees(&b.plan));
    assert_eq!(ids.last(), Some(&id_of(&found[0])), "the new payee isn't the newest");
}

#[test]
fn spec_6_5_payee_name_finds_an_existing_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Corner Shop"));
    let count = server.payees(&b.plan).len();
    for _ in 0..2 {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("Corner Shop"))));
        assert_eq!(txn["payee_id"], json!(shop));
    }
    assert_eq!(server.payees(&b.plan).len(), count);
}

#[test]
fn spec_6_5_payee_name_reuses_the_payee_it_created() {
    let server = Server::start();
    let b = Budget::new(&server);
    let first = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("Megastore"))));
    let second = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("Megastore"))));
    assert_eq!(first["payee_id"], second["payee_id"]);
    assert_eq!(ordinary_payees_named(&server, &b.plan, "Megastore").len(), 1);
}

#[test]
fn spec_6_5_payee_name_matches_exactly() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Corner Shop"));
    for name in ["corner shop", "CORNER SHOP", "Corner Shop ", " Corner Shop", "Corner  Shop", "Corner\u{a0}Shop"] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!(name))));
        assert_ne!(txn["payee_id"], json!(shop), "payee_name {name:?} found Corner Shop");
        let found = ordinary_payees_named(&server, &b.plan, name);
        assert_eq!(found.len(), 1, "payee_name {name:?}: {:?}", server.payees(&b.plan));
        assert_eq!(txn["payee_id"], found[0]["id"]);
    }
}

#[test]
fn spec_6_5_payee_name_finds_starting_balance_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = id_of(&server.add_account(&plan, "Checking", "checking", 1000));
    let payee = starting_balance_payee(&server, &plan);
    let txn = server.create_transaction(
        &plan,
        &json!({"account_id": checking, "date": "2026-10-04", "amount": 5, "category_id": "ready_to_assign",
                "payee_name": "Starting Balance"}),
    );
    assert_eq!(txn["payee_id"], json!(payee));
}

#[test]
fn spec_6_5_payee_name_never_finds_a_transfer_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("Transfer : Savings"))));
    assert_ne!(txn["payee_id"], json!(b.savings.payee));
    assert_matches(&txn, &json!({"transfer_account_id": null, "transfer_transaction_id": null, "category_id": b.groceries}), "POST");
    let found = ordinary_payees_named(&server, &b.plan, "Transfer : Savings");
    assert_eq!(found.len(), 1, "{:?}", server.payees(&b.plan));
    assert_eq!(txn["payee_id"], found[0]["id"]);
    assert_eq!(server.transactions(&b.plan).len(), 1, "a transfer half was created");
    assert_eq!(server.balances(&b.plan, &b.savings.id), (0, 0, 0));
}

#[test]
fn spec_6_5_payee_name_prefers_ordinary_payee_over_transfer_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let ordinary = id_of(&server.create_payee(&b.plan, "Transfer : Savings"));
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!("Transfer : Savings"))));
    assert_eq!(txn["payee_id"], json!(ordinary));
    assert_eq!(txn["transfer_account_id"], Value::Null);
}

#[test]
fn spec_6_5_payee_name_in_tracking_account() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.house.id, "date": "2026-10-04", "amount": 1, "payee_name": "Appraiser"}),
    );
    assert_eq!(txn["payee_id"], ordinary_payees_named(&server, &b.plan, "Appraiser")[0]["id"]);
}

#[test]
fn spec_6_5_payee_id_wins_over_payee_name() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    server.create_payee(&b.plan, "Other");
    let count = server.payees(&b.plan).len();
    for name in ["Other", "Never Created"] {
        let body = merged(&b.spend(-100), &json!({"payee_id": shop, "payee_name": name}));
        let txn = server.create_transaction(&b.plan, &body);
        assert_eq!(txn["payee_id"], json!(shop), "payee_name {name}");
    }
    assert_eq!(server.payees(&b.plan).len(), count, "payee_name created a payee: {:?}", server.payees(&b.plan));
    let body = merged(&b.spend(-100), &json!({"payee_id": b.savings.payee, "payee_name": "Shop"}));
    let txn = server.create_transaction(&b.plan, &with(&body, "category_id", None));
    assert_eq!(txn["payee_id"], json!(b.savings.payee));
    assert_eq!(txn["transfer_account_id"], json!(b.savings.id));
}

#[test]
fn spec_6_5_null_payee_id_uses_payee_name() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = merged(&b.spend(-100), &json!({"payee_id": null, "payee_name": "Shop"}));
    let txn = server.create_transaction(&b.plan, &body);
    assert_eq!(txn["payee_id"], ordinary_payees_named(&server, &b.plan, "Shop")[0]["id"]);
}

#[test]
fn spec_6_5_rejects_invalid_payee_name() {
    let server = Server::start();
    let b = Budget::new(&server);
    let before = server.payees(&b.plan);
    for bad in [json!(""), json!("   "), json!("\t\n"), json!("x".repeat(201)), json!(5), json!(false), json!(["Shop"]), json!({"name": "Shop"})] {
        let body = with(&b.spend(-100), "payee_name", Some(bad.clone()));
        let what = format!("POST payee_name {}", bad.to_string().chars().take(40).collect::<String>());
        assert_error(&server.post_transaction(&b.plan, &body), 400, &what);
    }
    assert_eq!(server.payees(&b.plan), before);
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_6_5_unused_payee_name_must_still_be_valid() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    for bad in [json!(""), json!(" "), json!("x".repeat(201)), json!(5)] {
        let body = merged(&b.spend(-100), &json!({"payee_id": shop, "payee_name": bad}));
        assert_error(&server.post_transaction(&b.plan, &body), 400, "POST payee_id with invalid payee_name");
    }
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_id", Some(json!(shop))));
    for bad in [json!(""), json!("x".repeat(201)), json!(true)] {
        let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"payee_id": shop, "payee_name": bad}));
        assert_error(&res, 400, "PATCH payee_id with invalid payee_name");
    }
    assert!(server.transactions(&b.plan).len() == 1);
}

#[test]
fn spec_6_5_payee_name_may_have_200_characters() {
    let server = Server::start();
    let b = Budget::new(&server);
    for name in ["x".repeat(200), "\u{e9}".repeat(200), "\u{1f600}".repeat(200), " a ".to_owned()] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-100), "payee_name", Some(json!(name))));
        let payee = server.get_json(&payee_path(&b, &str_field(&txn, "payee_id")));
        assert_eq!(payee["name"], json!(name));
    }
}

#[test]
fn spec_6_5_patch_payee_name_finds_or_creates() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let txn = server.create_transaction(&b.plan, &b.spend(-100));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_name": "Shop"}));
    assert_eq!(got["payee_id"], json!(shop));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_name": "Fresh"}));
    assert_eq!(got["payee_id"], ordinary_payees_named(&server, &b.plan, "Fresh")[0]["id"]);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"payee_name": "Transfer : Savings"}));
    assert_eq!(got["transfer_account_id"], Value::Null);
    assert_ne!(got["payee_id"], json!(b.savings.payee));
}

#[test]
fn spec_6_5_patch_rejects_invalid_payee_name() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-100));
    let before = server.payees(&b.plan);
    for bad in [json!(""), json!("  "), json!("x".repeat(201)), json!(7)] {
        let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"payee_name": bad}));
        assert_error(&res, 400, "PATCH invalid payee_name");
    }
    assert_eq!(server.payees(&b.plan), before);
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
}

// ---- 7.8 ----

#[test]
fn spec_7_8_hidden_categories_take_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.patch_ok(&b.path(&format!("/categories/{}", b.groceries)), &json!({"hidden": true}));
    server.patch_ok(&b.path(&format!("/category_groups/{}", b.group)), &json!({"hidden": true}));
    let txn = server.create_transaction(&b.plan, &b.spend(-100));
    assert_eq!(txn["category_id"], json!(b.groceries));
    let split = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -3,
                "subtransactions": [{"amount": -1, "category_id": b.groceries}, {"amount": -2, "category_id": b.household}]}),
    );
    assert_eq!(split["subtransactions"][0]["category_id"], json!(b.groceries));
    let listed = server.transactions_where(&b.plan, &format!("category_id={}", b.groceries));
    assert_eq!(listed.len(), 2);
}
