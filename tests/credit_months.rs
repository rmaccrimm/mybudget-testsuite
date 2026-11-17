//! SPEC section 11 for plans with credit accounts: lines in credit accounts (11.1), payment
//! categories' assignments and entries (11.2, 11.3), the months' fields (11.6) and Ready to
//! Assign with cash advances and payment categories (11.7).

use mybudget_testsuite::{
    Card, CreditBudget, Server, Value, assert_error, assert_matches, assert_month_identity, assert_status,
    credit_entry as e, entry_path, id_of, json, zero_entry as zero,
};

const RTA: &str = "ready_to_assign";

/// The `category_id`s of the month's `categories`, in order.
fn category_ids(month: &Value) -> Vec<String> {
    month["categories"]
        .as_array()
        .unwrap_or_else(|| panic!("`categories` isn't an array: {month}"))
        .iter()
        .map(|e| e["category_id"].as_str().unwrap_or_else(|| panic!("entry without a string category_id: {e}")).to_owned())
        .collect()
}

/// The month must have `expected`'s fields and satisfy 11.7's identity.
fn check_totals(b: &CreditBudget, server: &Server, expected: &Value, what: &str) {
    let month = expected["month"].as_str().unwrap();
    let got = b.month(server, month);
    assert_matches(&got, expected, &format!("{what}: {month}"));
    assert_month_identity(&got);
}

// ---- 4.5 ----

#[test]
fn spec_4_5_payment_category_months_of_other_plans_return_404() {
    let server = Server::start();
    let a = CreditBudget::new(&server);
    let b = CreditBudget::new(&server);
    let path = entry_path(&b.plan, "2026-10", &a.visa_payment);
    assert_error(&server.get(&path), 404, "GET another plan's payment category");
    assert_error(&server.patch(&path, &json!({"assigned": 1000})), 404, "PATCH another plan's payment category");
    assert_eq!(a.entry(&server, "2026-10", &a.visa_payment)["assigned"], 0);
}

// ---- 7.8 ----

#[test]
fn spec_7_8_hidden_payment_categories_take_part_in_months() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    server.patch_ok(&format!("/plans/{}/categories/{vp}", b.plan), &json!({"hidden": true}));
    server.patch_ok(&format!("/plans/{}/categories/{g}", b.plan), &json!({"hidden": true}));
    b.txn(&server, &b.checking, "2026-10-01", 500000, RTA);
    b.assign(&server, "2026-10", g, 100000);
    b.assign(&server, "2026-10", vp, 20000);
    b.txn(&server, &b.visa, "2026-10-05", -60000, g);
    let october = b.check(
        &server,
        "2026-10",
        [e(vp, 20000, 60000, 80000, 0, 0), zero(lp), e(g, 100000, -60000, 40000, 0, 0), zero(h)],
        "hidden",
    );
    assert_matches(&october, &json!({"assigned": 120000, "activity": -60000, "ready_to_assign": 380000}), "hidden");
}

// ---- 11.1 ----

#[test]
fn spec_11_1_lines_in_credit_accounts_count() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.visa, "2026-10-05", -30000, g);
    b.txn(&server, &b.line, "2026-10-06", -20000, g);
    b.txn(&server, &b.checking, "2026-10-07", -10000, g);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-08", "amount": -15000,
                "subtransactions": [{"amount": -5000, "category_id": g}, {"amount": -10000, "category_id": h}]}),
    );
    let october = b.check(
        &server,
        "2026-10",
        [zero(vp), zero(lp), e(g, 0, -65000, -65000, 10000, 55000), e(h, 0, -10000, -10000, 0, 10000)],
        "credit lines",
    );
    assert_matches(&october, &json!({"activity": -75000, "income": 0}), "October");
}

#[test]
fn spec_11_1_lines_in_closed_credit_accounts_count() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    b.txn(&server, &b.visa, "2026-10-05", -50000, g);
    b.transfer(&server, &b.checking, &b.visa, "2026-10-20", -50000);
    let closed = server.patch_ok(&format!("/plans/{}/accounts/{}", b.plan, b.visa.id), &json!({"closed": true}));
    assert_eq!(closed["closed"], true);
    b.txn(&server, &b.visa, "2026-10-25", -10000, g);
    b.check(&server, "2026-10", [e(vp, 0, 10000, 10000, 0, 0), zero(lp), e(g, 100000, -60000, 40000, 0, 0), zero(h)], "closed");
}

