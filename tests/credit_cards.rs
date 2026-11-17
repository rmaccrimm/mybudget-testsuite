//! SPEC section 12, credit cards: payment categories' entries (12.1), funding (12.2),
//! credit overspending (12.3), payment category activity (12.4), payments and cash advances
//! (12.5), what changes nothing (12.6) and the example (12.7).

use mybudget_testsuite::{
    Card, CreditBudget, MONTH_ENTRY_FIELDS, MONTH_FIELDS, Server, Value, assert_fields, assert_matches,
    assert_month_identity, assert_status, credit_entry as e, id_of, json, str_field, zero_entry as zero,
};

const RTA: &str = "ready_to_assign";
const MAX: i64 = 1_000_000_000_000;

/// The month must have `expected`'s fields and satisfy 11.7's identity.
fn check_totals(b: &CreditBudget, server: &Server, expected: &Value, what: &str) {
    let month = expected["month"].as_str().unwrap();
    let got = b.month(server, month);
    assert_matches(&got, expected, &format!("{what}: {month}"));
    assert_month_identity(&got);
}

// ---- 12.1 ----

#[test]
fn spec_12_1_payment_category_entries_have_every_field() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.assign(&server, "2026-10", &b.groceries, 50000);
    b.txn(&server, &b.visa, "2026-10-05", -80000, &b.groceries);
    for month in ["2026-09", "2026-10", "2026-11"] {
        for payment in [&b.visa_payment, &b.line_payment] {
            let got = b.entry(&server, month, payment);
            assert_fields(&got, &MONTH_ENTRY_FIELDS, &format!("{month} payment entry"));
            assert_eq!(got["category_id"], payment.as_str(), "{month}: {got}");
            assert_eq!(got["credit_overspending"], 0, "{month}: {got}");
        }
        for listed in b.month(&server, month)["categories"].as_array().unwrap() {
            assert_fields(listed, &MONTH_ENTRY_FIELDS, &format!("{month} listed entry"));
        }
    }
    assert_matches(&b.entry(&server, "2026-10", &b.visa_payment), &e(&b.visa_payment, 0, 50000, 50000, 0, 0), "Visa's");
}

#[test]
fn spec_12_1_payment_category_available_rolls_over() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 50000);
    b.txn(&server, &b.visa, "2026-10-05", -50000, g);
    assert_matches(&b.assign(&server, "2026-10", vp, 100000), &e(vp, 100000, 50000, 150000, 0, 0), "PATCH October");
    let gz = e(g, 0, 0, 0, 0, 0);
    b.check(&server, "2026-10", [e(vp, 100000, 50000, 150000, 0, 0), zero(lp), e(g, 50000, -50000, 0, 0, 0), zero(h)], "rolls");
    b.check(&server, "2026-11", [e(vp, 0, 0, 150000, 0, 0), zero(lp), gz.clone(), zero(h)], "rolls");
    assert_matches(&b.assign(&server, "2026-11", vp, -20000), &e(vp, -20000, 0, 130000, 0, 0), "PATCH November");
    b.check(&server, "2026-11", [e(vp, -20000, 0, 130000, 0, 0), zero(lp), gz.clone(), zero(h)], "rolls");
    b.check(&server, "2026-12", [e(vp, 0, 0, 130000, 0, 0), zero(lp), gz.clone(), zero(h)], "rolls");
    b.check(&server, "2027-06", [e(vp, 0, 0, 130000, 0, 0), zero(lp), gz, zero(h)], "rolls");
}

#[test]
fn spec_12_1_overpaying_a_card_is_cash_overspending() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", vp, 30000);
    // Checking pays Visa 100.00, more than its payment category holds.
    b.transfer(&server, &b.checking, &b.visa, "2026-10-20", -100000);
    b.check(&server, "2026-10", [e(vp, 30000, -100000, -70000, 70000, 0), zero(lp), zero(g), zero(h)], "overpaid");
    b.check(&server, "2026-11", [zero(vp), zero(lp), zero(g), zero(h)], "overpaid");
    let totals = |month: &str, cos_last: i64, left_over: i64, ready: i64| {
        json!({"month": month, "income": 0, "cash_advances": 0, "assigned": 0, "activity": 0,
               "cash_overspending_last_month": cos_last, "left_over_from_last_month": left_over,
               "assigned_in_future": 0, "ready_to_assign": ready})
    };
    let mut october = totals("2026-10", 0, 0, 970000);
    october["income"] = json!(1000000);
    october["assigned"] = json!(30000);
    check_totals(&b, &server, &october, "overpaid");
    check_totals(&b, &server, &totals("2026-11", 70000, 970000, 900000), "overpaid");
    check_totals(&b, &server, &totals("2026-12", 0, 900000, 900000), "overpaid");
}

