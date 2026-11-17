//! SPEC sections 11 and 12 on plans with credit accounts as they change: months follow
//! edits to earlier months (12.2), to card spending (12.4) and to transfers between cash
//! and credit accounts (12.5), the funding order follows deleted and new cards (12.2), and
//! starting balances and closed cards (11.6, 12.6).

use mybudget_testsuite::{
    Card, CreditBudget, Server, Value, assert_matches, assert_month_identity, assert_status, credit_entry as e,
    id_of, json, str_field, zero_entry as zero,
};

const RTA: &str = "ready_to_assign";

// ---- 11.6 ----

#[test]
fn spec_11_6_closed_cards_payment_categories_stay_in_months() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 500000, RTA);
    b.assign(&server, "2026-10", g, 40000);
    b.txn(&server, &b.visa, "2026-10-05", -40000, g);
    b.transfer(&server, &b.checking, &b.visa, "2026-10-20", -40000);
    let closed = server.patch_ok(&format!("/plans/{}/accounts/{}", b.plan, b.visa.id), &json!({"closed": true}));
    assert_eq!(closed["closed"], true, "{closed}");
    assert_matches(&b.assign(&server, "2026-11", vp, 15000), &e(vp, 15000, 0, 15000, 0, 0), "PATCH a closed card's");
    b.check(&server, "2026-10", [e(vp, 0, 0, 0, 0, 0), zero(lp), e(g, 40000, -40000, 0, 0, 0), zero(h)], "closed");
    let november = b.check(&server, "2026-11", [e(vp, 15000, 0, 15000, 0, 0), zero(lp), zero(g), zero(h)], "closed");
    assert_matches(
        &november,
        &json!({"income": 0, "assigned": 15000, "left_over_from_last_month": 460000, "ready_to_assign": 445000}),
        "November",
    );
}

// ---- 12.2 ----

#[test]
fn spec_12_2_changes_to_earlier_months_change_later_funding() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 50000);
    let october_spend = id_of(&b.txn(&server, &b.visa, "2026-10-05", -80000, g));
    b.txn(&server, &b.visa, "2026-11-05", -20000, g);
    b.check(&server, "2026-10", [e(vp, 0, 50000, 50000, 0, 0), zero(lp), e(g, 50000, -80000, -30000, 0, 30000), zero(h)], "start");
    b.check(&server, "2026-11", [e(vp, 0, 0, 50000, 0, 0), zero(lp), e(g, 0, -20000, -20000, 0, 20000), zero(h)], "start");

    // More in October leaves money over to fund November's spending.
    b.assign(&server, "2026-10", g, 100000);
    b.check(&server, "2026-10", [e(vp, 0, 80000, 80000, 0, 0), zero(lp), e(g, 100000, -80000, 20000, 0, 0), zero(h)], "more");
    b.check(&server, "2026-11", [e(vp, 0, 20000, 100000, 0, 0), zero(lp), e(g, 0, -20000, 0, 0, 0), zero(h)], "more");

    // October's cash spending uses that money up first.
    b.txn(&server, &b.checking, "2026-10-25", -20000, g);
    b.check(&server, "2026-10", [e(vp, 0, 80000, 80000, 0, 0), zero(lp), e(g, 100000, -100000, 0, 0, 0), zero(h)], "cash");
    b.check(&server, "2026-11", [e(vp, 0, 0, 80000, 0, 0), zero(lp), e(g, 0, -20000, -20000, 0, 20000), zero(h)], "cash");

    // Without October's card spending, November's is funded again.
    assert_status(&server.delete_transaction(&b.plan, &october_spend), 204, "DELETE October's spending");
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 100000, -20000, 80000, 0, 0), zero(h)], "deleted");
    b.check(&server, "2026-11", [e(vp, 0, 20000, 20000, 0, 0), zero(lp), e(g, 0, -20000, 60000, 0, 0), zero(h)], "deleted");
    b.check(&server, "2026-12", [e(vp, 0, 0, 20000, 0, 0), zero(lp), e(g, 0, 0, 60000, 0, 0), zero(h)], "deleted");
}

#[test]
fn spec_12_2_carryover_from_the_first_month_funds_card_spending_in_the_last() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "1900-01", g, 100000);
    // Credit Line's spending is entered and dated first, but Visa is funded first.
    b.txn(&server, &b.line, "2999-12-01", -150000, g);
    b.txn(&server, &b.visa, "2999-12-31", -30000, g);
    b.check(&server, "1900-01", [zero(vp), zero(lp), e(g, 100000, 0, 100000, 0, 0), zero(h)], "first month");
    b.check(&server, "2500-06", [zero(vp), zero(lp), e(g, 0, 0, 100000, 0, 0), zero(h)], "between");
    b.check(&server, "2999-11", [zero(vp), zero(lp), e(g, 0, 0, 100000, 0, 0), zero(h)], "between");
    b.check(
        &server,
        "2999-12",
        [e(vp, 0, 30000, 30000, 0, 0), e(lp, 0, 70000, 70000, 0, 0), e(g, 0, -180000, -80000, 0, 80000), zero(h)],
        "last month",
    );
}