#[test]
fn spec_11_1_unapproved_and_uncleared_credit_lines_count() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 30000);
    let post = |body: Value| {
        server.create_transaction(&b.plan, &body);
    };
    post(json!({"account_id": b.visa.id, "date": "2026-10-05", "amount": -30000, "category_id": g,
                "approved": false, "cleared": "uncleared"}));
    post(json!({"account_id": b.line.id, "date": "2026-10-06", "amount": -10000, "category_id": h,
                "approved": false, "flag_color": "red"}));
    post(json!({"account_id": b.checking.id, "date": "2026-10-07", "amount": -20000, "payee_id": b.visa.payee,
                "approved": false, "cleared": "reconciled"}));
    post(json!({"account_id": b.wallet.id, "date": "2026-10-08", "amount": 5000, "payee_id": b.line.payee,
                "approved": false}));
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 10000, 10000, 0, 0), zero(lp), e(g, 30000, -30000, 0, 0, 0), e(h, 0, -10000, -10000, 0, 10000)],
        "unapproved",
    );
    assert_eq!(b.month(&server, "2026-10")["cash_advances"], 5000);
}

// ---- 11.2 ----

#[test]
fn spec_11_2_payment_categories_take_assignments() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g) = (&b.visa_payment, &b.line_payment, &b.groceries);
    b.assign(&server, "2026-10", g, 40000);
    b.txn(&server, &b.visa, "2026-10-05", -40000, g);
    assert_matches(&b.assign(&server, "2026-10", vp, 25000), &e(vp, 25000, 40000, 65000, 0, 0), "PATCH Visa's October");
    assert_matches(&b.assign(&server, "2026-11", vp, -5000), &e(vp, -5000, 0, 60000, 0, 0), "PATCH Visa's November");
    assert_matches(&b.assign(&server, "2026-10", lp, -1000), &e(lp, -1000, 0, -1000, 1000, 0), "PATCH Credit Line's October");
    assert_matches(&b.assign(&server, "2026-10", vp, 0), &e(vp, 0, 40000, 40000, 0, 0), "PATCH Visa's October back to 0");
    assert_matches(&b.entry(&server, "2026-11", vp), &e(vp, -5000, 0, 35000, 0, 0), "GET Visa's November");
}

#[test]
fn spec_11_2_payment_category_assigned_must_be_an_amount() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let path = entry_path(&b.plan, "2026-10", &b.visa_payment);
    for body in [
        json!({"assigned": null}),
        json!({"assigned": 1.5}),
        json!({"assigned": "1000"}),
        json!({"assigned": 1000000000001i64}),
        json!({"assigned": true}),
    ] {
        assert_error(&server.patch(&path, &body), 400, &format!("PATCH {body}"));
    }
    assert_matches(&b.entry(&server, "2026-10", &b.visa_payment), &zero(&b.visa_payment), "unchanged");
    let res = server.patch(&path, &json!({"assigned": -1000000000000i64}));
    assert_status(&res, 200, "PATCH the smallest amount");
    assert_eq!(res.json()["assigned"], -1000000000000i64);
}

// ---- 11.3 ----

#[test]
fn spec_11_3_activity_includes_lines_in_credit_accounts() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let g = &b.groceries;
    b.assign(&server, "2026-10", g, 500000);
    b.txn(&server, &b.visa, "2026-10-03", -120000, g);
    b.txn(&server, &b.line, "2026-10-04", -30000, g);
    b.txn(&server, &b.visa, "2026-10-05", 10000, g);
    b.txn(&server, &b.wallet, "2026-10-06", -5000, g);
    b.txn(&server, &b.visa, "2026-11-01", -7000, g);
    assert_matches(&b.entry(&server, "2026-10", g), &e(g, 500000, -145000, 355000, 0, 0), "October");
    assert_matches(&b.entry(&server, "2026-11", g), &e(g, 0, -7000, 348000, 0, 0), "November");
}

#[test]
fn spec_11_3_entries_are_the_same_as_in_the_month() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", g, 100000);
    b.assign(&server, "2026-11", lp, 30000);
    b.txn(&server, &b.visa, "2026-10-03", -150000, g);
    b.txn(&server, &b.line, "2026-10-04", 20000, h);
    b.txn(&server, &b.checking, "2026-11-04", -5000, h);
    b.transfer(&server, &b.savings, &b.line, "2026-11-20", -60000);
    for month in ["2026-09", "2026-10", "2026-11", "2026-12"] {
        let listed = b.month(&server, month);
        let list = listed["categories"].as_array().unwrap();
        assert_eq!(category_ids(&listed), vec![vp.clone(), lp.clone(), g.clone(), h.clone()], "{month}");
        for entry in list {
            let category = entry["category_id"].as_str().unwrap();
            assert_eq!(&b.entry(&server, month, category), entry, "{month} {category}");
        }
    }
}