#[test]
fn spec_12_1_refund_beyond_the_payment_categorys_money_overspends_it() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.visa, "2026-10-12", 30000, g);
    b.check(&server, "2026-10", [e(vp, 0, -30000, -30000, 30000, 0), zero(lp), e(g, 0, 30000, 30000, 0, 0), zero(h)], "refund");
    b.check(&server, "2026-11", [zero(vp), zero(lp), e(g, 0, 0, 30000, 0, 0), zero(h)], "refund");
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-11", "income": 0, "cash_advances": 0, "assigned": 0, "activity": 0,
                "cash_overspending_last_month": 30000, "left_over_from_last_month": 0,
                "assigned_in_future": 0, "ready_to_assign": -30000}),
        "refund",
    );
}

// ---- 12.2 ----

#[test]
fn spec_12_2_covered_spending_moves_to_the_payment_category() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 300000);
    b.txn(&server, &b.visa, "2026-10-05", -100000, g);
    b.check(&server, "2026-10", [e(vp, 0, 100000, 100000, 0, 0), zero(lp), e(g, 300000, -100000, 200000, 0, 0), zero(h)], "covered");
    b.check(&server, "2026-11", [e(vp, 0, 0, 100000, 0, 0), zero(lp), e(g, 0, 0, 200000, 0, 0), zero(h)], "covered");
    b.check(&server, "2026-09", [zero(vp), zero(lp), zero(g), zero(h)], "covered");
}

#[test]
fn spec_12_2_only_what_the_category_has_is_funded() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 50000);
    b.txn(&server, &b.visa, "2026-10-05", -80000, g);
    b.assign(&server, "2026-10", h, 20000);
    b.txn(&server, &b.line, "2026-10-06", -25000, h);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 50000, 50000, 0, 0), e(lp, 0, 20000, 20000, 0, 0), e(g, 50000, -80000, -30000, 0, 30000),
         e(h, 20000, -25000, -5000, 0, 5000)],
        "partly funded",
    );
    b.check(
        &server,
        "2026-11",
        [e(vp, 0, 0, 50000, 0, 0), e(lp, 0, 0, 20000, 0, 0), zero(g), zero(h)],
        "partly funded",
    );
}

#[test]
fn spec_12_2_cash_spending_uses_the_categorys_money_first() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    // The card spends first, but the cash spending later in the month still comes first.
    b.txn(&server, &b.visa, "2026-10-02", -50000, g);
    let cash = id_of(&b.txn(&server, &b.checking, "2026-10-25", -80000, g));
    b.check(&server, "2026-10", [e(vp, 0, 20000, 20000, 0, 0), zero(lp), e(g, 100000, -130000, -30000, 0, 30000), zero(h)], "cash first");
    assert_eq!(b.month(&server, "2026-11")["cash_overspending_last_month"], 0);
    // More cash spending than the category has: nothing is left to fund the card.
    server.patch_transaction_ok(&b.plan, &cash, &json!({"amount": -120000}));
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 100000, -170000, -70000, 20000, 50000), zero(h)], "cash beyond");
    assert_eq!(b.month(&server, "2026-11")["cash_overspending_last_month"], 20000);
}

#[test]
fn spec_12_2_cash_inflows_add_to_funds() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.visa, "2026-10-03", -100000, g);
    b.txn(&server, &b.wallet, "2026-10-20", 60000, g);
    b.check(&server, "2026-10", [e(vp, 0, 60000, 60000, 0, 0), zero(lp), e(g, 0, -40000, -40000, 0, 40000), zero(h)], "cash in");
    // Money brought in from the house to Savings, categorized to Groceries, is a cash line too.
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.house.id, "date": "2026-10-21", "amount": -10000, "payee_id": b.savings.payee,
                "category_id": g}),
    );
    b.check(&server, "2026-10", [e(vp, 0, 70000, 70000, 0, 0), zero(lp), e(g, 0, -30000, -30000, 0, 30000), zero(h)], "from the house");
}

#[test]
fn spec_12_2_positive_carryover_adds_to_funds() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    b.txn(&server, &b.visa, "2026-11-04", -70000, g);
    b.txn(&server, &b.visa, "2026-12-04", -50000, g);
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 100000, 0, 100000, 0, 0), zero(h)], "carryover");
    b.check(&server, "2026-11", [e(vp, 0, 70000, 70000, 0, 0), zero(lp), e(g, 0, -70000, 30000, 0, 0), zero(h)], "carryover");
    b.check(&server, "2026-12", [e(vp, 0, 30000, 100000, 0, 0), zero(lp), e(g, 0, -50000, -20000, 0, 20000), zero(h)], "carryover");
    b.check(&server, "2027-01", [e(vp, 0, 0, 100000, 0, 0), zero(lp), zero(g), zero(h)], "carryover");
}

#[test]
fn spec_12_2_negative_carryover_adds_nothing() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-08", -40000, g);
    b.assign(&server, "2026-11", g, 30000);
    b.txn(&server, &b.visa, "2026-11-09", -50000, g);
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 0, -40000, -40000, 40000, 0), zero(h)], "overspent before");
    b.check(&server, "2026-11", [e(vp, 0, 30000, 30000, 0, 0), zero(lp), e(g, 30000, -50000, -20000, 0, 20000), zero(h)], "overspent before");
}

