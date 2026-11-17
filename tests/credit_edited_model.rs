//! SPEC sections 11 and 12 on plans with credit accounts after random edits: transactions
//! created, changed (amounts, dates, categories, payees to and from transfer payees, splits)
//! and deleted, accounts created with starting balances, closed, reopened and deleted, and
//! categories created and moved. At checkpoints, every month must be what sections 11 and
//! 12 give for the plan's transactions as `GET /transactions` lists them (8.5), its accounts
//! and categories as listed (5.6, 7.1), and the assignments made (11.2).

use std::collections::{BTreeMap, HashMap};

use mybudget_testsuite::{Server, Value, assert_matches, assert_month_identity, credit_entry, id_of, json, str_field};

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

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

/// Months are numbered from 2026-05 (0). Every date and assignment falls in months 2 to 9
/// (2026-07 to 2027-02); months 0 to 13 are checked.
const CHECKED_MONTHS: usize = 14;
const FIRST_MONTH: usize = 2026 * 12 + 4;

fn month_name(i: usize) -> String {
    let n = FIRST_MONTH + i;
    format!("{:04}-{:02}", n / 12, n % 12 + 1)
}

/// The month number of a date in the window.
fn month_of(date: &str) -> usize {
    let year: usize = date[..4].parse().unwrap();
    let month: usize = date[5..7].parse().unwrap();
    let n = year * 12 + month - 1;
    assert!((FIRST_MONTH..FIRST_MONTH + CHECKED_MONTHS).contains(&n), "date {date} outside the window");
    n - FIRST_MONTH
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Cash,
    Credit,
    Tracking,
}

fn kind_of(ty: &str) -> Kind {
    match ty {
        "checking" | "savings" | "cash" => Kind::Cash,
        "creditCard" | "lineOfCredit" => Kind::Credit,
        "otherAsset" | "otherLiability" => Kind::Tracking,
        other => panic!("unknown account type {other}"),
    }
}

const TYPES: [&str; 7] = ["checking", "savings", "cash", "creditCard", "lineOfCredit", "otherAsset", "otherLiability"];

#[derive(Clone, Debug)]
struct Account {
    id: String,
    payee: String,
    kind: Kind,
    /// Its payment category, for a credit account.
    payment: Option<String>,
}

/// The plan as the edits have left it, from the client's side.
struct Edits<'a> {
    server: &'a Server,
    rng: Rng,
    plan: String,
    accounts: Vec<Account>,
    groups: Vec<String>,
    users: Vec<String>,
    /// Assigned amounts by (category, month).
    assigned: BTreeMap<(String, usize), i64>,
    names: usize,
    /// How many transaction `POST`s, `PATCH`es and `DELETE`s succeeded.
    posted: usize,
    patched: usize,
    deleted: usize,
}

impl<'a> Edits<'a> {
    fn new(server: &'a Server, seed: u64) -> Edits<'a> {
        let plan = server.plan_id("Household");
        let mut edits = Edits {
            server,
            rng: Rng(seed),
            plan,
            accounts: Vec::new(),
            groups: Vec::new(),
            users: Vec::new(),
            assigned: BTreeMap::new(),
            names: 0,
            posted: 0,
            patched: 0,
            deleted: 0,
        };
        // Cards and cash accounts interleaved, and a tracking account of each kind.
        for ty in ["checking", "creditCard", "savings", "lineOfCredit", "cash", "creditCard", "otherAsset", "otherLiability"] {
            edits.create_account(ty, 0);
        }
        for group in ["Everyday", "Bills"] {
            let id = id_of(&server.create_group(&edits.plan, group));
            edits.groups.push(id);
        }
        for i in 0..4 {
            edits.create_category(i % 2);
        }
        edits
    }

    fn name(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix} {}", self.names)
    }

    fn date(&mut self) -> String {
        let month = self.rng.range(2, 9) as usize;
        format!("{}-{:02}", month_name(month), self.rng.range(1, 28))
    }

    /// A category for a line: a user category, or now and then Ready to Assign.
    fn category(&mut self) -> String {
        if self.rng.below(5) == 0 { RTA.to_owned() } else { self.rng.pick(&self.users).clone() }
    }

