//! SPEC sections 11 and 12 on generated plans with credit accounts, checked against a model
//! of both: user categories' entries (12.3, with 12.2's funding), payment categories'
//! entries (12.4) and the months' totals (12.5, with 11.6 and 11.7).

use std::collections::BTreeMap;

use mybudget_testsuite::{Acct, Card, Server, Value, assert_matches, assert_month_identity, credit_entry, id_of, json};

const RTA: &str = "ready_to_assign";

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

/// Months are numbered from 2026-05 (0). Data falls in months 2 to 9 (2026-07 to 2027-02);
/// months 0 to 13 are checked.
const CHECKED_MONTHS: usize = 14;

fn month_name(i: usize) -> String {
    let n = 2026 * 12 + 4 + i;
    format!("{:04}-{:02}", n / 12, n % 12 + 1)
}

/// The account a line is in.
#[derive(Clone, Copy)]
enum Place {
    Cash,
    /// A credit account, by its index in creation order.
    Card(usize),
}

/// What the plan holds.
struct Generated {
    plan: String,
    /// Every category in the order months list them: the cards' payment categories, in the
    /// cards' order, then the user categories.
    categories: Vec<String>,
    cards: usize,
    /// Lines: (month, account, user category index or `None` for Ready to Assign, amount).
    lines: Vec<(usize, Place, Option<usize>, i64)>,
    /// Payments (12.5): (month, card, amount).
    payments: Vec<(usize, usize, i64)>,
    /// Cash advances (12.5): (month, size).
    advances: Vec<(usize, i64)>,
    /// Assigned amounts by (month, index into `categories`).
    assigned: BTreeMap<(usize, usize), i64>,
}