#[test]
fn spec_12_2_negative_funds_fund_nothing() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", h, -20000);
    b.txn(&server, &b.visa, "2026-10-10", -10000, h);
    b.check(&server, "2026-10", [zero(vp), zero(lp), zero(g), e(h, -20000, -10000, -30000, 20000, 10000)], "negative funds");
}

#[test]
fn spec_12_2_spending_and_refunds_net_within_each_card() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    // Groceries: Visa's net is 70.00 of spending, with the category already overspent in cash.
    b.txn(&server, &b.checking, "2026-10-01", -50000, g);
    b.txn(&server, &b.visa, "2026-10-03", -100000, g);
    b.txn(&server, &b.visa, "2026-10-10", 30000, g);
    // Household: Visa's net is a 40.00 refund.
    b.assign(&server, "2026-10", h, 50000);
    b.txn(&server, &b.visa, "2026-10-04", -20000, h);
    b.txn(&server, &b.visa, "2026-10-11", 60000, h);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, -40000, -40000, 40000, 0), zero(lp), e(g, 0, -120000, -120000, 50000, 70000),
         e(h, 50000, 40000, 90000, 0, 0)],
        "netted",
    );
    assert_matches(&b.month(&server, "2026-10"), &json!({"activity": -80000}), "netted");
    assert_eq!(b.month(&server, "2026-11")["cash_overspending_last_month"], 90000);
}

#[test]
fn spec_12_2_refunds_on_one_card_fund_spending_on_another() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.visa, "2026-10-02", 40000, g);
    b.txn(&server, &b.line, "2026-10-05", -100000, g);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, -40000, -40000, 40000, 0), e(lp, 0, 40000, 40000, 0, 0), e(g, 0, -60000, -60000, 0, 60000), zero(h)],
        "refund funds the other card",
    );
}

#[test]
fn spec_12_2_cards_are_funded_in_the_order_they_were_created() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let g = id_of(&server.create_category(&plan, &group, "Groceries"));
    let visa = Card::from(&server.add_account(&plan, "Visa", "creditCard", 0));
    let amex = Card::from(&server.add_account(&plan, "Amex", "creditCard", 0));
    let master = Card::from(&server.add_account(&plan, "Mastercard", "creditCard", 0));
    server.assign(&plan, "2026-10", &g, 120000);
    // Entered, and dated, in the opposite order to the cards'.
    for (card, date) in [(&master, "2026-10-01"), (&amex, "2026-10-02"), (&visa, "2026-10-30")] {
        server.create_transaction(
            &plan,
            &json!({"account_id": card.id, "date": date, "amount": -50000, "category_id": g}),
        );
    }
    let expected = json!({"categories": [
        e(&visa.payment, 0, 50000, 50000, 0, 0),
        e(&amex.payment, 0, 50000, 50000, 0, 0),
        e(&master.payment, 0, 20000, 20000, 0, 0),
        e(&g, 120000, -150000, -30000, 0, 30000)]});
    assert_matches(&server.month(&plan, "2026-10"), &expected, "three cards");
    // Renaming a card doesn't change when it was created.
    server.patch_ok(&format!("/plans/{plan}/accounts/{}", visa.id), &json!({"name": "Zulu"}));
    assert_matches(&server.month(&plan, "2026-10"), &expected, "after renaming Visa");
    assert_matches(&server.month_entry(&plan, "2026-10", &master.payment), &e(&master.payment, 0, 20000, 20000, 0, 0), "GET");
}

#[test]
fn spec_12_2_a_line_of_credit_created_later_is_funded_later() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    b.txn(&server, &b.line, "2026-10-01", -60000, g);
    b.txn(&server, &b.visa, "2026-10-20", -80000, g);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 80000, 80000, 0, 0), e(lp, 0, 20000, 20000, 0, 0), e(g, 100000, -140000, -40000, 0, 40000), zero(h)],
        "Visa first",
    );
}

#[test]
fn spec_12_2_each_category_is_funded_separately() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    b.txn(&server, &b.visa, "2026-10-05", -60000, g);
    b.txn(&server, &b.visa, "2026-10-06", -40000, h);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 60000, 60000, 0, 0), zero(lp), e(g, 100000, -60000, 40000, 0, 0), e(h, 0, -40000, -40000, 0, 40000)],
        "separately",
    );
}

#[test]
fn spec_12_2_funding_is_worked_out_month_by_month() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 50000);
    b.txn(&server, &b.visa, "2026-10-05", -80000, g);
    b.assign(&server, "2026-11", g, 100000);
    b.txn(&server, &b.visa, "2026-11-05", -20000, g);
    b.check(&server, "2026-10", [e(vp, 0, 50000, 50000, 0, 0), zero(lp), e(g, 50000, -80000, -30000, 0, 30000), zero(h)], "by month");
    b.check(&server, "2026-11", [e(vp, 0, 20000, 70000, 0, 0), zero(lp), e(g, 100000, -20000, 80000, 0, 0), zero(h)], "by month");
}