    /// Between one and three subtransactions adding up to `total`.
    fn subtransactions(&mut self, total: i64) -> Vec<Value> {
        let n = self.rng.range(1, 3);
        let mut left = total;
        let mut subs = Vec::new();
        for i in 0..n {
            let amount = if i == n - 1 { left } else { self.rng.range(-150000, 60000) };
            left -= amount;
            let category = self.category();
            subs.push(json!({"amount": amount, "category_id": category}));
        }
        subs
    }

    fn account_by_id(&self, id: &str) -> Account {
        self.accounts.iter().find(|a| a.id == id).unwrap_or_else(|| panic!("no account {id}")).clone()
    }

    /// An account other than `not`.
    fn other_account(&mut self, not: &str) -> Account {
        let others: Vec<Account> = self.accounts.iter().filter(|a| a.id != not).cloned().collect();
        self.rng.pick(&others).clone()
    }

    fn create_account(&mut self, ty: &str, starting_balance: i64) {
        let name = self.name("Account");
        let mut body = json!({"name": name, "type": ty});
        if starting_balance != 0 {
            body["starting_balance"] = json!(starting_balance);
            body["starting_date"] = json!(self.date());
        }
        let a = self.server.create_account(&self.plan, &body);
        let payment = a["payment_category_id"].as_str().map(str::to_owned);
        self.accounts.push(Account { id: id_of(&a), payee: str_field(&a, "transfer_payee_id"), kind: kind_of(ty), payment });
    }

    fn create_category(&mut self, group: usize) {
        let name = self.name("Category");
        let c = self.server.create_category(&self.plan, &self.groups[group], &name);
        self.users.push(id_of(&c));
    }

    fn post_transaction(&mut self) {
        let a = self.rng.pick(&self.accounts).clone();
        let date = self.date();
        let amount = self.rng.range(-300000, 150000);
        let mut body = json!({"account_id": a.id, "date": date, "amount": amount});
        match self.rng.below(4) {
            0 if a.kind != Kind::Tracking => {
                body["subtransactions"] = json!(self.subtransactions(amount));
            }
            1 => {
                let b = self.other_account(&a.id);
                body["payee_id"] = json!(b.payee);
                if (a.kind == Kind::Tracking) != (b.kind == Kind::Tracking) {
                    body["category_id"] = json!(self.category());
                }
            }
            _ => {
                if a.kind != Kind::Tracking {
                    body["category_id"] = json!(self.category());
                }
            }
        }
        if self.server.post_transaction(&self.plan, &body).status == 201 {
            self.posted += 1;
        }
    }

    fn patch_transaction(&mut self) {
        let txns = self.server.transactions(&self.plan);
        if txns.is_empty() {
            return;
        }
        let t = self.rng.pick(&txns).clone();
        let own = self.account_by_id(&str_field(&t, "account_id"));
        let split = !t["subtransactions"].as_array().unwrap().is_empty();
        let transfer = !t["transfer_account_id"].is_null();
        let mut body = json!({});
        match self.rng.below(7) {
            0 => {
                let amount = self.rng.range(-300000, 150000);
                body["amount"] = json!(amount);
                if split {
                    body["subtransactions"] = json!(self.subtransactions(amount));
                }
            }
            1 => body["date"] = json!(self.date()),
            2 => body["category_id"] = json!(self.category()),
            3 => {
                // To a transfer, or to another account's transfer payee.
                let b = self.other_account(&own.id);
                body["payee_id"] = json!(b.payee);
                if split {
                    body["subtransactions"] = json!([]);
                }
                if (own.kind == Kind::Tracking) != (b.kind == Kind::Tracking) && self.rng.below(3) != 0 {
                    body["category_id"] = json!(self.category());
                }
            }
            4 => {
                // No longer a transfer.
                if self.rng.below(2) == 0 {
                    body["payee_id"] = Value::Null;
                } else {
                    body["payee_name"] = json!("Corner Shop");
                }
                if own.kind != Kind::Tracking {
                    body["category_id"] = json!(self.category());
                }
                if split {
                    body["subtransactions"] = json!([]);
                }
            }
            5 => {
                if transfer {
                    body["payee_id"] = Value::Null;
                }
                let amount = t["amount"].as_i64().unwrap();
                body["subtransactions"] = json!(self.subtransactions(amount));
            }
            _ => {
                body["subtransactions"] = json!([]);
                body["category_id"] = json!(self.category());
            }
        }
        if self.server.patch_transaction(&self.plan, &str_field(&t, "id"), &body).status == 200 {
            self.patched += 1;
        }
    }

