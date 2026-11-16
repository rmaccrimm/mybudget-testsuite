//! SPEC 11.7 and 11.8: Ready to Assign, for plans without credit accounts. Also checks
//! category entries (11.3 to 11.5) and months (11.6) against a model on generated plans.

use std::collections::BTreeMap;

use mybudget_testsuite::{Acct, CashBudget, MONTH_FIELDS, Server, Value, assert_fields, assert_matches, entry, id_of, json};

const RTA: &str = "ready_to_assign";

/// The Ready to Assign fields of a month (11.6, 11.7).
fn rta(month: &str, income: i64, assigned: i64, cos_last: i64, left_over: i64, in_future: i64, ready: i64) -> Value {
    json!({"month": month, "income": income, "cash_advances": 0, "assigned": assigned,
           "cash_overspending_last_month": cos_last, "left_over_from_last_month": left_over,
           "assigned_in_future": in_future, "ready_to_assign": ready})
}

/// The month must have `expected`'s fields and satisfy 11.7's closing identity.
fn check_month(b: &CashBudget, server: &Server, expected: &Value) {
    let month = expected["month"].as_str().unwrap();
    let got = b.month(server, month);
    assert_matches(&got, expected, month);
    assert_identity(&got);
}

/// `ready_to_assign` = `left_over_from_last_month` + `income` + `cash_advances`
/// - `cash_overspending_last_month` - `assigned` - `assigned_in_future` (11.7).
fn assert_identity(m: &Value) {
    let f = |k: &str| m[k].as_i64().unwrap_or_else(|| panic!("`{k}` isn't an integer: {m}"));
    let expected = f("left_over_from_last_month") + f("income") + f("cash_advances")
        - f("cash_overspending_last_month")
        - f("assigned")
        - f("assigned_in_future");
    assert_eq!(f("ready_to_assign"), expected, "11.7's identity doesn't hold: {m}");
    assert!(f("assigned_in_future") >= 0, "negative assigned_in_future: {m}");
}

// ---- 11.7 ----

