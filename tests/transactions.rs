//! SPEC section 8: transactions.

use mybudget_testsuite::{
    Budget, Server, TRANSACTION_FIELDS, Value, assert_error, assert_fields, assert_matches, assert_status, id_of,
    ids_of, json, merged, spec_fields, str_field, with,
};

/// The id of the plan's ordinary payee named `name`.
fn payee_named(server: &Server, plan: &str, name: &str) -> String {
    let found: Vec<Value> = server
        .payees(plan)
        .into_iter()
        .filter(|p| p["name"] == name && p["transfer_account_id"].is_null())
        .collect();
    assert_eq!(found.len(), 1, "expected one payee named {name:?}: {:?}", server.payees(plan));
    id_of(&found[0])
}

/// A plan holding 5.2's Checking and a category Groceries, as 8.1's example assumes.
fn example_plan(server: &Server) -> (String, String, String) {
    let plan = server.plan_id("Household");
    let checking = server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "starting_balance": 2500000, "starting_date": "2026-10-01"}),
    );
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    (plan, id_of(&checking), groceries)
}

fn shop_body(checking: &str, groceries: &str) -> Value {
    json!({"account_id": checking, "date": "2026-10-04", "amount": -120000,
           "payee_name": "Corner Shop", "category_id": groceries, "memo": "weekly shop"})
}

fn pay_body(checking: &str) -> Value {
    json!({"account_id": checking, "date": "2026-10-15", "amount": 1800000, "payee_name": "Employer",
           "category_id": "ready_to_assign"})
}

fn shop_expected(txn_id: &str, checking: &str, shop: &str, groceries: &str) -> Value {
    json!({"id": txn_id, "account_id": checking, "date": "2026-10-04", "amount": -120000,
           "payee_id": shop, "category_id": groceries, "memo": "weekly shop",
           "cleared": "uncleared", "approved": true, "flag_color": null,
           "transfer_account_id": null, "transfer_transaction_id": null, "subtransactions": []})
}

// ---- 8.1 ----

#[test]
fn spec_8_1_creates_transaction_matching_example() {
    let server = Server::start();
    let (plan, checking, groceries) = example_plan(&server);
    let res = server.post_transaction(&plan, &shop_body(&checking, &groceries));
    assert_status(&res, 201, "POST 8.1's example");
    let txn = res.json();
    let shop = payee_named(&server, &plan, "Corner Shop");
    let id = id_of(&txn);
    assert_matches(&txn, &shop_expected(&id, &checking, &shop, &groceries), "8.1's example");
    assert_fields(&txn, &TRANSACTION_FIELDS, "8.1's example");
}

#[test]
fn spec_8_1_requires_account_date_and_amount() {
    let server = Server::start();
    let b = Budget::new(&server);
    for field in ["account_id", "date", "amount"] {
        let body = with(&b.spend(-1000), field, None);
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST without {field}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_8_1_rejects_account_id_that_is_not_an_account() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [
        json!("nope"),
        json!(""),
        json!(5),
        json!(true),
        json!([b.checking.id]),
        json!({"id": b.checking.id}),
        json!(b.checking.payee),
        json!(b.groceries),
        json!(b.group),
        json!(b.plan),
        json!("ready_to_assign"),
    ] {
        let body = with(&b.spend(-1000), "account_id", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with account_id {bad}"));
    }
}

#[test]
fn spec_8_1_transaction_without_payee_has_none() {
    let server = Server::start();
    let b = Budget::new(&server);
    let without = server.create_transaction(&b.plan, &b.spend(-1000));
    assert_eq!(without["payee_id"], Value::Null, "{without}");
    let null_id = server.create_transaction(&b.plan, &with(&b.spend(-1000), "payee_id", Some(Value::Null)));
    assert_eq!(null_id["payee_id"], Value::Null, "{null_id}");
    let both_null = server.create_transaction(
        &b.plan,
        &merged(&b.spend(-1000), &json!({"payee_id": null, "payee_name": null})),
    );
    assert_eq!(both_null["payee_id"], Value::Null, "{both_null}");
    assert_eq!(server.transaction(&b.plan, &id_of(&without))["payee_id"], Value::Null);
}

#[test]
fn spec_8_1_takes_payee_id() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Corner Shop"));
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-1000), "payee_id", Some(json!(shop))));
    assert_eq!(txn["payee_id"], json!(shop));
    assert_eq!(server.transaction(&b.plan, &id_of(&txn))["payee_id"], json!(shop));
}

#[test]
fn spec_8_1_rejects_payee_id_that_is_not_a_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [
        json!("nope"),
        json!(""),
        json!(5),
        json!(false),
        json!([]),
        json!(b.checking.id),
        json!(b.groceries),
        json!(b.plan),
        json!("Corner Shop"),
    ] {
        let body = with(&b.spend(-1000), "payee_id", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with payee_id {bad}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_8_1_memo_defaults_to_null_and_is_stored_exactly() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    assert_eq!(txn["memo"], Value::Null, "{txn}");
    for memo in [json!(null), json!(""), json!("  weekly shop  "), json!("caf\u{e9} \u{1f600}\n\ttab")] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-1000), "memo", Some(memo.clone())));
        assert_eq!(txn["memo"], memo, "POST memo {memo}");
        assert_eq!(server.transaction(&b.plan, &id_of(&txn))["memo"], memo, "GET memo {memo}");
    }
}