    fn delete_transaction(&mut self) {
        let txns = self.server.transactions(&self.plan);
        if !txns.is_empty() {
            let id = str_field(self.rng.pick(&txns), "id");
            if self.server.delete_transaction(&self.plan, &id).status == 204 {
                self.deleted += 1;
            }
        }
    }

    fn assign(&mut self) {
        let mut categories = self.users.clone();
        categories.extend(self.accounts.iter().filter_map(|a| a.payment.clone()));
        let category = self.rng.pick(&categories).clone();
        let month = self.rng.range(2, 9) as usize;
        let amount = self.rng.range(-100000, 500000);
        self.server.assign(&self.plan, &month_name(month), &category, amount);
        self.assigned.insert((category, month), amount);
    }

    fn step(&mut self) {
        match self.rng.below(100) {
            0..=29 => self.post_transaction(),
            30..=54 => self.patch_transaction(),
            55..=61 => self.delete_transaction(),
            62..=79 => self.assign(),
            80..=83 if self.accounts.len() < 12 => {
                let ty = *self.rng.pick(&TYPES);
                let balance = if self.rng.below(2) == 0 { 0 } else { self.rng.range(-500000, 500000) };
                self.create_account(ty, balance);
            }
            84..=86 => {
                let a = self.rng.pick(&self.accounts).clone();
                let closed = self.rng.below(2) == 0;
                self.server.patch(&format!("/plans/{}/accounts/{}", self.plan, a.id), &json!({ "closed": closed }));
            }
            87..=89 if self.accounts.len() > 4 => {
                let a = self.rng.pick(&self.accounts).clone();
                if self.server.delete(&format!("/plans/{}/accounts/{}", self.plan, a.id)).status == 204 {
                    self.accounts.retain(|x| x.id != a.id);
                    if let Some(payment) = &a.payment {
                        self.assigned.retain(|(c, _), _| c != payment);
                    }
                }
            }
            90..=93 if self.users.len() < 7 => {
                if self.rng.below(3) == 0 {
                    let name = self.name("Group");
                    let id = id_of(&self.server.create_group(&self.plan, &name));
                    self.groups.push(id);
                }
                let group = self.rng.below(self.groups.len() as u64) as usize;
                self.create_category(group);
            }
            94..=99 => {
                let category = self.rng.pick(&self.users).clone();
                let group = self.rng.pick(&self.groups).clone();
                self.server.patch(&format!("/plans/{}/categories/{category}", self.plan), &json!({ "group_id": group }));
            }
            _ => self.post_transaction(),
        }
    }
}

/// What sections 11 and 12 give for the plan as the server lists it.
struct Expected {
    /// Months 0 to 13. Nothing happens outside months 2 to 9.
    months: Vec<Value>,
    /// Every category's id, in the order of 7.1.
    order: Vec<String>,
    /// The payment categories' ids.
    payments: Vec<String>,
}