#[test]
fn spec_12_2_split_lines_in_a_credit_account_are_funded() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    b.assign(&server, "2026-10", h, 10000);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-07", "amount": -100000,
                "subtransactions": [{"amount": -70000, "category_id": g},
                                    {"amount": -40000, "category_id": h},
                                    {"amount": 10000, "category_id": g}]}),
    );
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 70000, 70000, 0, 0), zero(lp), e(g, 100000, -60000, 40000, 0, 0), e(h, 10000, -40000, -30000, 0, 30000)],
        "split",
    );
}

#[test]
fn spec_12_2_spending_through_a_tracking_transfer_is_funded() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", h, 30000);
    // Paying toward the mortgage with the line of credit, out of Household: a line in Credit Line.
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.mortgage.id, "date": "2026-10-09", "amount": 45000, "payee_id": b.line.payee,
                "category_id": h}),
    );
    b.check(&server, "2026-10", [zero(vp), e(lp, 0, 30000, 30000, 0, 0), zero(g), e(h, 30000, -45000, -15000, 0, 15000)], "tracking");
}

#[test]
fn spec_12_2_funding_works_at_the_ends_of_the_date_range() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "1900-01-01", MAX, RTA);
    b.assign(&server, "1900-01", g, MAX);
    b.txn(&server, &b.visa, "1900-01-15", -MAX, g);
    b.transfer(&server, &b.checking, &b.visa, "1900-02-01", -MAX);
    b.txn(&server, &b.line, "2999-12-31", -MAX, h);
    let first = b.check(&server, "1900-01", [e(vp, 0, MAX, MAX, 0, 0), zero(lp), e(g, MAX, -MAX, 0, 0, 0), zero(h)], "range ends");
    assert_matches(&first, &json!({"income": MAX, "assigned": MAX, "activity": -MAX, "ready_to_assign": 0}), "1900-01");
    b.check(&server, "1900-02", [e(vp, 0, -MAX, 0, 0, 0), zero(lp), zero(g), zero(h)], "range ends");
    b.check(&server, "2999-11", [zero(vp), zero(lp), zero(g), zero(h)], "range ends");
    let last = b.check(&server, "2999-12", [zero(vp), zero(lp), zero(g), e(h, 0, -MAX, -MAX, 0, MAX)], "range ends");
    assert_matches(
        &last,
        &json!({"income": 0, "cash_advances": 0, "assigned": 0, "activity": -MAX, "cash_overspending_last_month": 0,
                "left_over_from_last_month": 0, "assigned_in_future": 0, "ready_to_assign": 0}),
        "2999-12",
    );
}

// ---- 12.3 ----

#[test]
fn spec_12_3_credit_overspending_is_unfunded_spending() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 30000);
    b.txn(&server, &b.visa, "2026-10-05", -100000, g);
    b.txn(&server, &b.line, "2026-10-06", -15000, h);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 30000, 30000, 0, 0), zero(lp), e(g, 30000, -100000, -70000, 0, 70000), e(h, 0, -15000, -15000, 0, 15000)],
        "credit overspending",
    );
}

#[test]
fn spec_12_3_credit_overspending_becomes_debt_not_less_ready_to_assign() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", g, 30000);
    b.txn(&server, &b.visa, "2026-10-05", -100000, g);
    let october = b.check(&server, "2026-10", [e(vp, 0, 30000, 30000, 0, 0), zero(lp), e(g, 30000, -100000, -70000, 0, 70000), zero(h)], "debt");
    assert_matches(
        &october,
        &json!({"income": 1000000, "cash_advances": 0, "assigned": 30000, "activity": -100000,
                "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
                "assigned_in_future": 0, "ready_to_assign": 970000}),
        "October",
    );
    let november = b.check(&server, "2026-11", [e(vp, 0, 0, 30000, 0, 0), zero(lp), zero(g), zero(h)], "debt");
    assert_matches(
        &november,
        &json!({"income": 0, "cash_advances": 0, "assigned": 0, "activity": 0,
                "cash_overspending_last_month": 0, "left_over_from_last_month": 970000,
                "assigned_in_future": 0, "ready_to_assign": 970000}),
        "November",
    );
    assert_eq!(server.balances(&b.plan, &b.visa.id).0, -100000, "Visa's balance");
}

#[test]
fn spec_12_3_cash_and_credit_overspending_in_one_category() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", g, 20000);
    b.txn(&server, &b.visa, "2026-10-03", -40000, g);
    b.txn(&server, &b.checking, "2026-10-04", -50000, g);
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 20000, -90000, -70000, 30000, 40000), zero(h)], "both");
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-11", "cash_overspending_last_month": 30000, "left_over_from_last_month": 980000,
                "ready_to_assign": 950000}),
        "both",
    );
}

#[test]
fn spec_12_3_spending_netted_away_by_refunds_is_not_overspending() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, -60000);
    b.txn(&server, &b.visa, "2026-10-02", -30000, g);
    b.txn(&server, &b.visa, "2026-10-03", 50000, g);
    b.assign(&server, "2026-10", h, -50000);
    b.txn(&server, &b.line, "2026-10-05", 20000, h);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, -20000, -20000, 20000, 0), e(lp, 0, -20000, -20000, 20000, 0), e(g, -60000, 20000, -40000, 40000, 0),
         e(h, -50000, 20000, -30000, 30000, 0)],
        "refunds only",
    );
}