#[test]
fn spec_8_1_rejects_invalid_memo() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [json!("x".repeat(501)), json!(5), json!(true), json!(["a"]), json!({"text": "a"})] {
        let body = with(&b.spend(-1000), "memo", Some(bad.clone()));
        let what = format!("POST with memo {}", bad.to_string().chars().take(40).collect::<String>());
        assert_error(&server.post_transaction(&b.plan, &body), 400, &what);
    }
}

#[test]
fn spec_8_1_cleared_defaults_by_account_type() {
    let server = Server::start();
    let b = Budget::new(&server);
    let on_budget = [(&b.checking, "uncleared"), (&b.savings, "uncleared"), (&b.wallet, "cleared"), (&b.visa, "uncleared"), (&b.loc, "uncleared")];
    for (account, want) in on_budget {
        let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": -1000, "category_id": b.groceries});
        let txn = server.create_transaction(&b.plan, &body);
        assert_eq!(txn["cleared"], want, "POST in {account:?}: {txn}");
        assert_eq!(server.transaction(&b.plan, &id_of(&txn))["cleared"], want);
    }
    for account in [&b.house, &b.mortgage] {
        let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": -1000});
        let txn = server.create_transaction(&b.plan, &body);
        assert_eq!(txn["cleared"], "uncleared", "POST in {account:?}: {txn}");
    }
}

#[test]
fn spec_8_1_accepts_each_cleared_value() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.checking, &b.wallet] {
        for cleared in ["uncleared", "cleared", "reconciled"] {
            let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": -1000,
                              "category_id": b.groceries, "cleared": cleared});
            let txn = server.create_transaction(&b.plan, &body);
            assert_eq!(txn["cleared"], cleared, "POST cleared {cleared}: {txn}");
            assert_eq!(server.transaction(&b.plan, &id_of(&txn))["cleared"], cleared);
        }
    }
}

#[test]
fn spec_8_1_rejects_invalid_cleared() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [json!("Cleared"), json!("UNCLEARED"), json!("pending"), json!(""), json!(null), json!(true), json!(1)] {
        let body = with(&b.spend(-1000), "cleared", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with cleared {bad}"));
    }
}

#[test]
fn spec_8_1_approved_defaults_to_true() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    assert_eq!(txn["approved"], true);
    for approved in [true, false] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-1000), "approved", Some(json!(approved))));
        assert_eq!(txn["approved"], approved, "POST approved {approved}");
        assert_eq!(server.transaction(&b.plan, &id_of(&txn))["approved"], approved);
    }
}

#[test]
fn spec_8_1_rejects_invalid_approved() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [json!(null), json!("true"), json!("false"), json!(1), json!(0), json!([true])] {
        let body = with(&b.spend(-1000), "approved", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with approved {bad}"));
    }
}

#[test]
fn spec_8_1_accepts_each_flag_color() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    assert_eq!(txn["flag_color"], Value::Null);
    for flag in [json!(null), json!("red"), json!("orange"), json!("yellow"), json!("green"), json!("blue"), json!("purple")] {
        let txn = server.create_transaction(&b.plan, &with(&b.spend(-1000), "flag_color", Some(flag.clone())));
        assert_eq!(txn["flag_color"], flag, "POST flag_color {flag}");
        assert_eq!(server.transaction(&b.plan, &id_of(&txn))["flag_color"], flag);
    }
}

#[test]
fn spec_8_1_rejects_invalid_flag_color() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [json!("Red"), json!("RED"), json!("pink"), json!("none"), json!(""), json!(" red"), json!(1), json!(true), json!(["red"])] {
        let body = with(&b.spend(-1000), "flag_color", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with flag_color {bad}"));
    }
}

#[test]
fn spec_8_1_stores_date_and_amount() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (date, amount) in [("2026-10-04", -120000_i64), ("1900-01-01", 1), ("2999-12-31", 0), ("2028-02-29", 1_000_000_000_000)] {
        let body = merged(&b.spend(0), &json!({"date": date, "amount": amount}));
        let txn = server.create_transaction(&b.plan, &body);
        assert_matches(&txn, &json!({"account_id": b.checking.id, "date": date, "amount": amount}), "POST");
        let got = server.transaction(&b.plan, &id_of(&txn));
        assert_matches(&got, &json!({"account_id": b.checking.id, "date": date, "amount": amount}), "GET");
    }
}

// ---- 8.2 ----