#[test]
fn spec_12_2_a_card_created_after_deleting_another_is_funded_last() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let g = id_of(&server.create_category(&plan, &group, "Groceries"));
    let visa = Card::from(&server.add_account(&plan, "Visa", "creditCard", 0));
    let amex = Card::from(&server.add_account(&plan, "Amex", "creditCard", 0));
    assert_status(&server.delete(&format!("/plans/{plan}/accounts/{}", visa.id)), 204, "DELETE Visa");
    let new_visa = Card::from(&server.add_account(&plan, "Visa", "creditCard", 0));
    server.assign(&plan, "2026-10", &g, 50000);
    for (card, date) in [(&new_visa, "2026-10-01"), (&amex, "2026-10-20")] {
        server.create_transaction(&plan, &json!({"account_id": card.id, "date": date, "amount": -40000, "category_id": g}));
    }
    let expected = json!({"categories": [
        e(&amex.payment, 0, 40000, 40000, 0, 0),
        e(&new_visa.payment, 0, 10000, 10000, 0, 0),
        e(&g, 50000, -80000, -30000, 0, 30000)]});
    let october = server.month(&plan, "2026-10");
    assert_matches(&october, &expected, "Amex before the new Visa");
    assert_month_identity(&october);
    assert_matches(&server.month_entry(&plan, "2026-10", &new_visa.payment), &e(&new_visa.payment, 0, 10000, 10000, 0, 0), "GET");
}

#[test]
fn spec_12_2_card_spending_moved_to_a_tracking_transfer_is_still_funded() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 50000);
    let spend = id_of(&b.txn(&server, &b.line, "2026-10-05", -50000, g));
    b.check(&server, "2026-10", [zero(vp), e(lp, 0, 50000, 50000, 0, 0), e(g, 50000, -50000, 0, 0, 0), zero(h)], "spending");
    // Paying toward the mortgage with it instead: still a line in Credit Line, keeping its category.
    let changed = server.patch_transaction_ok(&b.plan, &spend, &json!({"payee_id": b.mortgage.payee}));
    assert_eq!(changed["category_id"], g.as_str(), "{changed}");
    b.check(&server, "2026-10", [zero(vp), e(lp, 0, 50000, 50000, 0, 0), e(g, 50000, -50000, 0, 0, 0), zero(h)], "to the mortgage");
    // Re-entered from the mortgage's side, as money into it from Visa, out of Household.
    let mortgage_half = str_field(&changed, "transfer_transaction_id");
    server.patch_transaction_ok(&b.plan, &mortgage_half, &json!({"payee_id": b.visa.payee, "category_id": h}));
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 50000, 0, 50000, 0, 0), e(h, 0, -50000, -50000, 0, 50000)], "from Visa");
    b.assign(&server, "2026-10", h, 20000);
    b.check(
        &server,
        "2026-10",
        [e(vp, 0, 20000, 20000, 0, 0), zero(lp), e(g, 50000, 0, 50000, 0, 0), e(h, 20000, -50000, -30000, 0, 30000)],
        "Household funded",
    );
}

// ---- 12.4 ----

#[test]
fn spec_12_4_editing_card_spending_moves_its_funding() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.assign(&server, "2026-10", g, 100000);
    let spend = id_of(&b.txn(&server, &b.visa, "2026-10-05", -60000, g));
    let patch = |body: Value| {
        server.patch_transaction_ok(&b.plan, &spend, &body);
    };
    b.check(&server, "2026-10", [e(vp, 0, 60000, 60000, 0, 0), zero(lp), e(g, 100000, -60000, 40000, 0, 0), zero(h)], "created");

    patch(json!({"category_id": h}));
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 100000, 0, 100000, 0, 0), e(h, 0, -60000, -60000, 0, 60000)], "to Household");

    patch(json!({"category_id": g, "amount": -130000}));
    b.check(&server, "2026-10", [e(vp, 0, 100000, 100000, 0, 0), zero(lp), e(g, 100000, -130000, -30000, 0, 30000), zero(h)], "bigger");

    patch(json!({"amount": 25000}));
    b.check(&server, "2026-10", [e(vp, 0, -25000, -25000, 25000, 0), zero(lp), e(g, 100000, 25000, 125000, 0, 0), zero(h)], "a refund");

    patch(json!({"amount": -40000, "date": "2026-11-02"}));
    b.check(&server, "2026-10", [zero(vp), zero(lp), e(g, 100000, 0, 100000, 0, 0), zero(h)], "moved to November");
    b.check(&server, "2026-11", [e(vp, 0, 40000, 40000, 0, 0), zero(lp), e(g, 0, -40000, 60000, 0, 0), zero(h)], "moved to November");

    patch(json!({"subtransactions": [{"amount": -30000, "category_id": g}, {"amount": -10000, "category_id": h}]}));
    b.check(
        &server,
        "2026-11",
        [e(vp, 0, 30000, 30000, 0, 0), zero(lp), e(g, 0, -30000, 70000, 0, 0), e(h, 0, -10000, -10000, 0, 10000)],
        "split",
    );

    patch(json!({"subtransactions": [{"amount": -40000, "category_id": RTA}]}));
    b.check(&server, "2026-11", [zero(vp), zero(lp), e(g, 0, 0, 100000, 0, 0), zero(h)], "split to Ready to Assign");

    patch(json!({"subtransactions": [], "category_id": h}));
    b.check(&server, "2026-11", [zero(vp), zero(lp), e(g, 0, 0, 100000, 0, 0), e(h, 0, -40000, -40000, 0, 40000)], "unsplit");

    assert_status(&server.delete_transaction(&b.plan, &spend), 204, "DELETE");
    b.check(&server, "2026-11", [zero(vp), zero(lp), e(g, 0, 0, 100000, 0, 0), zero(h)], "deleted");
}