// ---- 12.4 ----

#[test]
fn spec_12_4_payments_come_off_the_payment_category() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", vp, 80000);
    b.transfer(&server, &b.checking, &b.visa, "2026-10-20", -50000);
    let october = b.check(&server, "2026-10", [e(vp, 80000, -50000, 30000, 0, 0), zero(lp), zero(g), zero(h)], "payment");
    assert_matches(
        &october,
        &json!({"income": 1000000, "cash_advances": 0, "assigned": 80000, "activity": 0, "ready_to_assign": 920000}),
        "October",
    );
    b.check(&server, "2026-11", [e(vp, 0, 0, 30000, 0, 0), zero(lp), zero(g), zero(h)], "payment");
}

#[test]
fn spec_12_4_payments_from_any_cash_account_entered_on_either_side() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", vp, 30000);
    b.assign(&server, "2026-10", lp, 35000);
    // From Savings, entered on Visa's side.
    b.transfer(&server, &b.visa, &b.savings, "2026-10-11", 30000);
    // From the wallet, entered on its side.
    b.transfer(&server, &b.wallet, &b.line, "2026-10-12", -20000);
    // From Checking, entered on Credit Line's side.
    b.transfer(&server, &b.line, &b.checking, "2026-10-13", 15000);
    let october = b.check(&server, "2026-10", [e(vp, 30000, -30000, 0, 0, 0), e(lp, 35000, -35000, 0, 0, 0), zero(g), zero(h)], "payments");
    assert_matches(&october, &json!({"income": 0, "cash_advances": 0, "activity": 0}), "October");
}

#[test]
fn spec_12_4_payments_count_in_the_month_of_their_date() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", vp, 50000);
    let out = b.transfer(&server, &b.checking, &b.visa, "2026-11-02", -50000);
    b.check(&server, "2026-10", [e(vp, 50000, 0, 50000, 0, 0), zero(lp), zero(g), zero(h)], "November payment");
    b.check(&server, "2026-11", [e(vp, 0, -50000, 0, 0, 0), zero(lp), zero(g), zero(h)], "November payment");
    let visa_half = str_field(&out, "transfer_transaction_id");
    server.patch_transaction_ok(&b.plan, &visa_half, &json!({"date": "2026-10-31"}));
    b.check(&server, "2026-10", [e(vp, 50000, -50000, 0, 0, 0), zero(lp), zero(g), zero(h)], "moved to October");
    b.check(&server, "2026-11", [zero(vp), zero(lp), zero(g), zero(h)], "moved to October");
}

#[test]
fn spec_12_4_payment_activity_combines_funding_refunds_and_payments() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 200000);
    b.assign(&server, "2026-10", h, 100000);
    b.txn(&server, &b.visa, "2026-10-03", -150000, g);
    b.txn(&server, &b.visa, "2026-10-04", 20000, h);
    b.txn(&server, &b.line, "2026-10-05", -50000, h);
    b.transfer(&server, &b.checking, &b.visa, "2026-10-25", -100000);
    let october = b.check(
        &server,
        "2026-10",
        [e(vp, 0, 30000, 30000, 0, 0), e(lp, 0, 50000, 50000, 0, 0), e(g, 200000, -150000, 50000, 0, 0),
         e(h, 100000, -30000, 70000, 0, 0)],
        "combined",
    );
    assert_matches(&october, &json!({"assigned": 300000, "activity": -180000}), "October");
}

#[test]
fn spec_12_4_payments_follow_changes_to_the_transfer() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g) = (&b.visa_payment, &b.line_payment, &b.groceries);
    let out = b.transfer(&server, &b.checking, &b.visa, "2026-10-20", -50000);
    let (out, visa_half) = (id_of(&out), str_field(&out, "transfer_transaction_id"));
    let activity = |month: &str, category: &str| b.entry(&server, month, category)["activity"].clone();
    let check = |what: &str, expected: [(&str, &str, i64); 5]| {
        for (month, category, want) in expected {
            assert_eq!(activity(month, category), want, "{what}: {month} {category}");
        }
    };
    check("created", [("2026-10", vp, -50000), ("2026-10", lp, 0), ("2026-11", vp, 0), ("2026-11", lp, 0), ("2026-11", g, 0)]);
    server.patch_transaction_ok(&b.plan, &visa_half, &json!({"amount": 70000}));
    check("amount", [("2026-10", vp, -70000), ("2026-10", lp, 0), ("2026-11", vp, 0), ("2026-11", lp, 0), ("2026-11", g, 0)]);
    server.patch_transaction_ok(&b.plan, &out, &json!({"payee_id": b.line.payee}));
    check("to Credit Line", [("2026-10", vp, 0), ("2026-10", lp, -70000), ("2026-11", vp, 0), ("2026-11", lp, 0), ("2026-11", g, 0)]);
    server.patch_transaction_ok(&b.plan, &out, &json!({"date": "2026-11-03"}));
    check("date", [("2026-10", vp, 0), ("2026-10", lp, 0), ("2026-11", vp, 0), ("2026-11", lp, -70000), ("2026-11", g, 0)]);
    server.patch_transaction_ok(&b.plan, &out, &json!({"payee_id": null, "category_id": g}));
    check("no longer a transfer", [("2026-10", vp, 0), ("2026-10", lp, 0), ("2026-11", vp, 0), ("2026-11", lp, 0), ("2026-11", g, -70000)]);
    assert_status(&server.delete_transaction(&b.plan, &out), 204, "DELETE");
    check("deleted", [("2026-10", vp, 0), ("2026-10", lp, 0), ("2026-11", vp, 0), ("2026-11", lp, 0), ("2026-11", g, 0)]);
}