#[test]
fn spec_8_2_creates_income_matching_example() {
    let server = Server::start();
    let (plan, checking, _) = example_plan(&server);
    let res = server.post_transaction(&plan, &pay_body(&checking));
    assert_status(&res, 201, "POST 8.2's example");
    let txn = res.json();
    let employer = payee_named(&server, &plan, "Employer");
    assert_matches(
        &txn,
        &json!({"id": id_of(&txn), "account_id": checking, "date": "2026-10-15", "amount": 1800000,
                "payee_id": employer, "category_id": "ready_to_assign", "memo": null,
                "cleared": "uncleared", "approved": true, "flag_color": null,
                "transfer_account_id": null, "transfer_transaction_id": null, "subtransactions": []}),
        "8.2's example",
    );
}

#[test]
fn spec_8_2_on_budget_transaction_requires_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.checking, &b.savings, &b.wallet, &b.visa, &b.loc] {
        let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": -1000});
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST without category in {account:?}"));
        let body = with(&body, "category_id", Some(Value::Null));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST null category in {account:?}"));
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_8_2_on_budget_accepts_user_category_or_ready_to_assign() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.checking, &b.savings, &b.wallet, &b.visa, &b.loc] {
        let rta = "ready_to_assign".to_owned();
        for (category, amount) in [(&b.groceries, -1000), (&b.household, 2000), (&rta, 3000), (&rta, -4000)] {
            let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": amount, "category_id": category});
            let txn = server.create_transaction(&b.plan, &body);
            assert_eq!(txn["category_id"], json!(category), "POST in {account:?}: {txn}");
            assert_eq!(server.transaction(&b.plan, &id_of(&txn))["category_id"], json!(category));
        }
    }
}

#[test]
fn spec_8_2_tracking_transaction_has_no_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.house, &b.mortgage] {
        let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": 5000});
        let txn = server.create_transaction(&b.plan, &body);
        assert_eq!(txn["category_id"], Value::Null, "{txn}");
        let txn = server.create_transaction(&b.plan, &with(&body, "category_id", Some(Value::Null)));
        assert_eq!(txn["category_id"], Value::Null, "{txn}");
        for bad in [&b.groceries, &"ready_to_assign".to_owned(), &b.visa_payment] {
            let res = server.post_transaction(&b.plan, &with(&body, "category_id", Some(json!(bad))));
            assert_error(&res, 400, &format!("POST category {bad} in tracking account {account:?}"));
        }
    }
}

#[test]
fn spec_8_2_rejects_payment_categories() {
    let server = Server::start();
    let b = Budget::new(&server);
    let loc_payment = str_field(&server.account(&b.plan, &b.loc.id), "payment_category_id");
    for account in [&b.checking, &b.wallet, &b.visa, &b.loc] {
        for payment in [&b.visa_payment, &loc_payment] {
            let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": 1000, "category_id": payment});
            assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST payment category in {account:?}"));
        }
    }
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_8_2_rejects_category_id_that_is_not_a_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in [
        json!("nope"),
        json!(""),
        json!("READY_TO_ASSIGN"),
        json!("Ready to Assign"),
        json!("ready_to_assign "),
        json!("Groceries"),
        json!(b.group),
        json!(b.checking.id),
        json!(b.checking.payee),
        json!(b.plan),
        json!(5),
        json!(true),
        json!([b.groceries]),
    ] {
        let body = with(&b.spend(-1000), "category_id", Some(bad.clone()));
        assert_error(&server.post_transaction(&b.plan, &body), 400, &format!("POST with category_id {bad}"));
    }
}

// ---- 8.3 ----

#[test]
fn spec_8_3_transfer_payee_makes_a_transfer() {
    let server = Server::start();
    let b = Budget::new(&server);
    let body = json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -5000, "payee_id": b.savings.payee});
    let txn = server.create_transaction(&b.plan, &body);
    assert_eq!(txn["transfer_account_id"], json!(b.savings.id), "{txn}");
    assert!(txn["transfer_transaction_id"].is_string(), "{txn}");
}

#[test]
fn spec_8_3_ordinary_payee_does_not_make_a_transfer() {
    let server = Server::start();
    let b = Budget::new(&server);
    let payee = id_of(&server.create_payee(&b.plan, "Transfer : Savings"));
    let txn = server.create_transaction(&b.plan, &with(&b.spend(-1000), "payee_id", Some(json!(payee))));
    assert_matches(&txn, &json!({"transfer_account_id": null, "transfer_transaction_id": null}), "ordinary payee");
    assert_eq!(server.transactions(&b.plan).len(), 1);
}

// ---- 8.4 ----

#[test]
fn spec_8_4_gets_one_transaction() {
    let server = Server::start();
    let (plan, checking, groceries) = example_plan(&server);
    let created = server.create_transaction(&plan, &shop_body(&checking, &groceries));
    let id = id_of(&created);
    let got = server.transaction(&plan, &id);
    let shop = payee_named(&server, &plan, "Corner Shop");
    assert_matches(&got, &shop_expected(&id, &checking, &shop, &groceries), "GET 8.1's transaction");
    assert_eq!(spec_fields(&got), spec_fields(&created));
}

#[test]
fn spec_8_4_missing_transaction_returns_404() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(&b.plan, &b.spend(-1000));
    for id in ["nope".to_owned(), b.checking.id.clone(), b.checking.payee.clone(), b.groceries.clone(), b.plan.clone(), "ready_to_assign".to_owned()] {
        assert_error(&server.get_transaction(&b.plan, &id), 404, &format!("GET transactions/{id}"));
    }
}