fn model(server: &Server, e: &Edits) -> Expected {
    let n = CHECKED_MONTHS;
    let accounts = server.accounts(&e.plan);
    let kinds: HashMap<String, Kind> = accounts.iter().map(|a| (id_of(a), kind_of(&str_field(a, "type")))).collect();
    // Credit accounts in the order they were created (5.6 lists them oldest first).
    let cards: Vec<String> = accounts.iter().map(id_of).filter(|id| kinds[id] == Kind::Credit).collect();
    let card_index: HashMap<&str, usize> = cards.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();

    // Every category in the order of 7.1: (id, the card it pays, if a payment category).
    let mut categories: Vec<(String, Option<usize>)> = Vec::new();
    for group in server.groups(&e.plan) {
        for c in group["categories"].as_array().unwrap() {
            let card = c["payment_account_id"].as_str().map(|a| card_index[a]);
            categories.push((id_of(c), card));
        }
    }
    let users: Vec<&str> = categories.iter().filter(|(_, k)| k.is_none()).map(|(id, _)| id.as_str()).collect();
    let user_index: HashMap<&str, usize> = users.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let k_count = cards.len();
    let u_count = users.len();

    let mut income = vec![0i64; n];
    let mut advances = vec![0i64; n];
    let mut payments = vec![vec![0i64; k_count]; n];
    let mut cash_lines = vec![vec![0i64; u_count]; n];
    let mut card_lines = vec![vec![vec![0i64; u_count]; k_count]; n];
    for t in server.transactions(&e.plan) {
        let account = str_field(&t, "account_id");
        let kind = kinds[&account];
        if kind == Kind::Tracking {
            continue;
        }
        let m = month_of(&str_field(&t, "date"));
        let amount = t["amount"].as_i64().unwrap();
        if kind == Kind::Credit
            && let Some(other) = t["transfer_account_id"].as_str()
            && kinds[other] == Kind::Cash
        {
            if amount > 0 {
                payments[m][card_index[account.as_str()]] += amount;
            } else {
                advances[m] -= amount;
            }
        }
        let mut line = |category: &str, amount: i64| {
            if category == RTA {
                if kind == Kind::Cash {
                    income[m] += amount;
                }
                return;
            }
            let u = *user_index.get(category).unwrap_or_else(|| panic!("line with category {category}: {t}"));
            match kind {
                Kind::Cash => cash_lines[m][u] += amount,
                _ => card_lines[m][card_index[account.as_str()]][u] += amount,
            }
        };
        let subs = t["subtransactions"].as_array().unwrap();
        if subs.is_empty() {
            if let Some(category) = t["category_id"].as_str() {
                line(category, amount);
            }
        } else {
            for s in subs {
                line(&str_field(s, "category_id"), s["amount"].as_i64().unwrap());
            }
        }
    }
    let assigned = |m: usize, id: &str| e.assigned.get(&(id.to_owned(), m)).copied().unwrap_or(0);

    let mut available: HashMap<&str, i64> = HashMap::new();
    let mut cash_overspending = vec![0i64; n];
    let mut activity_total = vec![0i64; n];
    let mut assigned_total = vec![0i64; n];
    let mut entries = Vec::new();
    for m in 0..n {
        let carry: HashMap<&str, i64> = categories.iter().map(|(id, _)| (id.as_str(), available.get(id.as_str()).copied().unwrap_or(0).max(0))).collect();
        let mut by_id: HashMap<&str, Value> = HashMap::new();
        let mut payment_activity = vec![0i64; k_count];
        for (u, &id) in users.iter().enumerate() {
            let nets: Vec<i64> = (0..k_count).map(|k| card_lines[m][k][u]).collect();
            let activity = cash_lines[m][u] + nets.iter().sum::<i64>();
            let avail = carry[id] + assigned(m, id) + activity;
            let spending: Vec<i64> = nets.iter().map(|&x| (-x).max(0)).collect();
            let refunds: Vec<i64> = nets.iter().map(|&x| x.max(0)).collect();
            let funds = carry[id] + assigned(m, id) + cash_lines[m][u] + refunds.iter().sum::<i64>();
            let spent: i64 = spending.iter().sum();
            let funded = spent.min(funds.max(0));
            let mut left = funded;
            for k in 0..k_count {
                let take = spending[k].min(left);
                left -= take;
                payment_activity[k] += take - refunds[k];
            }
            let credit = spent - funded;
            let cash = (-avail).max(0) - credit;
            available.insert(id, avail);
            cash_overspending[m] += cash;
            activity_total[m] += activity;
            by_id.insert(id, credit_entry(id, assigned(m, id), activity, avail, cash, credit));
        }
        for (id, card) in &categories {
            let Some(k) = card else { continue };
            let activity = payment_activity[*k] - payments[m][*k];
            let avail = carry[id.as_str()] + assigned(m, id) + activity;
            let cash = (-avail).max(0);
            available.insert(id.as_str(), avail);
            cash_overspending[m] += cash;
            by_id.insert(id.as_str(), credit_entry(id, assigned(m, id), activity, avail, cash, 0));
        }
        assigned_total[m] = categories.iter().map(|(id, _)| assigned(m, id)).sum();
        entries.push(categories.iter().map(|(id, _)| by_id[id.as_str()].clone()).collect::<Vec<_>>());
    }

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
    let payments = categories.iter().filter(|(_, k)| k.is_some()).map(|(id, _)| id.clone()).collect();
    Expected { months, order: categories.into_iter().map(|(id, _)| id).collect(), payments }
}