// ---- 12.5 ----

#[test]
fn spec_12_5_cash_advance_adds_to_ready_to_assign() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.transfer(&server, &b.visa, &b.checking, "2026-10-10", -100000);
    let october = b.check(&server, "2026-10", [zero(vp), zero(lp), zero(g), zero(h)], "advance");
    assert_matches(
        &october,
        &json!({"month": "2026-10", "income": 0, "cash_advances": 100000, "assigned": 0, "activity": 0,
                "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
                "assigned_in_future": 0, "ready_to_assign": 100000}),
        "October",
    );
    for (month, left_over) in [("2026-09", 0), ("2026-11", 100000), ("2027-03", 100000)] {
        check_totals(
            &b,
            &server,
            &json!({"month": month, "income": 0, "cash_advances": 0, "assigned": 0, "activity": 0,
                    "cash_overspending_last_month": 0, "left_over_from_last_month": left_over,
                    "assigned_in_future": 0, "ready_to_assign": left_over}),
            "advance",
        );
    }
    b.check(&server, "2026-11", [zero(vp), zero(lp), zero(g), zero(h)], "advance");
}

#[test]
fn spec_12_5_cash_advances_from_any_card_entered_on_either_side() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    // Entered on Checking's side: Visa's half is -50.00.
    b.transfer(&server, &b.checking, &b.visa, "2026-10-03", 50000);
    // Into the wallet from the line of credit, entered on the wallet's side.
    b.transfer(&server, &b.wallet, &b.line, "2026-10-04", 20000);
    // Into Savings, entered on the line of credit's side, in November.
    b.transfer(&server, &b.line, &b.savings, "2026-11-01", -5000);
    b.check(&server, "2026-10", [zero(vp), zero(lp), zero(g), zero(h)], "advances");
    b.check(&server, "2026-11", [zero(vp), zero(lp), zero(g), zero(h)], "advances");
    check_totals(&b, &server, &json!({"month": "2026-10", "income": 0, "cash_advances": 70000, "ready_to_assign": 70000}), "advances");
    check_totals(
        &b,
        &server,
        &json!({"month": "2026-11", "income": 0, "cash_advances": 5000, "left_over_from_last_month": 70000,
                "ready_to_assign": 75000}),
        "advances",
    );
}

#[test]
fn spec_12_5_cash_advances_follow_changes_to_the_transfer() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let vp = &b.visa_payment;
    let advance = id_of(&b.transfer(&server, &b.visa, &b.checking, "2026-10-10", -100000));
    let check = |what: &str, oct: i64, nov: i64, nov_payment: i64| {
        assert_eq!(b.month(&server, "2026-10")["cash_advances"], oct, "{what}: October");
        assert_eq!(b.month(&server, "2026-11")["cash_advances"], nov, "{what}: November");
        assert_eq!(b.entry(&server, "2026-11", vp)["activity"], nov_payment, "{what}: Visa's payment category");
    };
    check("created", 100000, 0, 0);
    server.patch_transaction_ok(&b.plan, &advance, &json!({"amount": -40000}));
    check("smaller", 40000, 0, 0);
    server.patch_transaction_ok(&b.plan, &advance, &json!({"date": "2026-11-01"}));
    check("moved", 0, 40000, 0);
    server.patch_transaction_ok(&b.plan, &advance, &json!({"amount": 40000}));
    check("now a payment", 0, 0, -40000);
    server.patch_transaction_ok(&b.plan, &advance, &json!({"payee_id": b.line.payee}));
    check("now between cards", 0, 0, 0);
    let half = server.patch_transaction_ok(&b.plan, &advance, &json!({"payee_id": b.wallet.payee, "amount": -25000}));
    check("into the wallet", 0, 25000, 0);
    let other = str_field(&half, "transfer_transaction_id");
    assert_status(&server.delete_transaction(&b.plan, &other), 204, "DELETE the wallet's half");
    check("deleted", 0, 0, 0);
}

