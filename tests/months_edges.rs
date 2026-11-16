//! More of SPEC section 11 for plans without credit accounts: lines that come from
//! transfer halves, unapproved or uncleared transactions and the ends of the date range,
//! assignments that stay with a moved category or go with a deleted credit account, later
//! overspending against Ready to Assign, and concurrent changes.

use mybudget_testsuite::{
    CashBudget, Server, Value, assert_error, assert_matches, assert_status, entry, entry_path, id_of, json,
    str_field,
};

const RTA: &str = "ready_to_assign";
const MAX: i64 = 1_000_000_000_000;

/// The Ready to Assign fields of a month (11.6, 11.7).
fn rta(month: &str, income: i64, assigned: i64, cos_last: i64, left_over: i64, in_future: i64, ready: i64) -> Value {
    json!({"month": month, "income": income, "cash_advances": 0, "assigned": assigned,
           "cash_overspending_last_month": cos_last, "left_over_from_last_month": left_over,
           "assigned_in_future": in_future, "ready_to_assign": ready})
}

/// The month must have `expected`'s fields.
fn check_month(b: &CashBudget, server: &Server, expected: &Value, what: &str) {
    let month = expected["month"].as_str().unwrap();
    assert_matches(&b.month(server, month), expected, &format!("{what}: {month}"));
}

/// The entry for `category` in the month's `categories`.
fn entry_in(month: &Value, category: &str) -> Value {
    let list = month["categories"].as_array().unwrap_or_else(|| panic!("`categories` isn't an array: {month}"));
    let found: Vec<&Value> = list.iter().filter(|e| e["category_id"] == category).collect();
    assert_eq!(found.len(), 1, "month lists {category} {} times: {month}", found.len());
    found[0].clone()
}

// ---- 3.11 ----

#[test]
fn spec_3_11_concurrent_assignments_and_income_all_count() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let categories: Vec<String> =
        (0..8).map(|i| id_of(&server.create_category(&b.plan, &b.group, &format!("Envelope {i}")))).collect();
    std::thread::scope(|s| {
        for (i, category) in categories.iter().enumerate() {
            let (server, b) = (&server, &b);
            s.spawn(move || {
                for m in 1..=6 {
                    let month = format!("2027-{m:02}");
                    let amount = (i as i64 + 1) * 1000 + m;
                    let res = server.patch(&entry_path(&b.plan, &month, category), &json!({"assigned": amount}));
                    assert_status(&res, 200, &format!("PATCH {month} {i}"));
                    assert_eq!(res.json()["assigned"], amount, "PATCH {month} {i}");
                    let res = server.post_transaction(
                        &b.plan,
                        &json!({"account_id": b.checking.id, "date": format!("{month}-{:02}", i + 1),
                                "amount": 10000, "category_id": RTA}),
                    );
                    assert_status(&res, 201, &format!("POST {month} {i}"));
                }
            });
        }
    });
    for m in 1..=6i64 {
        let month = format!("2027-{m:02}");
        let got = b.month(&server, &month);
        assert_matches(&got, &json!({"income": 80000, "assigned": 36000 + 8 * m}), &month);
        for (i, category) in categories.iter().enumerate() {
            assert_eq!(entry_in(&got, category)["assigned"], (i as i64 + 1) * 1000 + m, "{month} {i}");
        }
    }
}

// ---- 5.9 ----

#[test]
fn spec_5_9_deleting_a_credit_account_drops_its_payment_assignments() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 100000);
    let visa = server.add_account(&b.plan, "Visa", "creditCard", 0);
    let payment = str_field(&visa, "payment_category_id");
    for (month, amount) in [("2026-10", 300000), ("2026-12", 200000)] {
        let res = server.patch(&entry_path(&b.plan, month, &payment), &json!({"assigned": amount}));
        assert_status(&res, 200, &format!("assigning to the payment category in {month}"));
    }
    let res = server.delete(&format!("/plans/{}/accounts/{}", b.plan, id_of(&visa)));
    assert_status(&res, 204, "DELETE Visa");
    // The plan has no credit account now, and the payment category's assignments are gone.
    let categories = |groceries: Value| json!([groceries, entry(&b.household, 0, 0, 0, 0)]);
    let mut october = rta("2026-10", 1000000, 100000, 0, 0, 0, 900000);
    october["activity"] = json!(0);
    october["categories"] = categories(entry(&b.groceries, 100000, 0, 100000, 0));
    check_month(&b, &server, &october, "after deleting Visa");
    for month in ["2026-11", "2026-12", "2027-01"] {
        let mut expected = rta(month, 0, 0, 0, 900000, 0, 900000);
        expected["categories"] = categories(entry(&b.groceries, 0, 0, 100000, 0));
        check_month(&b, &server, &expected, "after deleting Visa");
    }
    for month in ["2026-10", "2026-12"] {
        let path = entry_path(&b.plan, month, &payment);
        assert_error(&server.get(&path), 404, &format!("GET the deleted payment category's {month}"));
        assert_error(&server.patch(&path, &json!({"assigned": 5})), 404, &format!("PATCH {month}"));
    }
}