// ---- 8.5 ----

#[test]
fn spec_8_5_lists_matching_example() {
    let server = Server::start();
    let (plan, checking, groceries) = example_plan(&server);
    let shop_txn = server.create_transaction(&plan, &shop_body(&checking, &groceries));
    let pay_txn = server.create_transaction(&plan, &pay_body(&checking));
    let shop = payee_named(&server, &plan, "Corner Shop");
    let query = format!("account_id={checking}&since_date=2026-10-02&until_date=2026-10-10");
    let listed = server.transactions_where(&plan, &query);
    let want = json!([shop_expected(&id_of(&shop_txn), &checking, &shop, &groceries)]);
    assert_matches(&Value::Array(listed), &want, "8.5's example");
    let all = server.transactions(&plan);
    assert_eq!(all.len(), 3, "{all:?}");
    assert_eq!(all[1]["id"], shop_txn["id"]);
    assert_eq!(all[2]["id"], pay_txn["id"]);
    assert_eq!(all[0]["date"], "2026-10-01", "the starting balance comes first: {all:?}");
}

#[test]
fn spec_8_5_new_plan_lists_no_transactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    assert!(server.transactions(&b.plan).is_empty());
}

#[test]
fn spec_8_5_lists_by_date_then_oldest_first() {
    let server = Server::start();
    let b = Budget::new(&server);
    let dates = ["2026-10-05", "2026-10-03", "2026-10-05", "2026-09-30", "2026-10-03", "2025-12-31", "2026-01-01", "2026-10-05"];
    let accounts = [&b.checking, &b.savings, &b.wallet, &b.visa];
    let ids: Vec<String> = dates
        .iter()
        .enumerate()
        .map(|(i, date)| {
            let body = json!({"account_id": accounts[i % 4].id, "date": date, "amount": -1000 - i as i64,
                              "category_id": b.groceries});
            id_of(&server.create_transaction(&b.plan, &body))
        })
        .collect();
    let want: Vec<String> = [5, 6, 3, 1, 4, 0, 2, 7].iter().map(|&i| ids[i].clone()).collect();
    assert_eq!(ids_of(&server.transactions(&b.plan)), want);
}

#[test]
fn spec_8_5_order_within_a_date_is_creation_order_after_date_changes() {
    let server = Server::start();
    let b = Budget::new(&server);
    let first = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1), "date", Some(json!("2026-10-09")))));
    let second = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-2), "date", Some(json!("2026-10-03")))));
    let third = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-3), "date", Some(json!("2026-10-03")))));
    server.patch_transaction_ok(&b.plan, &first, &json!({"date": "2026-10-03"}));
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![first.clone(), second.clone(), third.clone()]);
    server.patch_transaction_ok(&b.plan, &second, &json!({"amount": -20}));
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![first, second, third]);
}

#[test]
fn spec_8_5_filters_by_account() {
    let server = Server::start();
    let b = Budget::new(&server);
    let a = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    let s = id_of(&server.create_transaction(
        &b.plan,
        &json!({"account_id": b.savings.id, "date": "2026-10-02", "amount": 7000, "category_id": "ready_to_assign"}),
    ));
    let h = id_of(&server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-03", "amount": 9})));
    let out = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -500, "payee_id": b.savings.payee}),
    );
    let in_id = str_field(&out, "transfer_transaction_id");
    let check = |account: &str, want: Vec<String>| {
        let got = ids_of(&server.transactions_where(&b.plan, &format!("account_id={account}")));
        assert_eq!(got, want, "account_id={account}");
    };
    check(&b.checking.id, vec![a, id_of(&out)]);
    check(&b.savings.id, vec![s, in_id]);
    check(&b.house.id, vec![h]);
    check(&b.wallet.id, vec![]);
}

#[test]
fn spec_8_5_filters_by_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop1 = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1), "payee_name", Some(json!("Shop")))));
    server.create_transaction(&b.plan, &with(&b.spend(-2), "payee_name", Some(json!("Other"))));
    server.create_transaction(&b.plan, &b.spend(-3));
    let shop2 = id_of(&server.create_transaction(&b.plan, &merged(&b.spend(-4), &json!({"payee_name": "Shop", "date": "2026-10-01"}))));
    let shop = payee_named(&server, &b.plan, "Shop");
    assert_eq!(ids_of(&server.transactions_where(&b.plan, &format!("payee_id={shop}"))), vec![shop2, shop1]);
    let out = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -500, "payee_id": b.savings.payee}),
    );
    let got = ids_of(&server.transactions_where(&b.plan, &format!("payee_id={}", b.savings.payee)));
    assert_eq!(got, vec![id_of(&out)]);
    let got = ids_of(&server.transactions_where(&b.plan, &format!("payee_id={}", b.checking.payee)));
    assert_eq!(got, vec![str_field(&out, "transfer_transaction_id")]);
}