// ---- 11.5 ----

#[test]
fn spec_11_5_credit_overspending_is_zero_without_credit_spending() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-04", -120000, g);
    b.txn(&server, &b.visa, "2026-10-05", 20000, h);
    b.assign(&server, "2026-10", h, -50000);
    b.txn(&server, &b.visa, "2026-11-05", -30000, h);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, -20000, -20000, 20000, 0), zero(lp), e(g, 0, -120000, -120000, 120000, 0), e(h, -50000, 20000, -30000, 30000, 0)],
        "October",
    );
    b.check(&server, "2026-11", [zero(vp), zero(lp), zero(g), e(h, 0, -30000, -30000, 0, 30000)], "November");
}

// ---- 11.6 ----

#[test]
fn spec_11_6_payment_categories_come_first() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let everyday = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &everyday, "Groceries"));
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let rent = id_of(&server.create_category(&plan, &bills, "Rent"));
    let visa = Card::from(&server.add_account(&plan, "Visa", "creditCard", 0));
    server.add_account(&plan, "Checking", "checking", 0);
    let line = Card::from(&server.add_account(&plan, "Credit Line", "lineOfCredit", 0));
    let ids = |month: &str| category_ids(&server.month(&plan, month));
    assert_eq!(ids("2026-10"), vec![visa.payment.clone(), line.payment.clone(), groceries.clone(), rent.clone()]);
    let amex = Card::from(&server.add_account(&plan, "Amex", "creditCard", 0));
    let all = vec![visa.payment.clone(), line.payment.clone(), amex.payment.clone(), groceries.clone(), rent.clone()];
    assert_eq!(ids("2026-10"), all);
    server.patch_ok(&format!("/plans/{plan}/accounts/{}", visa.id), &json!({"name": "Zeta"}));
    server.patch_ok(&format!("/plans/{plan}/categories/{}", visa.payment), &json!({"hidden": true}));
    assert_eq!(ids("2026-11"), all, "after renaming and hiding Visa's");
    assert_status(&server.delete(&format!("/plans/{plan}/accounts/{}", line.id)), 204, "DELETE Credit Line");
    assert_eq!(ids("2026-10"), vec![visa.payment.clone(), amex.payment.clone(), groceries.clone(), rent.clone()]);
    assert_matches(
        &server.month(&plan, "2026-10"),
        &json!({"categories": [zero(&visa.payment), zero(&amex.payment), zero(&groceries), zero(&rent)]}),
        "entries",
    );
}

#[test]
fn spec_11_6_income_leaves_out_credit_accounts() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let store = server.create_account(
        &b.plan,
        &json!({"name": "Store Card", "type": "creditCard", "starting_balance": -350000, "starting_date": "2026-10-01"}),
    );
    assert_eq!(store["balance"], -350000);
    b.txn(&server, &b.checking, "2026-10-01", 100000, RTA);
    b.txn(&server, &b.visa, "2026-10-05", 20000, RTA);
    b.txn(&server, &b.line, "2026-10-06", -7000, RTA);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-07", "amount": 4000,
                "subtransactions": [{"amount": 5000, "category_id": RTA}, {"amount": -1000, "category_id": b.groceries}]}),
    );
    b.txn(&server, &b.savings, "2026-10-08", 30000, RTA);
    // A cash advance brings money into Checking, but as a cash advance, not income.
    b.transfer(&server, &b.visa, &b.checking, "2026-10-09", -40000);
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-10", "income": 130000, "cash_advances": 40000, "activity": -1000,
                "ready_to_assign": 170000}),
        "income",
    );
}

#[test]
fn spec_11_6_activity_leaves_out_payment_categories() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    b.txn(&server, &b.visa, "2026-10-05", -60000, g);
    b.txn(&server, &b.checking, "2026-10-06", -10000, h);
    b.transfer(&server, &b.checking, &b.line, "2026-10-07", -30000);
    let october = b.check(
        &server,
        "2026-10",
        [e(vp, 0, 60000, 60000, 0, 0), e(lp, 0, -30000, -30000, 30000, 0), e(g, 100000, -60000, 40000, 0, 0),
         e(h, 0, -10000, -10000, 10000, 0)],
        "activity",
    );
    assert_eq!(october["activity"], -70000, "{october}");
}