/// Builds a plan from `seed`: cash, credit and tracking accounts, three user categories in
/// two groups, every kind of transaction that makes lines, payments or cash advances (or
/// doesn't), and assignments to user and payment categories.
fn generate(server: &Server, seed: u64) -> Generated {
    let mut rng = Rng(seed);
    let plan = server.plan_id("Household");
    let everyday = id_of(&server.create_group(&plan, "Everyday"));
    let groceries = id_of(&server.create_category(&plan, &everyday, "Groceries"));
    let household = id_of(&server.create_category(&plan, &everyday, "Household"));
    // Cash and credit accounts interleaved, and the cards' names in the opposite order to theirs.
    let account = |name: &str, ty: &str| server.add_account(&plan, name, ty, 0);
    let checking = Acct::from(&account("Checking", "checking"));
    let visa = Card::from(&account("Visa", "creditCard"));
    let savings = Acct::from(&account("Savings", "savings"));
    let line = Card::from(&account("Credit Line", "lineOfCredit"));
    let wallet = Acct::from(&account("Wallet", "cash"));
    let amex = Card::from(&account("Amex", "creditCard"));
    let house = Acct::from(&account("House", "otherAsset"));
    let mortgage = Acct::from(&account("Mortgage", "otherLiability"));
    let other = id_of(&server.create_group(&plan, "Other"));
    let fun = id_of(&server.create_category(&plan, &other, "Fun"));

    let cards = [visa, line, amex];
    let cash = [checking, savings, wallet];
    let tracking = [house, mortgage];
    let users = [groceries, household, fun];
    let mut categories: Vec<String> = cards.iter().map(|c| c.payment.clone()).collect();
    categories.extend(users.iter().cloned());

    let mut lines = Vec::new();
    let mut payments = Vec::new();
    let mut advances = Vec::new();
    let cat_id = |c: Option<usize>| c.map_or(RTA.to_owned(), |i| users[i].clone());
    // A category for a cash line, and an amount that suits it.
    let pick_cash = |rng: &mut Rng| -> (Option<usize>, i64) {
        if rng.below(4) == 0 {
            (None, rng.range(-100000, 1500000))
        } else {
            (Some(rng.below(3) as usize), rng.range(-300000, 60000))
        }
    };
    // A category for a credit line: mostly spending, sometimes refunds.
    let pick_card = |rng: &mut Rng| -> (Option<usize>, i64) {
        if rng.below(5) == 0 {
            (None, rng.range(-300000, 100000))
        } else {
            (Some(rng.below(3) as usize), rng.range(-250000, 90000))
        }
    };
    let post = |body: Value| {
        server.create_transaction(&plan, &body);
    };

    for _ in 0..50 {
        let month = rng.range(2, 9) as usize;
        let date = format!("{}-{:02}", month_name(month), rng.range(1, 28));
        match rng.below(20) {
            0..=3 => {
                let a = rng.below(3) as usize;
                let (c, amount) = pick_cash(&mut rng);
                post(json!({"account_id": cash[a].id, "date": date, "amount": amount, "category_id": cat_id(c)}));
                lines.push((month, Place::Cash, c, amount));
            }
            4..=8 | 19 => {
                let k = rng.below(3) as usize;
                let (c, amount) = pick_card(&mut rng);
                post(json!({"account_id": cards[k].id, "date": date, "amount": amount, "category_id": cat_id(c)}));
                lines.push((month, Place::Card(k), c, amount));
            }
            9 => {
                let a = rng.below(3) as usize;
                let mut subs = Vec::new();
                let mut total = 0;
                for _ in 0..rng.range(1, 3) {
                    let (c, amount) = pick_cash(&mut rng);
                    subs.push(json!({"amount": amount, "category_id": cat_id(c)}));
                    total += amount;
                    lines.push((month, Place::Cash, c, amount));
                }
                post(json!({"account_id": cash[a].id, "date": date, "amount": total, "subtransactions": subs}));
            }
            10 => {
                let k = rng.below(3) as usize;
                let mut subs = Vec::new();
                let mut total = 0;
                for _ in 0..rng.range(2, 4) {
                    let (c, amount) = pick_card(&mut rng);
                    subs.push(json!({"amount": amount, "category_id": cat_id(c)}));
                    total += amount;
                    lines.push((month, Place::Card(k), c, amount));
                }
                post(json!({"account_id": cards[k].id, "date": date, "amount": total, "subtransactions": subs}));
            }
            11 => {
                // Only a tracking account: no line.
                let t = rng.below(2) as usize;
                post(json!({"account_id": tracking[t].id, "date": date, "amount": rng.range(-500000, 500000)}));
            }
            12 => {
                // Between a tracking account and a cash account: the cash half is a line.
                let (a, t) = (rng.below(3) as usize, rng.below(2) as usize);
                let (c, amount) = pick_cash(&mut rng);
                if rng.below(2) == 0 {
                    post(json!({"account_id": tracking[t].id, "date": date, "amount": -amount,
                                "payee_id": cash[a].payee, "category_id": cat_id(c)}));
                } else {
                    post(json!({"account_id": cash[a].id, "date": date, "amount": amount,
                                "payee_id": tracking[t].payee, "category_id": cat_id(c)}));
                }
                lines.push((month, Place::Cash, c, amount));
            }
            13 => {
                // Between a tracking account and a card: the card's half is a line.
                let (k, t) = (rng.below(3) as usize, rng.below(2) as usize);
                let (c, amount) = pick_card(&mut rng);
                if rng.below(2) == 0 {
                    post(json!({"account_id": tracking[t].id, "date": date, "amount": -amount,
                                "payee_id": cards[k].payee, "category_id": cat_id(c)}));
                } else {
                    post(json!({"account_id": cards[k].id, "date": date, "amount": amount,
                                "payee_id": tracking[t].payee, "category_id": cat_id(c)}));
                }
                lines.push((month, Place::Card(k), c, amount));
            }
            14..=16 => {
                // Between a cash account and a card: a payment or a cash advance.
                let (a, k) = (rng.below(3) as usize, rng.below(3) as usize);
                let card_half =
                    if rng.below(3) == 0 { -rng.range(10000, 150000) } else { rng.range(10000, 300000) };
                if rng.below(2) == 0 {
                    post(json!({"account_id": cards[k].id, "date": date, "amount": card_half, "payee_id": cash[a].payee}));
                } else {
                    post(json!({"account_id": cash[a].id, "date": date, "amount": -card_half, "payee_id": cards[k].payee}));
                }
                if card_half > 0 {
                    payments.push((month, k, card_half));
                } else {
                    advances.push((month, -card_half));
                }
            }
            17 => {
                // Between two cards: nothing.
                let k = rng.below(3) as usize;
                let to = (k + 1 + rng.below(2) as usize) % 3;
                post(json!({"account_id": cards[k].id, "date": date, "amount": rng.range(-200000, 200000),
                            "payee_id": cards[to].payee}));
            }
            _ => {
                // Between two cash accounts: nothing.
                let a = rng.below(3) as usize;
                let to = (a + 1 + rng.below(2) as usize) % 3;
                post(json!({"account_id": cash[a].id, "date": date, "amount": rng.range(-400000, 400000),
                            "payee_id": cash[to].payee}));
            }
        }
    }
    let mut assigned = BTreeMap::new();
    for _ in 0..30 {
        let month = rng.range(2, 9) as usize;
        let i = rng.below(categories.len() as u64) as usize;
        let amount = rng.range(-100000, 500000);
        server.assign(&plan, &month_name(month), &categories[i], amount);
        assigned.insert((month, i), amount);
    }
    Generated { plan, categories, cards: cards.len(), lines, payments, advances, assigned }
}