#[test]
fn spec_8_5_filters_by_category_including_subtransactions() {
    let server = Server::start();
    let b = Budget::new(&server);
    let plain = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    let other = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1000), "category_id", Some(json!(b.household)))));
    let split_with = id_of(&server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -3000, "subtransactions": [
            {"amount": -1000, "category_id": b.household}, {"amount": -2000, "category_id": b.groceries}]}),
    ));
    let split_without = id_of(&server.create_transaction(
        &b.plan,
        &json!({"account_id": b.checking.id, "date": "2026-10-06", "amount": -3000, "subtransactions": [
            {"amount": -1000, "category_id": b.household}, {"amount": -2000, "category_id": "ready_to_assign"}]}),
    ));
    let income = id_of(&server.create_transaction(&b.plan, &with(&b.spend(5000), "category_id", Some(json!("ready_to_assign")))));
    let by = |category: &str| ids_of(&server.transactions_where(&b.plan, &format!("category_id={category}")));
    assert_eq!(by(&b.groceries), vec![plain.clone(), split_with.clone()]);
    assert_eq!(by(&b.household), vec![other, split_with, split_without.clone()]);
    assert_eq!(by("ready_to_assign"), vec![income, split_without]);
}

#[test]
fn spec_8_5_filters_by_ready_to_assign_include_starting_balances() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = id_of(&server.add_account(&plan, "Checking", "checking", 2500000));
    server.add_account(&plan, "House", "otherAsset", 100000);
    let got = server.transactions_where(&plan, "category_id=ready_to_assign");
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["account_id"], json!(checking));
}

#[test]
fn spec_8_5_filters_by_date_range_inclusively() {
    let server = Server::start();
    let b = Budget::new(&server);
    let dates = ["2026-09-30", "2026-10-01", "2026-10-15", "2026-10-31", "2026-11-01"];
    let ids: Vec<String> = dates
        .iter()
        .map(|d| id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1000), "date", Some(json!(d))))))
        .collect();
    let q = |query: &str| ids_of(&server.transactions_where(&b.plan, query));
    assert_eq!(q("since_date=2026-10-01"), ids[1..].to_vec());
    assert_eq!(q("until_date=2026-10-31"), ids[..4].to_vec());
    assert_eq!(q("since_date=2026-10-01&until_date=2026-10-31"), ids[1..4].to_vec());
    assert_eq!(q("since_date=2026-10-15&until_date=2026-10-15"), vec![ids[2].clone()]);
    assert_eq!(q("since_date=2026-10-02&until_date=2026-10-14"), Vec::<String>::new());
    assert_eq!(q("since_date=2026-10-31&until_date=2026-10-01"), Vec::<String>::new());
    assert_eq!(q("since_date=1900-01-01&until_date=2999-12-31"), ids);
}

#[test]
fn spec_8_5_filters_by_approved() {
    let server = Server::start();
    let b = Budget::new(&server);
    let yes = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let no = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-2), "approved", Some(json!(false)))));
    let yes2 = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-3), "approved", Some(json!(true)))));
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "approved=true")), vec![yes.clone(), yes2.clone()]);
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "approved=false")), vec![no.clone()]);
    server.patch_transaction_ok(&b.plan, &no, &json!({"approved": true}));
    assert_eq!(ids_of(&server.transactions_where(&b.plan, "approved=true")), vec![yes, no, yes2]);
    assert!(server.transactions_where(&b.plan, "approved=false").is_empty());
}

#[test]
fn spec_8_5_filters_combine() {
    let server = Server::start();
    let b = Budget::new(&server);
    let mk = |account: &str, date: &str, category: &str, payee: &str, approved: bool| {
        let body = json!({"account_id": account, "date": date, "amount": -1000, "category_id": category,
                          "payee_name": payee, "approved": approved});
        id_of(&server.create_transaction(&b.plan, &body))
    };
    let target = mk(&b.checking.id, "2026-10-10", &b.groceries, "Shop", false);
    mk(&b.savings.id, "2026-10-10", &b.groceries, "Shop", false);
    mk(&b.checking.id, "2026-09-10", &b.groceries, "Shop", false);
    mk(&b.checking.id, "2026-11-10", &b.groceries, "Shop", false);
    mk(&b.checking.id, "2026-10-10", &b.household, "Shop", false);
    mk(&b.checking.id, "2026-10-10", &b.groceries, "Other", false);
    mk(&b.checking.id, "2026-10-10", &b.groceries, "Shop", true);
    let shop = payee_named(&server, &b.plan, "Shop");
    let query = format!(
        "account_id={}&payee_id={shop}&category_id={}&since_date=2026-10-01&until_date=2026-10-31&approved=false",
        b.checking.id, b.groceries
    );
    assert_eq!(ids_of(&server.transactions_where(&b.plan, &query)), vec![target]);
}