// ---- 11.1 ----

#[test]
fn spec_11_1_unapproved_and_uncleared_transactions_are_lines() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let post = |body: Value| {
        server.create_transaction(&b.plan, &body);
    };
    post(json!({"account_id": b.checking.id, "date": "2026-10-01", "amount": 500000, "category_id": RTA,
                "approved": false, "cleared": "uncleared"}));
    post(json!({"account_id": b.savings.id, "date": "2026-10-02", "amount": 200000, "category_id": RTA,
                "cleared": "reconciled", "flag_color": "red"}));
    post(json!({"account_id": b.wallet.id, "date": "2026-10-03", "amount": -30000, "category_id": b.groceries,
                "approved": false}));
    post(json!({"account_id": b.checking.id, "date": "2026-10-04", "amount": -20000, "category_id": b.household,
                "approved": false, "cleared": "uncleared", "flag_color": "purple"}));
    post(json!({"account_id": b.checking.id, "date": "2026-10-05", "amount": -15000, "approved": false,
                "cleared": "uncleared",
                "subtransactions": [{"amount": -10000, "category_id": b.groceries},
                                    {"amount": -5000, "category_id": b.household}]}));
    let expected = json!({"income": 700000, "activity": -65000,
                          "categories": [entry(&b.groceries, 0, -40000, -40000, 40000),
                                         entry(&b.household, 0, -25000, -25000, 25000)]});
    assert_matches(&b.month(&server, "2026-10"), &expected, "October");
    assert_matches(&b.month(&server, "2026-11"), &json!({"cash_overspending_last_month": 65000}), "November");
}

#[test]
fn spec_11_1_lines_at_the_ends_of_the_date_range_count() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "1900-01-01", MAX, RTA);
    b.txn(&server, &b.checking, "1900-01-31", -MAX, &b.household);
    b.txn(&server, &b.savings, "2999-12-31", MAX, RTA);
    b.txn(&server, &b.wallet, "2999-12-31", -MAX, &b.groceries);
    let mut first = rta("1900-01", MAX, 0, 0, 0, 0, MAX);
    first["activity"] = json!(-MAX);
    first["categories"] = json!([entry(&b.groceries, 0, 0, 0, 0), entry(&b.household, 0, -MAX, -MAX, MAX)]);
    check_month(&b, &server, &first, "range ends");
    check_month(&b, &server, &rta("1900-02", 0, 0, MAX, MAX, 0, 0), "range ends");
    check_month(&b, &server, &rta("2999-11", 0, 0, 0, 0, 0, 0), "range ends");
    let mut last = rta("2999-12", MAX, 0, 0, 0, 0, MAX);
    last["activity"] = json!(-MAX);
    last["categories"] = json!([entry(&b.groceries, 0, -MAX, -MAX, MAX), entry(&b.household, 0, 0, 0, 0)]);
    check_month(&b, &server, &last, "range ends");
}

#[test]
fn spec_11_1_transfer_lines_follow_the_other_halfs_date_and_amount() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    // Selling part of the house into Checking, entered on the house's side.
    let sale = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.house.id, "date": "2026-10-03", "amount": -500000, "payee_id": b.checking.payee,
                "category_id": RTA}),
    );
    let (sale, deposit) = (id_of(&sale), str_field(&sale, "transfer_transaction_id"));
    // Paying the mortgage from Savings, out of Household.
    let payment = server.create_transaction(
        &b.plan,
        &json!({"account_id": b.savings.id, "date": "2026-10-04", "amount": -200000, "payee_id": b.mortgage.payee,
                "category_id": b.household}),
    );
    let mortgage_half = str_field(&payment, "transfer_transaction_id");
    let check = |what: &str, months: [(&str, i64, i64); 4]| {
        for (month, income, household) in months {
            let m = b.month(&server, month);
            assert_matches(&m, &json!({"income": income, "activity": household}), &format!("{what}: {month}"));
            assert_eq!(entry_in(&m, &b.household)["activity"], household, "{what}: {month} household");
        }
    };
    check("created", [("2026-09", 0, 0), ("2026-10", 500000, -200000), ("2026-11", 0, 0), ("2026-12", 0, 0)]);
    server.patch_transaction_ok(&b.plan, &sale, &json!({"date": "2026-11-20", "amount": -300000}));
    check("house half moved", [("2026-09", 0, 0), ("2026-10", 0, -200000), ("2026-11", 300000, 0), ("2026-12", 0, 0)]);
    server.patch_transaction_ok(&b.plan, &deposit, &json!({"date": "2026-12-01"}));
    check("checking half moved", [("2026-09", 0, 0), ("2026-10", 0, -200000), ("2026-11", 0, 0), ("2026-12", 300000, 0)]);
    server.patch_transaction_ok(&b.plan, &mortgage_half, &json!({"amount": 250000}));
    check("mortgage half's amount", [("2026-09", 0, 0), ("2026-10", 0, -250000), ("2026-11", 0, 0), ("2026-12", 300000, 0)]);
    server.patch_transaction_ok(&b.plan, &mortgage_half, &json!({"date": "2026-09-30"}));
    check("mortgage half's date", [("2026-09", 0, -250000), ("2026-10", 0, 0), ("2026-11", 0, 0), ("2026-12", 300000, 0)]);
    assert_status(&server.delete_transaction(&b.plan, &mortgage_half), 204, "DELETE the mortgage half");
    check("mortgage half deleted", [("2026-09", 0, 0), ("2026-10", 0, 0), ("2026-11", 0, 0), ("2026-12", 300000, 0)]);
}