#[test]
fn spec_12_5_card_spending_made_a_transfer_to_cash_is_a_cash_advance() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    let spend = id_of(&b.txn(&server, &b.visa, "2026-10-05", -50000, g));
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 0, -50000, -50000, 0, 50000), zero(h)], "spending");
    let changed = server.patch_transaction_ok(&b.plan, &spend, &json!({"payee_id": b.checking.payee}));
    assert_eq!(changed["category_id"], Value::Null, "{changed}");
    let october = b.check(&server, "2026-10", [zero(vp), zero(lp), zero(g), zero(h)], "now a cash advance");
    assert_matches(&october, &json!({"cash_advances": 50000, "activity": 0, "ready_to_assign": 50000}), "now a cash advance");
    server.patch_transaction_ok(&b.plan, &spend, &json!({"payee_id": null, "category_id": h}));
    let october = b.check(&server, "2026-10", [zero(vp), zero(lp), zero(g), e(h, 0, -50000, -50000, 0, 50000)], "spending again");
    assert_matches(&october, &json!({"cash_advances": 0, "ready_to_assign": 0}), "spending again");
}

#[test]
fn spec_12_5_cash_advances_work_at_the_ends_of_the_date_range() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    b.transfer(&server, &b.visa, &b.checking, "1900-01-01", -MAX);
    b.transfer(&server, &b.line, &b.wallet, "2999-12-31", -MAX);
    b.assign(&server, "2999-12", &b.groceries, MAX);
    b.assign(&server, "2999-12", &b.household, MAX);
    for expected in [
        json!({"month": "1900-01", "income": 0, "cash_advances": MAX, "assigned": 0, "left_over_from_last_month": 0,
               "assigned_in_future": MAX, "ready_to_assign": 0}),
        json!({"month": "2999-11", "income": 0, "cash_advances": 0, "assigned": 0, "left_over_from_last_month": MAX,
               "assigned_in_future": MAX, "ready_to_assign": 0}),
        json!({"month": "2999-12", "income": 0, "cash_advances": MAX, "assigned": 2 * MAX,
               "left_over_from_last_month": MAX, "assigned_in_future": 0, "ready_to_assign": 0}),
    ] {
        check_totals(&b, &server, &expected, "range ends");
    }
}

#[test]
fn spec_12_5_transfers_with_tracking_accounts_are_neither_payments_nor_advances() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 40000);
    // Visa's half is negative, but the other half is in a tracking account: spending, not an advance.
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-05", "amount": -40000, "payee_id": b.house.payee,
                "category_id": g}),
    );
    // Visa's half is positive, from a tracking account, categorized to Ready to Assign: no payment.
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.mortgage.id, "date": "2026-10-06", "amount": -25000, "payee_id": b.visa.payee,
                "category_id": RTA}),
    );
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.line.id, "date": "2026-10-07", "amount": -10000, "payee_id": b.mortgage.payee,
                "category_id": RTA}),
    );
    let october = b.check(&server, "2026-10", [e(vp, 0, 40000, 40000, 0, 0), zero(lp), e(g, 40000, -40000, 0, 0, 0), zero(h)], "tracking");
    assert_matches(&october, &json!({"income": 0, "cash_advances": 0, "ready_to_assign": -40000}), "October");
}

// ---- 12.6 ----

#[test]
fn spec_12_6_ready_to_assign_lines_in_credit_accounts_change_nothing() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    let store = Card::from(&server.create_account(
        &b.plan,
        &json!({"name": "Store Card", "type": "creditCard", "starting_balance": -350000, "starting_date": "2026-10-01"}),
    ));
    // A statement credit, a balance adjustment, and a split with a Ready to Assign part.
    b.txn(&server, &b.visa, "2026-10-05", 20000, RTA);
    b.txn(&server, &b.line, "2026-10-06", -5000, RTA);
    server.create_transaction(
        &b.plan,
        &json!({"account_id": b.visa.id, "date": "2026-10-07", "amount": -30000,
                "subtransactions": [{"amount": -10000, "category_id": RTA}, {"amount": -20000, "category_id": g}]}),
    );
    let expected = json!({"month": "2026-10", "income": 0, "cash_advances": 0, "assigned": 0, "activity": -20000,
                          "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
                          "assigned_in_future": 0, "ready_to_assign": 0,
                          "categories": [zero(vp), zero(lp), zero(&store.payment), e(g, 0, -20000, -20000, 0, 20000), zero(h)]});
    let got = b.month(&server, "2026-10");
    assert_matches(&got, &expected, "October");
    assert_month_identity(&got);
    assert_matches(
        &b.month(&server, "2026-11"),
        &json!({"income": 0, "cash_advances": 0, "cash_overspending_last_month": 0, "ready_to_assign": 0,
                "categories": [zero(vp), zero(lp), zero(&store.payment), zero(g), zero(h)]}),
        "November",
    );
}