#[test]
fn spec_8_5_ids_that_match_nothing_give_empty_list() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(&b.plan, &b.spend(-1000));
    let other = server.plan_id("Other");
    let other_account = id_of(&server.add_account(&other, "Elsewhere", "checking", 1000));
    for query in [
        "account_id=nope".to_owned(),
        "payee_id=nope".to_owned(),
        "category_id=nope".to_owned(),
        format!("account_id={other_account}"),
        format!("account_id={}", b.groceries),
        format!("category_id={}", b.checking.id),
        format!("payee_id={}", b.checking.id),
        format!("category_id={}", b.visa_payment),
    ] {
        let res = server.get(&b.path(&format!("/transactions?{query}")));
        assert_status(&res, 200, &format!("GET transactions?{query}"));
        assert_eq!(res.json(), json!([]), "GET transactions?{query}");
    }
}

#[test]
fn spec_8_5_rejects_malformed_dates() {
    let server = Server::start();
    let b = Budget::new(&server);
    for param in ["since_date", "until_date"] {
        for bad in ["2026-02-30", "2026-10", "2026-10-1", "20261001", "1899-12-31", "3000-01-01", "", "today", "2026-10-01T00:00:00"] {
            let path = b.path(&format!("/transactions?{param}={bad}"));
            assert_error(&server.get(&path), 400, &format!("GET {path}"));
        }
    }
}

#[test]
fn spec_8_5_rejects_approved_other_than_true_or_false() {
    let server = Server::start();
    let b = Budget::new(&server);
    for bad in ["yes", "TRUE", "True", "False", "1", "0", "", "null"] {
        let path = b.path(&format!("/transactions?approved={bad}"));
        assert_error(&server.get(&path), 400, &format!("GET {path}"));
    }
}

#[test]
fn spec_8_5_transactions_of_missing_plan_return_404() {
    let server = Server::start();
    assert_error(&server.get("/plans/nope/transactions"), 404, "GET /plans/nope/transactions");
    assert_error(&server.post("/plans/nope/transactions", &json!({})), 404, "POST /plans/nope/transactions");
}

// ---- 8.6 ----

#[test]
fn spec_8_6_patches_transaction_matching_example() {
    let server = Server::start();
    let (plan, checking, groceries) = example_plan(&server);
    let id = id_of(&server.create_transaction(&plan, &shop_body(&checking, &groceries)));
    let shop = payee_named(&server, &plan, "Corner Shop");
    let res = server.patch_transaction(&plan, &id, &json!({"cleared": "cleared", "flag_color": "green"}));
    assert_status(&res, 200, "PATCH 8.6's example");
    let mut want = shop_expected(&id, &checking, &shop, &groceries);
    want["cleared"] = json!("cleared");
    want["flag_color"] = json!("green");
    assert_matches(&res.json(), &want, "8.6's example");
    assert_matches(&server.transaction(&plan, &id), &want, "GET after 8.6's example");
}

#[test]
fn spec_8_6_changes_each_field_alone() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let start = merged(&b.spend(-1000), &json!({"payee_id": shop, "memo": "m", "flag_color": "red"}));
    let changes = [
        json!({"date": "2027-01-31"}),
        json!({"amount": 2500}),
        json!({"amount": 0}),
        json!({"memo": "new memo"}),
        json!({"memo": null}),
        json!({"cleared": "reconciled"}),
        json!({"cleared": "cleared"}),
        json!({"approved": false}),
        json!({"flag_color": "purple"}),
        json!({"flag_color": null}),
        json!({"category_id": b.household}),
        json!({"category_id": "ready_to_assign"}),
        json!({"payee_id": null}),
    ];
    for change in changes {
        let txn = server.create_transaction(&b.plan, &start);
        let id = id_of(&txn);
        let mut want = spec_fields(&txn);
        for (k, v) in change.as_object().unwrap() {
            want[k] = v.clone();
        }
        let got = server.patch_transaction_ok(&b.plan, &id, &change);
        assert_eq!(spec_fields(&got), want, "PATCH {change}");
        assert_eq!(spec_fields(&server.transaction(&b.plan, &id)), want, "GET after PATCH {change}");
    }
}

#[test]
fn spec_8_6_changes_several_fields_at_once() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    let change = json!({"date": "2026-11-02", "amount": -777, "memo": "x", "cleared": "cleared", "approved": false,
                        "flag_color": "blue", "category_id": b.household});
    let got = server.patch_transaction_ok(&b.plan, &id, &change);
    assert_matches(&got, &change, "PATCH");
    assert_matches(&server.transaction(&b.plan, &id), &change, "GET after PATCH");
}

#[test]
fn spec_8_6_accepts_the_transactions_own_account_id() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"account_id": b.checking.id, "memo": "same"}));
    assert_eq!(got["account_id"], json!(b.checking.id));
    assert_eq!(got["memo"], "same");
}

#[test]
fn spec_8_6_rejects_a_different_account_id() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    let id = id_of(&txn);
    for bad in [json!(b.savings.id), json!(b.wallet.id), json!(b.house.id), json!("nope"), json!(null), json!(5)] {
        let res = server.patch_transaction(&b.plan, &id, &json!({"account_id": bad, "memo": "changed"}));
        assert_error(&res, 400, &format!("PATCH account_id {bad}"));
    }
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id)), spec_fields(&txn));
}