#[test]
fn spec_11_7_assigning_ahead_holds_money_back_in_earlier_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 300000);
    b.assign(&server, "2026-11", &b.household, 200000);
    b.assign(&server, "2026-12", &b.groceries, 400000);
    for expected in [
        rta("2026-09", 0, 0, 0, 0, 0, 0),
        rta("2026-10", 1000000, 300000, 0, 0, 600000, 100000),
        rta("2026-11", 0, 200000, 0, 700000, 400000, 100000),
        rta("2026-12", 0, 400000, 0, 500000, 0, 100000),
        rta("2027-01", 0, 0, 0, 100000, 0, 100000),
        rta("2031-05", 0, 0, 0, 100000, 0, 100000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_over_assigning_a_later_month_makes_that_month_negative() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 1000000);
    b.assign(&server, "2026-11", &b.household, 500000);
    for expected in [
        rta("2026-09", 0, 0, 0, 0, 0, 0),
        rta("2026-10", 1000000, 1000000, 0, 0, 0, 0),
        rta("2026-11", 0, 500000, 0, 0, 0, -500000),
        rta("2026-12", 0, 0, 0, -500000, 0, -500000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_positive_pool_is_never_negative_because_of_later_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 300000, RTA);
    b.assign(&server, "2026-11", &b.groceries, 500000);
    for expected in [
        rta("2026-09", 0, 0, 0, 0, 0, 0),
        rta("2026-10", 300000, 0, 0, 0, 300000, 0),
        rta("2026-11", 0, 500000, 0, 300000, 0, -200000),
        rta("2026-12", 0, 0, 0, -200000, 0, -200000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_months_with_nothing_before_later_assignments_show_zero() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.assign(&server, "2026-11", &b.groceries, 500000);
    for expected in [
        rta("1900-01", 0, 0, 0, 0, 0, 0),
        rta("2026-10", 0, 0, 0, 0, 0, 0),
        rta("2026-11", 0, 500000, 0, 0, 0, -500000),
        rta("2026-12", 0, 0, 0, -500000, 0, -500000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_over_assigning_this_month_stays_negative_until_new_income() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 1500000);
    check_month(&b, &server, &rta("2026-10", 1000000, 1500000, 0, 0, 0, -500000));
    check_month(&b, &server, &rta("2026-11", 0, 0, 0, -500000, 0, -500000));
    b.txn(&server, &b.savings, "2026-11-15", 800000, RTA);
    for expected in [
        rta("2026-10", 1000000, 1500000, 0, 0, 0, -500000),
        rta("2026-11", 800000, 0, 0, -500000, 0, 300000),
        rta("2026-12", 0, 0, 0, 300000, 0, 300000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_later_income_covers_later_assignments_first() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-11", &b.groceries, 500000);
    b.txn(&server, &b.checking, "2026-12-15", 600000, RTA);
    b.assign(&server, "2027-01", &b.household, 700000);
    for expected in [
        rta("2026-10", 1000000, 0, 0, 0, 600000, 400000),
        rta("2026-11", 0, 500000, 0, 1000000, 100000, 400000),
        rta("2026-12", 600000, 0, 0, 500000, 700000, 400000),
        rta("2027-01", 0, 700000, 0, 1100000, 0, 400000),
        rta("2027-02", 0, 0, 0, 400000, 0, 400000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_shortfall_takes_the_worst_later_month() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-11", &b.groceries, 600000);
    b.txn(&server, &b.wallet, "2026-12-03", 1000000, RTA);
    for expected in [
        rta("2026-10", 1000000, 0, 0, 0, 600000, 400000),
        rta("2026-11", 0, 600000, 0, 1000000, 0, 400000),
        rta("2026-12", 1000000, 0, 0, 400000, 0, 1400000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_negative_assignments_free_money() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 1000000);
    b.assign(&server, "2026-11", &b.groceries, -300000);
    for expected in [
        rta("2026-10", 1000000, 1000000, 0, 0, 0, 0),
        rta("2026-11", 0, -300000, 0, 0, 0, 300000),
        rta("2026-12", 0, 0, 0, 300000, 0, 300000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_cash_overspending_comes_out_of_the_following_months() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.assign(&server, "2026-10", &b.groceries, 100000);
    b.txn(&server, &b.checking, "2026-10-10", -150000, &b.groceries);
    // This month's overspending leaves this month's Ready to Assign alone.
    check_month(&b, &server, &rta("2026-10", 1000000, 100000, 0, 0, 0, 900000));
    check_month(&b, &server, &rta("2026-11", 0, 0, 50000, 900000, 0, 850000));
    check_month(&b, &server, &rta("2026-12", 0, 0, 0, 850000, 0, 850000));
    b.assign(&server, "2026-11", &b.household, 300000);
    for expected in [
        rta("2026-10", 1000000, 100000, 0, 0, 300000, 600000),
        rta("2026-11", 0, 300000, 50000, 900000, 0, 550000),
        rta("2026-12", 0, 0, 0, 550000, 0, 550000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_overspending_in_consecutive_months_adds_up() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA);
    b.txn(&server, &b.checking, "2026-10-10", -40000, &b.groceries);
    b.txn(&server, &b.checking, "2026-11-10", -60000, &b.groceries);
    b.txn(&server, &b.wallet, "2026-11-11", -5000, &b.household);
    for expected in [
        rta("2026-10", 1000000, 0, 0, 0, 0, 1000000),
        rta("2026-11", 0, 0, 40000, 1000000, 0, 960000),
        rta("2026-12", 0, 0, 65000, 960000, 0, 895000),
        rta("2027-01", 0, 0, 0, 895000, 0, 895000),
    ] {
        check_month(&b, &server, &expected);
    }
}

#[test]
fn spec_11_7_spans_the_whole_month_range() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    b.txn(&server, &b.checking, "1900-01-01", 1000000, RTA);
    b.assign(&server, "2999-12", &b.groceries, 400000);
    for expected in [
        rta("1900-01", 1000000, 0, 0, 0, 400000, 600000),
        rta("2026-10", 0, 0, 0, 1000000, 400000, 600000),
        rta("2999-11", 0, 0, 0, 1000000, 400000, 600000),
        rta("2999-12", 0, 400000, 0, 1000000, 0, 600000),
    ] {
        check_month(&b, &server, &expected);
    }
    assert_matches(&b.entry(&server, "2999-12", &b.groceries), &entry(&b.groceries, 400000, 0, 400000, 0), "2999-12");
}

#[test]
fn spec_11_7_follows_transaction_and_assignment_changes() {
    let server = Server::start();
    let b = CashBudget::new(&server);
    let pay = id_of(&b.txn(&server, &b.checking, "2026-10-01", 1000000, RTA));
    b.assign(&server, "2026-10", &b.groceries, 200000);
    check_month(&b, &server, &rta("2026-10", 1000000, 200000, 0, 0, 0, 800000));
    server.patch_transaction_ok(&b.plan, &pay, &json!({"date": "2026-11-01"}));
    check_month(&b, &server, &rta("2026-10", 0, 200000, 0, 0, 0, -200000));
    check_month(&b, &server, &rta("2026-11", 1000000, 0, 0, -200000, 0, 800000));
    server.patch_transaction_ok(&b.plan, &pay, &json!({"category_id": b.household}));
    check_month(&b, &server, &rta("2026-11", 0, 0, 0, -200000, 0, -200000));
    server.patch_transaction_ok(&b.plan, &pay, &json!({"category_id": RTA, "date": "2026-10-01", "amount": 300000}));
    b.assign(&server, "2026-10", &b.groceries, 100000);
    check_month(&b, &server, &rta("2026-10", 300000, 100000, 0, 0, 0, 200000));
    assert_eq!(server.delete_transaction(&b.plan, &pay).status, 204);
    check_month(&b, &server, &rta("2026-10", 0, 100000, 0, 0, 0, -100000));
}

// ---- 11.8 ----

/// 11.8's plan: returns the plan and the ids of Groceries, Rent and Dining.
fn example_plan(server: &Server) -> (String, String, String, String) {
    let plan = server.plan_id("Household");
    let checking = id_of(&server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "starting_balance": 3000000, "starting_date": "2026-10-01"}),
    ));
    let group = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &group, "Groceries"));
    let rent = id_of(&server.create_category(&plan, &group, "Rent"));
    let dining = id_of(&server.create_category(&plan, &group, "Dining"));
    server.assign(&plan, "2026-10", &groceries, 400000);
    server.assign(&plan, "2026-10", &rent, 1200000);
    server.assign(&plan, "2026-10", &dining, 50000);
    server.assign(&plan, "2026-11", &rent, 1200000);
    server.create_transaction(
        &plan,
        &json!({"account_id": checking, "date": "2026-10-04", "amount": -120000, "category_id": groceries}),
    );
    server.create_transaction(
        &plan,
        &json!({"account_id": checking, "date": "2026-10-11", "amount": -80000, "category_id": dining}),
    );
    (plan, groceries, rent, dining)
}

#[test]
fn spec_11_8_october_matches_example() {
    let server = Server::start();
    let (plan, groceries, rent, dining) = example_plan(&server);
    let got = server.month(&plan, "2026-10");
    assert_fields(&got, &MONTH_FIELDS, "October");
    assert_matches(
        &got,
        &json!({"month": "2026-10", "income": 3000000, "cash_advances": 0, "assigned": 1650000,
                "activity": -200000, "cash_overspending_last_month": 0, "left_over_from_last_month": 0,
                "assigned_in_future": 1200000, "ready_to_assign": 150000,
                "categories": [
                    {"category_id": groceries, "assigned": 400000, "activity": -120000, "available": 280000,
                     "cash_overspending": 0, "credit_overspending": 0},
                    {"category_id": rent, "assigned": 1200000, "activity": 0, "available": 1200000,
                     "cash_overspending": 0, "credit_overspending": 0},
                    {"category_id": dining, "assigned": 50000, "activity": -80000, "available": -30000,
                     "cash_overspending": 30000, "credit_overspending": 0}]}),
        "11.8's October",
    );
}

#[test]
fn spec_11_8_november_matches_example() {
    let server = Server::start();
    let (plan, groceries, rent, dining) = example_plan(&server);
    let got = server.month(&plan, "2026-11");
    assert_fields(&got, &MONTH_FIELDS, "November");
    assert_matches(
        &got,
        &json!({"month": "2026-11", "income": 0, "cash_advances": 0, "assigned": 1200000,
                "activity": 0, "cash_overspending_last_month": 30000, "left_over_from_last_month": 1350000,
                "assigned_in_future": 0, "ready_to_assign": 120000,
                "categories": [
                    {"category_id": groceries, "assigned": 0, "activity": 0, "available": 280000,
                     "cash_overspending": 0, "credit_overspending": 0},
                    {"category_id": rent, "assigned": 1200000, "activity": 0, "available": 2400000,
                     "cash_overspending": 0, "credit_overspending": 0},
                    {"category_id": dining, "assigned": 0, "activity": 0, "available": 0,
                     "cash_overspending": 0, "credit_overspending": 0}]}),
        "11.8's November",
    );
}

// ---- Generated plans, checked against a model of 11.1 to 11.7 ----

/// A small deterministic generator (xorshift64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// An integer from `lo` to `hi` inclusive.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
}

/// Months are numbered from 2026-05 (0). Data falls in months 2 to 11 (2026-07 to
/// 2027-04); months 0 to 13 are checked.
const CHECKED_MONTHS: usize = 14;

fn month_name(i: usize) -> String {
    let n = 2026 * 12 + 4 + i;
    format!("{:04}-{:02}", n / 12, n % 12 + 1)
}

/// What the plan holds: lines as (month, category index or `None` for Ready to Assign,
/// amount), and assigned amounts by (month, category index).
struct Generated {
    b: CashBudget,
    categories: Vec<String>,
    lines: Vec<(usize, Option<usize>, i64)>,
    assigned: BTreeMap<(usize, usize), i64>,
}

/// Builds a plan from `seed`: cash and tracking accounts, three categories in two groups,
/// transactions of every kind that makes lines (or doesn't), and assignments.
fn generate(server: &Server, seed: u64) -> Generated {
    let mut rng = Rng(seed);
    let b = CashBudget::new(server);
    let other = id_of(&server.create_group(&b.plan, "Other"));
    let fun = id_of(&server.create_category(&b.plan, &other, "Fun"));
    let categories = vec![b.groceries.clone(), b.household.clone(), fun];
    let cash: [&Acct; 3] = [&b.checking, &b.savings, &b.wallet];
    let mut lines = Vec::new();
    let mut assigned = BTreeMap::new();

    // A category index or None (Ready to Assign), and an amount that suits it.
    let pick = |rng: &mut Rng| -> (Option<usize>, i64) {
        if rng.below(4) == 0 {
            (None, rng.range(-100000, 1500000))
        } else {
            (Some(rng.below(3) as usize), rng.range(-300000, 60000))
        }
    };
    let cat_id = |c: Option<usize>| c.map_or(RTA.to_owned(), |i| categories[i].clone());

    for _ in 0..30 {
        let month = rng.range(2, 11) as usize;
        let date = format!("{}-{:02}", month_name(month), rng.range(1, 28));
        let account = cash[rng.below(3) as usize];
        match rng.below(11) {
            0..=5 => {
                let (c, amount) = pick(&mut rng);
                server.create_transaction(
                    &b.plan,
                    &json!({"account_id": account.id, "date": date, "amount": amount, "category_id": cat_id(c)}),
                );
                lines.push((month, c, amount));
            }
            6 | 7 => {
                let mut subs = Vec::new();
                let mut total = 0;
                for _ in 0..rng.range(1, 3) {
                    let (c, amount) = pick(&mut rng);
                    subs.push(json!({"amount": amount, "category_id": cat_id(c)}));
                    total += amount;
                    lines.push((month, c, amount));
                }
                server.create_transaction(
                    &b.plan,
                    &json!({"account_id": account.id, "date": date, "amount": total, "subtransactions": subs}),
                );
            }
            8 => {
                // Only a tracking account: no line.
                server.create_transaction(
                    &b.plan,
                    &json!({"account_id": b.house.id, "date": date, "amount": rng.range(-500000, 500000)}),
                );
            }
            9 => {
                // From the house into a cash account: the cash half is a line.
                let (c, amount) = pick(&mut rng);
                server.create_transaction(
                    &b.plan,
                    &json!({"account_id": b.house.id, "date": date, "amount": -amount, "payee_id": account.payee,
                            "category_id": cat_id(c)}),
                );
                lines.push((month, c, amount));
            }
            _ => {
                // Between two cash accounts: lines without a category.
                let to = cash[(rng.below(2) as usize + 1 + cash.iter().position(|a| a.id == account.id).unwrap()) % 3];
                server.create_transaction(
                    &b.plan,
                    &json!({"account_id": account.id, "date": date, "amount": rng.range(-400000, 400000),
                            "payee_id": to.payee}),
                );
            }
        }
    }
    for _ in 0..25 {
        let month = rng.range(2, 11) as usize;
        let c = rng.below(3) as usize;
        let amount = rng.range(-100000, 600000);
        server.assign(&b.plan, &month_name(month), &categories[c], amount);
        assigned.insert((month, c), amount);
    }
    Generated { b, categories, lines, assigned }
}

/// The months 11.3 to 11.7 give for the generated plan, months 0 to 13. Nothing happens
/// before month 0 or after month 11, so the model needn't look further.
fn model(g: &Generated) -> Vec<Value> {
    let n = CHECKED_MONTHS;
    let k = g.categories.len();
    let mut activity = vec![vec![0i64; k]; n];
    let mut income = vec![0i64; n];
    for &(m, c, amount) in &g.lines {
        match c {
            Some(c) => activity[m][c] += amount,
            None => income[m] += amount,
        }
    }
    let assigned = |m: usize, c: usize| g.assigned.get(&(m, c)).copied().unwrap_or(0);
    let mut available = vec![vec![0i64; k]; n];
    let mut cos_last = vec![0i64; n];
    let mut net = vec![0i64; n];
    for m in 0..n {
        for c in 0..k {
            let carry = if m == 0 { 0 } else { available[m - 1][c].max(0) };
            available[m][c] = carry + assigned(m, c) + activity[m][c];
            if m + 1 < n {
                cos_last[m + 1] += (-available[m][c]).max(0);
            }
        }
        net[m] = income[m] - (0..k).map(|c| assigned(m, c)).sum::<i64>();
    }
    let mut months = Vec::new();
    let mut left_over = 0;
    for m in 0..n {
        let pool = left_over + net[m] - cos_last[m];
        let mut shortfall = 0;
        let mut running = 0;
        for later in &net[m + 1..] {
            running += later;
            shortfall = shortfall.max(-running);
        }
        let in_future = if pool < 0 { 0 } else { pool.min(shortfall) };
        let entries: Vec<Value> = (0..k)
            .map(|c| entry(&g.categories[c], assigned(m, c), activity[m][c], available[m][c], (-available[m][c]).max(0)))
            .collect();
        months.push(json!({
            "month": month_name(m), "income": income[m], "cash_advances": 0,
            "assigned": (0..k).map(|c| assigned(m, c)).sum::<i64>(),
            "activity": activity[m].iter().sum::<i64>(),
            "cash_overspending_last_month": cos_last[m], "left_over_from_last_month": left_over,
            "assigned_in_future": in_future, "ready_to_assign": pool - in_future,
            "categories": entries,
        }));
        left_over += net[m] - cos_last[m];
    }
    months
}

#[test]
fn spec_11_7_generated_plans_match_the_model() {
    for seed in [0x9E3779B97F4A7C15, 0x2545F4914F6CDD1D, 12345, 0xDEADBEEF] {
        let server = Server::start();
        let g = generate(&server, seed);
        for expected in model(&g) {
            let month = expected["month"].as_str().unwrap().to_owned();
            let got = g.b.month(&server, &month);
            let mut totals = expected.clone();
            totals.as_object_mut().unwrap().remove("categories");
            assert_matches(&got, &totals, &format!("seed {seed:#x}, {month}"));
            assert_identity(&got);
        }
    }
}

#[test]
fn spec_11_4_generated_plans_match_the_model() {
    for seed in [0x9E3779B97F4A7C15, 0x2545F4914F6CDD1D, 12345, 0xDEADBEEF] {
        let server = Server::start();
        let g = generate(&server, seed);
        for expected in model(&g) {
            let month = expected["month"].as_str().unwrap().to_owned();
            let got = g.b.month(&server, &month);
            assert_matches(&got["categories"], &expected["categories"], &format!("seed {seed:#x}, {month} categories"));
        }
    }
}