/// The months sections 11 and 12 give for the generated plan, months 0 to 13. Nothing
/// happens before month 0 or after month 9, so the model needn't look further.
fn model(g: &Generated) -> Vec<Value> {
    let n = CHECKED_MONTHS;
    let cards = g.cards;
    let total = g.categories.len();
    let users = total - cards;
    let mut income = vec![0i64; n];
    let mut cash_lines = vec![vec![0i64; users]; n];
    // card_lines[m][k][c]: the sum of month m's lines in card k with user category c.
    let mut card_lines = vec![vec![vec![0i64; users]; cards]; n];
    for &(m, place, c, amount) in &g.lines {
        match (place, c) {
            (Place::Cash, None) => income[m] += amount,
            (Place::Cash, Some(c)) => cash_lines[m][c] += amount,
            (Place::Card(_), None) => {}
            (Place::Card(k), Some(c)) => card_lines[m][k][c] += amount,
        }
    }
    let mut payments = vec![vec![0i64; cards]; n];
    for &(m, k, amount) in &g.payments {
        payments[m][k] += amount;
    }
    let mut advances = vec![0i64; n];
    for &(m, size) in &g.advances {
        advances[m] += size;
    }
    let assigned = |m: usize, i: usize| g.assigned.get(&(m, i)).copied().unwrap_or(0);

    let mut available = vec![vec![0i64; total]; n];
    let mut entries = vec![Vec::new(); n];
    let mut cash_overspending = vec![0i64; n];
    let mut activity_total = vec![0i64; n];
    for m in 0..n {
        let carry: Vec<i64> = (0..total).map(|i| if m == 0 { 0 } else { available[m - 1][i].max(0) }).collect();
        let mut payment_activity = vec![0i64; cards];
        let mut user_entries = Vec::new();
        for c in 0..users {
            let i = cards + c;
            let nets: Vec<i64> = (0..cards).map(|k| card_lines[m][k][c]).collect();
            let activity = cash_lines[m][c] + nets.iter().sum::<i64>();
            let avail = carry[i] + assigned(m, i) + activity;
            let spending: Vec<i64> = nets.iter().map(|&x| (-x).max(0)).collect();
            let refunds: Vec<i64> = nets.iter().map(|&x| x.max(0)).collect();
            let funds = carry[i] + assigned(m, i) + cash_lines[m][c] + refunds.iter().sum::<i64>();
            let spent: i64 = spending.iter().sum();
            let funded = spent.min(funds.max(0));
            let mut left = funded;
            for k in 0..cards {
                let take = spending[k].min(left);
                left -= take;
                payment_activity[k] += take - refunds[k];
            }
            let credit = spent - funded;
            let cash = (-avail).max(0) - credit;
            available[m][i] = avail;
            cash_overspending[m] += cash;
            activity_total[m] += activity;
            user_entries.push(credit_entry(&g.categories[i], assigned(m, i), activity, avail, cash, credit));
        }
        for k in 0..cards {
            let activity = payment_activity[k] - payments[m][k];
            let avail = carry[k] + assigned(m, k) + activity;
            let cash = (-avail).max(0);
            available[m][k] = avail;
            cash_overspending[m] += cash;
            entries[m].push(credit_entry(&g.categories[k], assigned(m, k), activity, avail, cash, 0));
        }
        entries[m].extend(user_entries);
    }

    let assigned_total: Vec<i64> = (0..n).map(|m| (0..total).map(|i| assigned(m, i)).sum()).collect();
    let net: Vec<i64> = (0..n).map(|m| income[m] + advances[m] - assigned_total[m]).collect();
    let mut months = Vec::new();
    let mut left_over = 0;
    for m in 0..n {
        let cos_last = if m == 0 { 0 } else { cash_overspending[m - 1] };
        let pool = left_over + net[m] - cos_last;
        let mut shortfall = 0;
        let mut running = 0;
        for later in &net[m + 1..] {
            running += later;
            shortfall = shortfall.max(-running);
        }
        let in_future = if pool < 0 { 0 } else { pool.min(shortfall) };
        months.push(json!({
            "month": month_name(m), "income": income[m], "cash_advances": advances[m],
            "assigned": assigned_total[m], "activity": activity_total[m],
            "cash_overspending_last_month": cos_last, "left_over_from_last_month": left_over,
            "assigned_in_future": in_future, "ready_to_assign": pool - in_future,
            "categories": entries[m],
        }));
        left_over += net[m] - cos_last;
    }
    months
}