const SEEDS: [u64; 4] = [0x9E3779B97F4A7C15, 0x2545F4914F6CDD1D, 0xA5A5_1234_5678_9ABC, 0x0123_4567_89AB_CDEF];
const STEPS: usize = 360;
const CHECKPOINTS: [usize; 3] = [120, 240, 360];

/// What a check sees: the plan, the label, the server's month and the model's.
struct Seen<'a> {
    server: &'a Server,
    plan: &'a str,
    what: String,
    got: Value,
    expected: Value,
    order: &'a [String],
    payments: &'a [String],
}

/// For each seed and checkpoint, runs `check` on every checked month.
fn compare(mut check: impl FnMut(&Seen)) {
    for seed in SEEDS {
        let server = Server::start();
        let mut edits = Edits::new(&server, seed);
        for step in 1..=STEPS {
            edits.step();
            if CHECKPOINTS.contains(&step) {
                let Expected { months, order, payments } = model(&server, &edits);
                let count = server.transactions(&edits.plan).len();
                eprintln!(
                    "seed {seed:#x}, after {step} edits: {count} transactions; {} POSTs, {} PATCHes and {} DELETEs succeeded",
                    edits.posted, edits.patched, edits.deleted
                );
                // The edits are valid but for a few; make sure enough of them took effect.
                assert!(
                    count >= 15 && edits.patched >= 10,
                    "seed {seed:#x}: only {count} transactions and {} PATCHes after {step} edits",
                    edits.patched
                );
                for expected in months {
                    let month = expected["month"].as_str().unwrap().to_owned();
                    let got = server.month(&edits.plan, &month);
                    check(&Seen {
                        server: &server,
                        plan: &edits.plan,
                        what: format!("seed {seed:#x}, after {step} edits, {month}"),
                        got,
                        expected,
                        order: &order,
                        payments: &payments,
                    });
                }
            }
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

/// The expected entries of payment categories (`payment` true) or user categories.
fn expected_entries(s: &Seen, payment: bool) -> Vec<Value> {
    s.expected["categories"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| s.payments.iter().any(|p| e["category_id"] == p.as_str()) == payment)
        .cloned()
        .collect()
}

#[test]
fn spec_11_6_edited_plans_list_every_category_in_order() {
    compare(|s| {
        let ids: Vec<&str> = s.got["categories"]
            .as_array()
            .unwrap_or_else(|| panic!("{}: `categories` isn't an array: {}", s.what, s.got))
            .iter()
            .map(|e| e["category_id"].as_str().unwrap_or_else(|| panic!("{}: entry without category_id: {e}", s.what)))
            .collect();
        assert_eq!(ids, s.order.iter().map(String::as_str).collect::<Vec<_>>(), "{}", s.what);
    });
}

#[test]
fn spec_12_3_edited_plans_match_the_model() {
    compare(|s| {
        for want in expected_entries(s, false) {
            let entry = entry_in(&s.got, &want["category_id"], &s.what);
            assert_matches(&entry, &want, &format!("{}: user category", s.what));
        }
    });
}

#[test]
fn spec_12_4_edited_plans_match_the_model() {
    compare(|s| {
        let month = s.expected["month"].as_str().unwrap();
        for want in expected_entries(s, true) {
            let entry = entry_in(&s.got, &want["category_id"], &s.what);
            assert_matches(&entry, &want, &format!("{}: payment category", s.what));
            let category = want["category_id"].as_str().unwrap();
            assert_matches(&s.server.month_entry(s.plan, month, category), &want, &format!("{}: GET payment entry", s.what));
        }
    });
}

#[test]
fn spec_12_5_edited_plans_match_the_model() {
    compare(|s| {
        let mut totals = s.expected.clone();
        totals.as_object_mut().unwrap().remove("categories");
        assert_matches(&s.got, &totals, &s.what);
        assert_month_identity(&s.got);
    });
}