// ---- 12.5 ----

#[test]
fn spec_12_5_cash_spending_made_a_transfer_to_a_card_is_a_payment_then_an_advance() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", vp, 50000);
    let out = id_of(&b.txn(&server, &b.checking, "2026-10-05", -50000, g));
    b.check(&server, "2026-10", [e(vp, 50000, 0, 50000, 0, 0), zero(lp), e(g, 0, -50000, -50000, 50000, 0), zero(h)], "spending");

    // Checking pays Visa instead: Visa's half is +50.00.
    let changed = server.patch_transaction_ok(&b.plan, &out, &json!({"payee_id": b.visa.payee}));
    assert_eq!(changed["category_id"], Value::Null, "{changed}");
    let october = b.check(&server, "2026-10", [e(vp, 50000, -50000, 0, 0, 0), zero(lp), zero(g), zero(h)], "a payment");
    assert_matches(&october, &json!({"cash_advances": 0, "activity": 0, "ready_to_assign": 950000}), "a payment");

    // Money into Checking from Visa: Visa's half is -30.00, a cash advance, moved to November.
    server.patch_transaction_ok(&b.plan, &out, &json!({"amount": 30000, "date": "2026-11-03"}));
    let october = b.check(&server, "2026-10", [e(vp, 50000, 0, 50000, 0, 0), zero(lp), zero(g), zero(h)], "an advance");
    assert_matches(&october, &json!({"cash_advances": 0, "ready_to_assign": 950000}), "an advance: October");
    let november = b.check(&server, "2026-11", [e(vp, 0, 0, 50000, 0, 0), zero(lp), zero(g), zero(h)], "an advance");
    assert_matches(&november, &json!({"cash_advances": 30000, "ready_to_assign": 980000}), "an advance: November");

    // Visa's half now goes to Savings: still a cash advance.
    let visa_half = str_field(&server.transaction(&b.plan, &out), "transfer_transaction_id");
    server.patch_transaction_ok(&b.plan, &visa_half, &json!({"payee_id": b.savings.payee}));
    assert_eq!(b.month(&server, "2026-11")["cash_advances"], 30000);

    // Then to Credit Line: between two cards, nothing.
    server.patch_transaction_ok(&b.plan, &visa_half, &json!({"payee_id": b.line.payee}));
    let november = b.check(&server, "2026-11", [e(vp, 0, 0, 50000, 0, 0), zero(lp), zero(g), zero(h)], "between cards");
    assert_matches(&november, &json!({"cash_advances": 0, "ready_to_assign": 950000}), "between cards");

    // Then plain card spending on Household.
    server.patch_transaction_ok(&b.plan, &visa_half, &json!({"payee_id": null, "category_id": h}));
    let november = b.check(&server, "2026-11", [e(vp, 0, 0, 50000, 0, 0), zero(lp), zero(g), e(h, 0, -30000, -30000, 0, 30000)], "spending");
    assert_matches(&november, &json!({"cash_advances": 0, "activity": -30000, "ready_to_assign": 950000}), "spending");
}

// ---- 12.6 ----

#[test]
fn spec_12_6_starting_balances_of_credit_accounts_change_nothing() {
    let server = Server::start();
    let b = CreditBudget::new(&server);
    let (vp, lp, g, h) = (&b.visa_payment, &b.line_payment, &b.groceries, &b.household);
    let create = |name: &str, ty: &str, balance: i64| {
        Card::from(&server.create_account(
            &b.plan,
            &json!({"name": name, "type": ty, "starting_balance": balance, "starting_date": "2026-10-15"}),
        ))
    };
    // A card that starts in credit, and a line of credit that starts in debt.
    let store = create("Store Card", "creditCard", 50000);
    let loan = create("Overdraft", "lineOfCredit", -400000);
    assert_eq!(server.balances(&b.plan, &store.id).0, 50000);
    for month in ["2026-10", "2026-11"] {
        let got = b.month(&server, month);
        assert_matches(
            &got,
            &json!({"month": month, "income": 0, "cash_advances": 0, "assigned": 0, "activity": 0,
                    "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
                    "assigned_in_future": 0, "ready_to_assign": 0,
                    "categories": [zero(vp), zero(lp), zero(&store.payment), zero(&loan.payment), zero(g), zero(h)]}),
            month,
        );
        assert_month_identity(&got);
    }
}