#[test]
fn spec_11_1_moving_a_tracking_transfers_on_budget_half_moves_its_line() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let sale = id_of(&server.create_transaction(
        &b.plan,
        &json!({"account_id": b.house.id, "date": "2026-10-03", "amount": -500000, "payee_id": b.checking.payee,
                "category_id": RTA}),
    ));
    let check = |what: &str, income: i64, groceries: i64| {
        let m = b.month(&server, "2026-10");
        assert_matches(&m, &json!({"income": income, "activity": groceries}), what);
        assert_eq!(entry_in(&m, &b.groceries)["activity"], groceries, "{what}: groceries");
    };
    check("into Checking", 500000, 0);
    server.patch_transaction_ok(&b.plan, &sale, &json!({"category_id": b.groceries}));
    check("recategorized from the house half", 0, 500000);
    server.patch_transaction_ok(&b.plan, &sale, &json!({"payee_id": b.savings.payee}));
    check("into Savings, keeping the category", 0, 500000);
    server.patch_transaction_ok(&b.plan, &sale, &json!({"payee_id": b.mortgage.payee}));
    check("into the mortgage", 0, 0);
    server.patch_transaction_ok(&b.plan, &sale, &json!({"payee_id": b.wallet.payee, "category_id": RTA}));
    check("into the wallet", 500000, 0);
    server.patch_transaction_ok(&b.plan, &sale, &json!({"payee_id": null}));
    check("no longer a transfer", 0, 0);
}

// ---- 11.2 ----

#[test]
fn spec_11_2_assignments_stay_with_a_moved_category() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 400000);
    b.txn(&server, &b.checking, "2026-10-04", -120000, &b.groceries);
    b.assign(&server, "2026-11", &b.groceries, 50000);
    let bills = id_of(&server.create_group(&b.plan, "Bills"));
    let g = &b.groceries;
    let h = &b.household;
    for (group, what) in [(&bills, "moved to Bills"), (&b.group, "moved back")] {
        server.patch_ok(
            &format!("/plans/{}/categories/{g}", b.plan),
            &json!({"group_id": group, "name": "Food", "note": "moved"}),
        );
        let mut october = rta("2026-10", 1000000, 400000, 0, 0, 50000, 550000);
        october["activity"] = json!(-120000);
        october["categories"] = json!([entry(h, 0, 0, 0, 0), entry(g, 400000, -120000, 280000, 0)]);
        check_month(&b, &server, &october, what);
        let mut november = rta("2026-11", 0, 50000, 0, 600000, 0, 550000);
        november["categories"] = json!([entry(h, 0, 0, 0, 0), entry(g, 50000, 0, 330000, 0)]);
        check_month(&b, &server, &november, what);
        assert_matches(&b.entry(&server, "2026-12", g), &entry(g, 0, 0, 330000, 0), what);
    }
}

// ---- 11.7 ----

#[test]
fn spec_11_7_later_overspending_does_not_hold_back_earlier_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.txn(&server, &b.checking, "2026-12-10", -300000, &b.groceries);
    // December's overspending comes out of January, and only January onwards.
    for expected in [
        rta("2026-10", 1000000, 0, 0, 0, 0, 1000000),
        rta("2026-11", 0, 0, 0, 1000000, 0, 1000000),
        rta("2026-12", 0, 0, 0, 1000000, 0, 1000000),
        rta("2027-01", 0, 0, 300000, 1000000, 0, 700000),
        rta("2027-02", 0, 0, 0, 700000, 0, 700000),
    ] {
        check_month(&b, &server, &expected, "overspending in December");
    }
    // Assigning ahead holds money back, but the overspending doesn't add to the shortfall.
    b.assign(&server, "2027-02", &b.household, 800000);
    for expected in [
        rta("2026-10", 1000000, 0, 0, 0, 800000, 200000),
        rta("2026-11", 0, 0, 0, 1000000, 800000, 200000),
        rta("2026-12", 0, 0, 0, 1000000, 800000, 200000),
        rta("2027-01", 0, 0, 300000, 1000000, 700000, 0),
        rta("2027-02", 0, 800000, 0, 700000, 0, -100000),
        rta("2027-03", 0, 0, 0, -100000, 0, -100000),
    ] {
        check_month(&b, &server, &expected, "with February assigned");
    }
}