#[test]
fn spec_8_6_payee_id_changes_or_removes_the_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let a = id_of(&server.create_payee(&b.plan, "A"));
    let c = id_of(&server.create_payee(&b.plan, "C"));
    let id = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1000), "payee_id", Some(json!(a)))));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": c}));
    assert_eq!(got["payee_id"], json!(c));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": null}));
    assert_eq!(got["payee_id"], Value::Null);
    assert_eq!(server.transaction(&b.plan, &id)["payee_id"], Value::Null);
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": a}));
    assert_eq!(got["payee_id"], json!(a));
}

#[test]
fn spec_8_6_payee_name_string_finds_or_creates_the_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let a = id_of(&server.create_payee(&b.plan, "A"));
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_name": "A"}));
    assert_eq!(got["payee_id"], json!(a));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_name": "Brand New"}));
    let created = payee_named(&server, &b.plan, "Brand New");
    assert_eq!(got["payee_id"], json!(created));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": null, "payee_name": "A"}));
    assert_eq!(got["payee_id"], json!(a));
}

#[test]
fn spec_8_6_null_payee_name_alone_changes_nothing() {
    let server = Server::start();
    let b = Budget::new(&server);
    let a = id_of(&server.create_payee(&b.plan, "A"));
    let id = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1000), "payee_id", Some(json!(a)))));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_name": null}));
    assert_eq!(got["payee_id"], json!(a));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_name": null, "memo": "m"}));
    assert_eq!(got["payee_id"], json!(a));
    assert_eq!(server.transaction(&b.plan, &id)["payee_id"], json!(a));
}

#[test]
fn spec_8_6_null_payee_id_without_payee_name_removes_payee() {
    let server = Server::start();
    let b = Budget::new(&server);
    let a = id_of(&server.create_payee(&b.plan, "A"));
    let id = id_of(&server.create_transaction(&b.plan, &with(&b.spend(-1000), "payee_id", Some(json!(a)))));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": null, "payee_name": null}));
    assert_eq!(got["payee_id"], Value::Null);
}

#[test]
fn spec_8_6_left_out_fields_keep_their_values() {
    let server = Server::start();
    let b = Budget::new(&server);
    let shop = id_of(&server.create_payee(&b.plan, "Shop"));
    let body = json!({"account_id": b.wallet.id, "date": "2026-10-04", "amount": -1000, "category_id": b.household,
                      "payee_id": shop, "memo": "m", "cleared": "uncleared", "approved": false, "flag_color": "yellow"});
    let txn = server.create_transaction(&b.plan, &body);
    let got = server.patch_transaction_ok(&b.plan, &id_of(&txn), &json!({"memo": "only this"}));
    let mut want = spec_fields(&txn);
    want["memo"] = json!("only this");
    assert_eq!(spec_fields(&got), want);
    assert_eq!(got["cleared"], "uncleared", "cleared went back to the cash default");
}

#[test]
fn spec_8_6_on_budget_category_cannot_be_removed() {
    let server = Server::start();
    let b = Budget::new(&server);
    for account in [&b.checking, &b.wallet, &b.visa] {
        let body = json!({"account_id": account.id, "date": "2026-10-04", "amount": -1000, "category_id": b.groceries});
        let txn = server.create_transaction(&b.plan, &body);
        let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"category_id": null}));
        assert_error(&res, 400, &format!("PATCH category_id null in {account:?}"));
        assert_eq!(server.transaction(&b.plan, &id_of(&txn))["category_id"], json!(b.groceries));
    }
}

#[test]
fn spec_8_6_category_rules_of_8_2_apply() {
    let server = Server::start();
    let b = Budget::new(&server);
    let spend = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    for bad in [json!(b.visa_payment), json!("nope"), json!(b.group), json!(5)] {
        let res = server.patch_transaction(&b.plan, &spend, &json!({"category_id": bad}));
        assert_error(&res, 400, &format!("PATCH category_id {bad}"));
    }
    let tracking = id_of(&server.create_transaction(&b.plan, &json!({"account_id": b.house.id, "date": "2026-10-04", "amount": 1})));
    for bad in [json!(b.groceries), json!("ready_to_assign"), json!(b.visa_payment)] {
        let res = server.patch_transaction(&b.plan, &tracking, &json!({"category_id": bad}));
        assert_error(&res, 400, &format!("PATCH tracking category_id {bad}"));
    }
    let got = server.patch_transaction_ok(&b.plan, &tracking, &json!({"category_id": null}));
    assert_eq!(got["category_id"], Value::Null);
}

#[test]
fn spec_8_6_rejects_invalid_values() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    let id = id_of(&txn);
    for bad in [
        json!({"date": "2026-02-30"}),
        json!({"date": null}),
        json!({"amount": 1.5}),
        json!({"amount": null}),
        json!({"amount": "5"}),
        json!({"memo": "x".repeat(501)}),
        json!({"cleared": "Cleared"}),
        json!({"cleared": null}),
        json!({"approved": null}),
        json!({"approved": "false"}),
        json!({"flag_color": "pink"}),
        json!({"payee_id": "nope"}),
        json!({"payee_name": ""}),
        json!({"payee_name": 5}),
        json!({"subtransactions": null}),
    ] {
        let res = server.patch_transaction(&b.plan, &id, &bad);
        assert_error(&res, 400, &format!("PATCH {}", bad.to_string().chars().take(60).collect::<String>()));
        assert_eq!(spec_fields(&server.transaction(&b.plan, &id)), spec_fields(&txn), "after PATCH {bad}");
    }
}