const SEEDS: [u64; 6] = [0x9E3779B97F4A7C15, 0x2545F4914F6CDD1D, 12345, 0xDEADBEEF, 0xC0FFEE, 0x5DEECE66D];

/// For each seed and checked month, the server's month and the model's.
fn compare(mut check: impl FnMut(&Generated, &Server, &str, &Value, &Value)) {
    for seed in SEEDS {
        let server = Server::start();
        let g = generate(&server, seed);
        for expected in model(&g) {
            let month = expected["month"].as_str().unwrap().to_owned();
            let got = server.month(&g.plan, &month);
            check(&g, &server, &format!("seed {seed:#x}, {month}"), &got, &expected);
        }
    }
}

/// The entry for `category` in the month's `categories`.
fn entry_in(month: &Value, category: &Value, what: &str) -> Value {
    let list = month["categories"].as_array().unwrap_or_else(|| panic!("{what}: `categories` isn't an array: {month}"));
    let found: Vec<&Value> = list.iter().filter(|e| &e["category_id"] == category).collect();
    assert_eq!(found.len(), 1, "{what}: month lists {category} {} times: {month}", found.len());
    found[0].clone()
}

#[test]
fn spec_12_3_generated_plans_match_the_model() {
    compare(|g, _, what, got, expected| {
        for want in &expected["categories"].as_array().unwrap()[g.cards..] {
            let entry = entry_in(got, &want["category_id"], what);
            assert_matches(&entry, want, &format!("{what}: user category"));
        }
    });
}

#[test]
fn spec_12_4_generated_plans_match_the_model() {
    compare(|g, server, what, got, expected| {
        let month = expected["month"].as_str().unwrap();
        for want in &expected["categories"].as_array().unwrap()[..g.cards] {
            let entry = entry_in(got, &want["category_id"], what);
            assert_matches(&entry, want, &format!("{what}: payment category"));
            let category = want["category_id"].as_str().unwrap();
            assert_matches(&server.month_entry(&g.plan, month, category), want, &format!("{what}: GET payment entry"));
        }
    });
}

#[test]
fn spec_12_5_generated_plans_match_the_model() {
    compare(|_, _, what, got, expected| {
        let mut totals = expected.clone();
        totals.as_object_mut().unwrap().remove("categories");
        assert_matches(got, &totals, what);
        assert_month_identity(got);
    });
}