#[test]
fn spec_11_6_assigned_includes_payment_categories() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.visa_payment, 200000);
    b.assign(&server, "2026-10", &b.line_payment, 50000);
    b.assign(&server, "2026-10", &b.groceries, 100000);
    b.assign(&server, "2026-11", &b.line_payment, -20000);
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-10", "income": 1000000, "assigned": 350000, "activity": 0,
                "assigned_in_future": 0, "ready_to_assign": 650000}),
        "assigned",
    );
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-11", "assigned": -20000, "left_over_from_last_month": 650000,
                "ready_to_assign": 670000}),
        "assigned",
    );
}

#[test]
fn spec_11_6_cash_overspending_last_month_counts_payment_categories_not_credit_overspending() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.transfer(&server, &b.checking, &b.visa, "2026-10-20", -40000);
    b.txn(&server, &b.visa, "2026-10-21", -70000, &b.groceries);
    b.txn(&server, &b.wallet, "2026-10-22", -5000, &b.household);
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-11", "cash_overspending_last_month": 45000, "left_over_from_last_month": 1000000,
                "ready_to_assign": 955000}),
        "overspending",
    );
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-12", "cash_overspending_last_month": 0, "left_over_from_last_month": 955000,
                "ready_to_assign": 955000}),
        "overspending",
    );
}

// ---- 11.7 ----

#[test]
fn spec_11_7_cash_advances_count_toward_later_assignments() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 500000, RTA);
    b.assign(&server, "2026-11", &b.groceries, 300000);
    b.transfer(&server, &b.visa, &b.checking, "2026-11-10", -200000);
    let month = |month: &str, income: i64, advances: i64, assigned: i64, left_over: i64, in_future: i64, ready: i64| {
        json!({"month": month, "income": income, "cash_advances": advances, "assigned": assigned,
               "cash_overspending_last_month": 0, "left_over_from_last_month": left_over,
               "assigned_in_future": in_future, "ready_to_assign": ready})
    };
    for expected in [
        month("2026-09", 0, 0, 0, 0, 0, 0),
        month("2026-10", 500000, 0, 0, 0, 100000, 400000),
        month("2026-11", 0, 200000, 300000, 500000, 0, 400000),
        month("2026-12", 0, 0, 0, 400000, 0, 400000),
    ] {
        check_totals(&b, &server, &expected, "advance");
    }
}

#[test]
fn spec_11_7_cash_advances_cover_over_assigning() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 100000);
    b.transfer(&server, &b.line, &b.wallet, "2026-11-02", -150000);
    for expected in [
        json!({"month": "2026-10", "cash_advances": 0, "assigned": 100000, "assigned_in_future": 0, "ready_to_assign": -100000}),
        json!({"month": "2026-11", "cash_advances": 150000, "left_over_from_last_month": -100000, "ready_to_assign": 50000}),
        json!({"month": "2026-12", "cash_advances": 0, "left_over_from_last_month": 50000, "ready_to_assign": 50000}),
    ] {
        check_totals(&b, &server, &expected, "advance");
    }
}

#[test]
fn spec_11_7_assigning_ahead_to_a_payment_category_holds_money_back() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 500000, RTA);
    b.assign(&server, "2026-11", &b.visa_payment, 200000);
    for expected in [
        json!({"month": "2026-10", "income": 500000, "assigned": 0, "left_over_from_last_month": 0,
               "assigned_in_future": 200000, "ready_to_assign": 300000}),
        json!({"month": "2026-11", "income": 0, "assigned": 200000, "left_over_from_last_month": 500000,
               "assigned_in_future": 0, "ready_to_assign": 300000}),
    ] {
        check_totals(&b, &server, &expected, "ahead");
    }
}

#[test]
fn spec_11_7_payment_category_overspending_comes_out_of_the_next_month() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.transfer(&server, &b.savings, &b.line, "2026-10-15", -60000);
    b.assign(&server, "2026-12", &b.groceries, 100000);
    for expected in [
        json!({"month": "2026-10", "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
               "assigned_in_future": 100000, "ready_to_assign": 900000}),
        json!({"month": "2026-11", "cash_overspending_last_month": 60000, "left_over_from_last_month": 1000000,
               "assigned_in_future": 100000, "ready_to_assign": 840000}),
        json!({"month": "2026-12", "cash_overspending_last_month": 0, "left_over_from_last_month": 940000,
               "assigned_in_future": 0, "ready_to_assign": 840000}),
    ] {
        check_totals(&b, &server, &expected, "overpaid");
    }
}