#[test]
fn spec_8_6_becoming_a_transfer_clears_the_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"payee_id": b.savings.payee}));
    assert_matches(&got, &json!({"category_id": null, "transfer_account_id": b.savings.id}), "PATCH payee to transfer");
    assert_eq!(server.transaction(&b.plan, &id)["category_id"], Value::Null);
}

#[test]
fn spec_8_6_becoming_a_transfer_with_a_category_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    let id = id_of(&txn);
    for category in [json!(b.groceries), json!(b.household), json!("ready_to_assign")] {
        let res = server.patch_transaction(&b.plan, &id, &json!({"payee_id": b.savings.payee, "category_id": category}));
        assert_error(&res, 400, &format!("PATCH to transfer with category {category}"));
    }
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id)), spec_fields(&txn));
    assert_eq!(server.transactions(&b.plan).len(), 1);
}

#[test]
fn spec_8_6_becoming_a_split_clears_the_category() {
    let server = Server::start();
    let b = Budget::new(&server);
    let id = id_of(&server.create_transaction(&b.plan, &b.spend(-1000)));
    let subs = json!([{"amount": -400, "category_id": b.groceries}, {"amount": -600, "category_id": b.household}]);
    let got = server.patch_transaction_ok(&b.plan, &id, &json!({"subtransactions": subs}));
    assert_eq!(got["category_id"], Value::Null, "{got}");
    assert_eq!(got["subtransactions"].as_array().map(Vec::len), Some(2), "{got}");
}

#[test]
fn spec_8_6_becoming_a_split_with_a_category_returns_400() {
    let server = Server::start();
    let b = Budget::new(&server);
    let txn = server.create_transaction(&b.plan, &b.spend(-1000));
    let subs = json!([{"amount": -400, "category_id": b.groceries}, {"amount": -600, "category_id": b.household}]);
    let res = server.patch_transaction(&b.plan, &id_of(&txn), &json!({"subtransactions": subs, "category_id": b.groceries}));
    assert_error(&res, 400, "PATCH to split with category");
    assert_eq!(spec_fields(&server.transaction(&b.plan, &id_of(&txn))), spec_fields(&txn));
}

#[test]
fn spec_8_6_missing_transaction_returns_404() {
    let server = Server::start();
    let b = Budget::new(&server);
    let res = server.patch_transaction(&b.plan, "nope", &json!({"memo": "x"}));
    assert_error(&res, 404, "PATCH transactions/nope");
    let res = server.patch_transaction(&b.plan, &b.checking.id, &json!({"memo": "x"}));
    assert_error(&res, 404, "PATCH transactions/<account id>");
}

// ---- 8.7 ----

#[test]
fn spec_8_7_deletes_transaction() {
    let server = Server::start();
    let b = Budget::new(&server);
    let keep = id_of(&server.create_transaction(&b.plan, &b.spend(-1)));
    let gone = id_of(&server.create_transaction(&b.plan, &b.spend(-2)));
    let res = server.delete_transaction(&b.plan, &gone);
    assert_status(&res, 204, "DELETE transaction");
    assert!(res.body.is_empty(), "DELETE has a body: {:?}", res.text());
    assert_error(&server.get_transaction(&b.plan, &gone), 404, "GET deleted transaction");
    assert_eq!(ids_of(&server.transactions(&b.plan)), vec![keep]);
    assert_error(&server.delete_transaction(&b.plan, &gone), 404, "DELETE deleted transaction again");
    assert_error(&server.patch_transaction(&b.plan, &gone, &json!({"memo": "x"})), 404, "PATCH deleted transaction");
}

#[test]
fn spec_8_7_deletes_starting_balance_transaction() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = id_of(&server.add_account(&plan, "Checking", "checking", 2500000));
    let txns = server.transactions(&plan);
    assert_eq!(txns.len(), 1, "{txns:?}");
    assert_status(&server.delete_transaction(&plan, &id_of(&txns[0])), 204, "DELETE starting balance");
    assert!(server.transactions(&plan).is_empty());
    assert_eq!(server.balances(&plan, &account), (0, 0, 0));
}

#[test]
fn spec_8_7_missing_transaction_returns_404() {
    let server = Server::start();
    let b = Budget::new(&server);
    server.create_transaction(&b.plan, &b.spend(-1));
    for id in ["nope", b.checking.id.as_str(), b.groceries.as_str(), b.checking.payee.as_str()] {
        assert_error(&server.delete_transaction(&b.plan, id), 404, &format!("DELETE transactions/{id}"));
    }
    assert_eq!(server.transactions(&b.plan).len(), 1);
}