#[test]
fn spec_12_6_transfers_between_two_credit_or_two_cash_accounts_change_nothing() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 500000, RTA);
    b.assign(&server, "2026-10", vp, 100000);
    b.transfer(&server, &b.visa, &b.line, "2026-10-10", -200000);
    b.transfer(&server, &b.line, &b.visa, "2026-11-05", -30000);
    b.transfer(&server, &b.line, &b.visa, "2026-11-06", 45000);
    b.transfer(&server, &b.checking, &b.savings, "2026-10-15", -100000);
    b.transfer(&server, &b.wallet, &b.checking, "2026-11-15", 20000);
    let october = b.check(&server, "2026-10", [e(vp, 100000, 0, 100000, 0, 0), zero(lp), zero(g), zero(h)], "between cards");
    assert_matches(
        &october,
        &json!({"income": 500000, "cash_advances": 0, "assigned": 100000, "activity": 0, "ready_to_assign": 400000}),
        "October",
    );
    let november = b.check(&server, "2026-11", [e(vp, 0, 0, 100000, 0, 0), zero(lp), zero(g), zero(h)], "between cards");
    assert_matches(
        &november,
        &json!({"income": 0, "cash_advances": 0, "assigned": 0, "activity": 0, "cash_overspending_last_month": 0,
                "ready_to_assign": 400000}),
        "November",
    );
}

// ---- 12.7 ----

/// 12.7's plan: returns the plan, Visa's id, its payment category and Groceries.
fn example_plan(server: &Server) -> (String, String, String, String) {
    let plan = server.plan_id("Household");
    let checking = server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "starting_balance": 1000000, "starting_date": "2026-10-01"}),
    );
    let visa = Card::from(&server.create_account(
        &plan,
        &json!({"name": "Visa", "type": "creditCard", "starting_balance": -200000, "starting_date": "2026-10-01"}),
    ));
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    server.assign(&plan, "2026-10", &groceries, 300000);
    server.assign(&plan, "2026-10", &visa.payment, 200000);
    let spend = |date: &str, amount: i64| {
        server.create_transaction(
            &plan,
            &json!({"account_id": visa.id, "date": date, "amount": amount, "category_id": groceries}),
        );
    };
    spend("2026-10-05", -100000);
    spend("2026-10-09", -250000);
    server.create_transaction(
        &plan,
        &json!({"account_id": id_of(&checking), "date": "2026-10-20", "amount": -200000, "payee_id": visa.payee}),
    );
    spend("2026-11-03", 30000);
    (plan, visa.id, visa.payment, groceries)
}

#[test]
fn spec_12_7_october_matches_example() {
    let server = Server::start();
    let (plan, _, visa_payment, groceries) = example_plan(&server);
    let got = server.month(&plan, "2026-10");
    assert_fields(&got, &MONTH_FIELDS, "October");
    assert_matches(
        &got,
        &json!({"month": "2026-10", "income": 1000000, "cash_advances": 0, "assigned": 500000,
                "activity": -350000, "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
                "assigned_in_future": 0, "ready_to_assign": 500000,
                "categories": [
                    {"category_id": visa_payment, "assigned": 200000, "activity": 100000, "available": 300000,
                     "cash_overspending": 0, "credit_overspending": 0},
                    {"category_id": groceries, "assigned": 300000, "activity": -350000, "available": -50000,
                     "cash_overspending": 0, "credit_overspending": 50000}]}),
        "12.7's October",
    );
}

#[test]
fn spec_12_7_november_matches_example() {
    let server = Server::start();
    let (plan, _, visa_payment, groceries) = example_plan(&server);
    let got = server.month(&plan, "2026-11");
    assert_fields(&got, &MONTH_FIELDS, "November");
    assert_matches(
        &got,
        &json!({"month": "2026-11", "income": 0, "cash_advances": 0, "assigned": 0,
                "activity": 30000, "cash_overspending_last_month": 0, "left_over_from_last_month": 500000,
                "assigned_in_future": 0, "ready_to_assign": 500000,
                "categories": [
                    {"category_id": visa_payment, "assigned": 0, "activity": -30000, "available": 270000,
                     "cash_overspending": 0, "credit_overspending": 0},
                    {"category_id": groceries, "assigned": 0, "activity": 30000, "available": 30000,
                     "cash_overspending": 0, "credit_overspending": 0}]}),
        "12.7's November",
    );
}

#[test]
fn spec_12_7_visa_owes_320_and_its_payment_category_holds_270() {
    let server = Server::start();
    let (plan, visa, visa_payment, groceries) = example_plan(&server);
    assert_eq!(server.balances(&plan, &visa).0, -320000, "Visa's balance");
    for (month, entry) in [
        ("2026-10", e(&visa_payment, 200000, 100000, 300000, 0, 0)),
        ("2026-11", e(&visa_payment, 0, -30000, 270000, 0, 0)),
        ("2026-12", e(&visa_payment, 0, 0, 270000, 0, 0)),
    ] {
        assert_matches(&server.month_entry(&plan, month, &visa_payment), &entry, &format!("{month} payment entry"));
    }
    assert_matches(
        &server.month_entry(&plan, "2026-10", &groceries),
        &e(&groceries, 300000, -350000, -50000, 0, 50000),
        "October Groceries",
    );
    assert_matches(&server.month(&plan, "2026-12"), &json!({"ready_to_assign": 500000}), "December");
}
