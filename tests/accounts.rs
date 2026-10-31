//! SPEC section 5: accounts.

use mybudget_testsuite::{
    START_DATE, Server, Value, assert_error, assert_matches, assert_status, id_of, ids_of, json, names_of, spec_fields,
    str_field,
};

/// Every account type, and whether it's on budget (5.1).
const TYPES: [(&str, bool); 7] = [
    ("checking", true),
    ("savings", true),
    ("cash", true),
    ("creditCard", true),
    ("lineOfCredit", true),
    ("otherAsset", false),
    ("otherLiability", false),
];

const CREDIT_TYPES: [&str; 2] = ["creditCard", "lineOfCredit"];

fn accounts_url(plan: &str) -> String {
    format!("/plans/{plan}/accounts")
}

fn account_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/accounts/{id}")
}

fn payee_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/payees/{id}")
}

fn category_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/categories/{id}")
}

fn group_url(plan: &str, id: &str) -> String {
    format!("/plans/{plan}/category_groups/{id}")
}

/// The plan's payees named exactly `Starting Balance`.
fn starting_balance_payees(server: &Server, plan: &str) -> Vec<Value> {
    server.payees(plan).into_iter().filter(|p| p["name"] == "Starting Balance").collect()
}

/// The internal `Credit Card Payments` group, which 7.1 lists first.
fn internal_group(server: &Server, plan: &str) -> Value {
    let groups = server.groups(plan);
    let first = groups.first().unwrap_or_else(|| panic!("GET categories listed no groups"));
    assert_eq!(first["internal"], true, "first group isn't internal: {first}");
    first.clone()
}

// ---- 5.1 ----

#[test]
fn spec_5_1_on_budget_follows_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, on_budget) in TYPES {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, 0);
        assert_eq!(account["type"], ty, "POST {ty}: {account}");
        assert_eq!(account["on_budget"], on_budget, "POST {ty}: {account}");
        let got = server.get_json(&account_url(&plan, &id_of(&account)));
        assert_eq!(got["type"], ty, "GET {ty}: {got}");
        assert_eq!(got["on_budget"], on_budget, "GET {ty}: {got}");
    }
    let listed = server.accounts(&plan);
    assert_eq!(listed.len(), TYPES.len());
    for ((ty, on_budget), account) in TYPES.iter().zip(&listed) {
        assert_eq!(account["type"], *ty, "list: {account}");
        assert_eq!(account["on_budget"], *on_budget, "list: {account}");
    }
}

#[test]
fn spec_5_1_on_budget_holds_with_a_starting_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, on_budget) in TYPES {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, -5000);
        assert_eq!(account["on_budget"], on_budget, "POST {ty} with a starting balance: {account}");
    }
}

#[test]
fn spec_5_1_rejects_unknown_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for ty in [
        json!("Checking"),
        json!("CHECKING"),
        json!("credit_card"),
        json!("creditcard"),
        json!("CreditCard"),
        json!("credit"),
        json!("line_of_credit"),
        json!("LineOfCredit"),
        json!("other_asset"),
        json!("otherasset"),
        json!("OtherLiability"),
        json!("tracking"),
        json!("loan"),
        json!("mortgage"),
        json!("investment"),
        json!(""),
        json!(" checking"),
        json!("checking "),
        json!("checking\u{0}"),
        json!(1),
        json!(true),
        json!(["checking"]),
        json!({"type": "checking"}),
    ] {
        let res = server.post(&accounts_url(&plan), &json!({"name": "Checking", "type": ty}));
        assert_status(&res, 400, &format!("type {ty}"));
    }
    assert!(server.accounts(&plan).is_empty(), "rejected types created accounts");
}

// ---- 5.2 ----

#[test]
fn spec_5_2_creates_account_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.post(
        &accounts_url(&plan),
        &json!({"name": "Checking", "type": "checking", "starting_balance": 2500000, "starting_date": "2026-10-01"}),
    );
    assert_status(&res, 201, "POST account");
    let account = res.json();
    let id = id_of(&account);
    let payee_id = str_field(&account, "transfer_payee_id");
    assert_ne!(payee_id, id, "the account and its transfer payee share an id");
    assert_matches(
        &account,
        &json!({"id": id, "name": "Checking", "type": "checking", "on_budget": true, "closed": false,
                "note": null, "balance": 2500000, "cleared_balance": 2500000, "uncleared_balance": 0,
                "transfer_payee_id": payee_id, "payment_category_id": null}),
        "POST response",
    );
}

#[test]
fn spec_5_2_requires_name_and_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for body in [
        json!({"type": "checking"}),
        json!({"name": "Checking"}),
        json!({}),
        json!({"note": "x", "starting_balance": 0}),
        json!({"Name": "Checking", "Type": "checking"}),
    ] {
        assert_status(&server.post(&accounts_url(&plan), &body), 400, &format!("POST {body}"));
    }
    assert!(server.accounts(&plan).is_empty(), "rejected POSTs created accounts");
}

#[test]
fn spec_5_2_rejects_invalid_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for name in [
        json!(""),
        json!(" "),
        json!("\t\n"),
        json!("\u{3000}"),
        json!("a".repeat(101)),
        json!(5),
        json!(true),
        json!(["Checking"]),
        json!({"name": "Checking"}),
    ] {
        let res = server.post(&accounts_url(&plan), &json!({"name": name, "type": "checking"}));
        assert_status(&res, 400, &format!("name {name}"));
    }
    assert!(server.accounts(&plan).is_empty(), "rejected names created accounts");
}

#[test]
fn spec_5_2_note_is_optional_and_nullable() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let absent = server.create_account(&plan, &json!({"name": "A", "type": "checking"}));
    assert_matches(&absent, &json!({"note": null}), "no note");
    let null = server.create_account(&plan, &json!({"name": "B", "type": "savings", "note": null}));
    assert_matches(&null, &json!({"note": null}), "note null");
    let given = server.create_account(&plan, &json!({"name": "C", "type": "cash", "note": "Joint account"}));
    assert_eq!(given["note"], "Joint account");
    let got = server.get_json(&account_url(&plan, &id_of(&given)));
    assert_eq!(got["note"], "Joint account");
    let got = server.get_json(&account_url(&plan, &id_of(&null)));
    assert_matches(&got, &json!({"note": null}), "GET after note null");
}

#[test]
fn spec_5_2_rejects_note_of_wrong_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for note in [json!(5), json!(true), json!(["x"]), json!({"text": "x"})] {
        let res = server.post(&accounts_url(&plan), &json!({"name": "Checking", "type": "checking", "note": note}));
        assert_status(&res, 400, &format!("note {note}"));
    }
    assert!(server.accounts(&plan).is_empty());
}

#[test]
fn spec_5_2_starting_balance_defaults_to_zero() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, _) in TYPES {
        let account = server.create_account(&plan, &json!({"name": "Account", "type": ty}));
        assert_matches(
            &account,
            &json!({"balance": 0, "cleared_balance": 0, "uncleared_balance": 0}),
            &format!("POST {ty} without starting_balance"),
        );
    }
}

#[test]
fn spec_5_2_zero_starting_balance_needs_no_date() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.post(&accounts_url(&plan), &json!({"name": "Checking", "type": "checking", "starting_balance": 0}));
    assert_status(&res, 201, "starting_balance 0 without starting_date");
    assert_matches(&res.json(), &json!({"balance": 0, "cleared_balance": 0, "uncleared_balance": 0}), "POST");
}

#[test]
fn spec_5_2_starting_date_is_unused_when_balance_is_zero() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for body in [
        json!({"name": "A", "type": "checking", "starting_balance": 0, "starting_date": "2026-10-01"}),
        json!({"name": "B", "type": "creditCard", "starting_date": "2026-10-01"}),
        json!({"name": "C", "type": "otherAsset", "starting_balance": 0, "starting_date": "1900-01-01"}),
        json!({"name": "D", "type": "lineOfCredit", "starting_balance": 0, "starting_date": "2999-12-31"}),
        json!({"name": "E", "type": "otherLiability", "starting_date": "2024-02-29"}),
    ] {
        let res = server.post(&accounts_url(&plan), &body);
        assert_status(&res, 201, &format!("POST {body}"));
        assert_matches(&res.json(), &json!({"balance": 0, "cleared_balance": 0, "uncleared_balance": 0}), "POST");
    }
    assert!(starting_balance_payees(&server, &plan).is_empty(), "a zero starting balance made a transaction");
}

#[test]
fn spec_5_2_unused_starting_date_must_not_be_null() {
    // `starting_date` isn't nullable, so `null` returns 400 even when the balance is 0 and
    // leaves the date unused. `-0` is 0 (3.7).
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = accounts_url(&plan);
    for ty in ["checking", "creditCard", "otherAsset"] {
        for balance in ["", r#", "starting_balance": 0"#, r#", "starting_balance": -0"#] {
            let body = format!(r#"{{"name": "Account", "type": "{ty}"{balance}, "starting_date": null}}"#);
            assert_status(&server.request_raw("POST", &url, &body), 400, &format!("POST {body}"));
        }
    }
    assert!(server.accounts(&plan).is_empty(), "rejected POSTs created accounts");
}

#[test]
fn spec_5_2_unused_starting_date_must_still_be_a_date() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let dates = [
        json!("2026-02-30"),
        json!("2026-02-29"),
        json!("2026-04-31"),
        json!("2026-13-01"),
        json!("2026-10-00"),
        json!("1899-12-31"),
        json!("3000-01-01"),
        json!("2026-10-1"),
        json!("2026/10/01"),
        json!("2026-10"),
        json!("2026-10-01T00:00:00Z"),
        json!(" 2026-10-01"),
        json!(""),
        json!(20261001),
        json!(0),
        json!(true),
        json!(["2026-10-01"]),
        json!({"date": "2026-10-01"}),
    ];
    for ty in ["checking", "creditCard", "otherLiability"] {
        for date in &dates {
            for body in [
                json!({"name": "Account", "type": ty, "starting_balance": 0, "starting_date": date}),
                json!({"name": "Account", "type": ty, "starting_date": date}),
            ] {
                assert_status(&server.post(&accounts_url(&plan), &body), 400, &format!("POST {body}"));
            }
        }
    }
    assert!(server.accounts(&plan).is_empty(), "rejected POSTs created accounts");
}

#[test]
fn spec_5_2_nonzero_starting_balance_requires_date() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for balance in [1000, -1000, 1, -1, 1_000_000_000_000_i64] {
        let body = json!({"name": "Checking", "type": "checking", "starting_balance": balance});
        assert_status(&server.post(&accounts_url(&plan), &body), 400, &format!("POST {body}"));
    }
    let body = json!({"name": "Checking", "type": "checking", "starting_balance": 1000, "starting_date": null});
    assert_status(&server.post(&accounts_url(&plan), &body), 400, &format!("POST {body}"));
    assert!(server.accounts(&plan).is_empty(), "rejected POSTs created accounts");
}

#[test]
fn spec_5_2_rejects_starting_balance_of_wrong_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for balance in [json!("1000"), json!("0"), json!(true), json!(false), json!([1000]), json!({"amount": 1000})] {
        let body = json!({"name": "Checking", "type": "checking", "starting_balance": balance, "starting_date": START_DATE});
        assert_status(&server.post(&accounts_url(&plan), &body), 400, &format!("POST {body}"));
    }
    assert!(server.accounts(&plan).is_empty());
}

#[test]
fn spec_5_2_rejects_starting_date_of_wrong_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for date in [json!(20261001), json!(true), json!(["2026-10-01"]), json!({"date": "2026-10-01"})] {
        let body = json!({"name": "Checking", "type": "checking", "starting_balance": 1000, "starting_date": date});
        assert_status(&server.post(&accounts_url(&plan), &body), 400, &format!("POST {body}"));
    }
    assert!(server.accounts(&plan).is_empty());
}

// ---- 5.3 ----

#[test]
fn spec_5_3_starting_balance_creates_starting_balance_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 2500000);
    let found = starting_balance_payees(&server, &plan);
    assert_eq!(found.len(), 1, "payees: {:?}", server.payees(&plan));
    assert_matches(&found[0], &json!({"name": "Starting Balance", "transfer_account_id": null}), "Starting Balance payee");
    let got = server.get_json(&payee_url(&plan, &id_of(&found[0])));
    assert_matches(&got, &json!({"name": "Starting Balance", "transfer_account_id": null}), "GET payee");
}

#[test]
fn spec_5_3_zero_starting_balance_creates_no_transaction() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let accounts = [
        server.create_account(&plan, &json!({"name": "A", "type": "checking"})),
        server.create_account(&plan, &json!({"name": "B", "type": "creditCard", "starting_balance": 0})),
        server.create_account(
            &plan,
            &json!({"name": "C", "type": "otherAsset", "starting_balance": 0, "starting_date": START_DATE}),
        ),
    ];
    assert!(starting_balance_payees(&server, &plan).is_empty(), "payees: {:?}", server.payees(&plan));
    for account in &accounts {
        let res = server.delete(&account_url(&plan, &id_of(account)));
        assert_status(&res, 204, &format!("DELETE {} (it should have no transactions)", account["name"]));
    }
}

#[test]
fn spec_5_3_reuses_starting_balance_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 2500000);
    let first = starting_balance_payees(&server, &plan);
    assert_eq!(first.len(), 1);
    for (name, ty, balance) in
        [("House", "otherAsset", 100000), ("Visa", "creditCard", -350000), ("Loan", "otherLiability", -5)]
    {
        server.add_account(&plan, name, ty, balance);
        let found = starting_balance_payees(&server, &plan);
        assert_eq!(found.len(), 1, "after {name}, payees: {:?}", server.payees(&plan));
        assert_eq!(found[0]["id"], first[0]["id"], "after {name}, the Starting Balance payee changed");
    }
}

#[test]
fn spec_5_3_reuses_existing_payee_named_starting_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let existing = server.create_payee(&plan, "Starting Balance");
    let account = server.add_account(&plan, "Checking", "checking", 2500000);
    let found = starting_balance_payees(&server, &plan);
    assert_eq!(found.len(), 1, "payees: {:?}", server.payees(&plan));
    assert_eq!(found[0]["id"], existing["id"]);
    let ids = ids_of(&server.payees(&plan));
    assert_eq!(ids, vec![id_of(&existing), str_field(&account, "transfer_payee_id")]);
}

#[test]
fn spec_5_3_finds_starting_balance_payee_by_exact_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let lookalikes = [
        "starting balance",
        "STARTING BALANCE",
        "Starting Balance ",
        " Starting Balance",
        "Starting  Balance",
        "Starting\u{a0}Balance",
        "StartingBalance",
    ];
    let created: Vec<String> = lookalikes.iter().map(|n| id_of(&server.create_payee(&plan, n))).collect();
    server.add_account(&plan, "Checking", "checking", 1000);
    let found = starting_balance_payees(&server, &plan);
    assert_eq!(found.len(), 1, "payees: {:?}", server.payees(&plan));
    assert!(!created.contains(&id_of(&found[0])), "a payee with another name was used: {:?}", found[0]);
    let names = names_of(&server.payees(&plan));
    for name in lookalikes {
        assert!(names.iter().any(|n| n == name), "payee {name:?} disappeared: {names:?}");
    }
}

#[test]
fn spec_5_3_finds_starting_balance_payee_by_its_current_name() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 1000);
    let original = starting_balance_payees(&server, &plan);
    assert_eq!(original.len(), 1);
    let original_id = id_of(&original[0]);
    server.patch_ok(&payee_url(&plan, &original_id), &json!({"name": "Opening Balance"}));
    server.add_account(&plan, "Savings", "savings", 2000);
    let found = starting_balance_payees(&server, &plan);
    assert_eq!(found.len(), 1, "payees: {:?}", server.payees(&plan));
    assert_ne!(id_of(&found[0]), original_id, "the renamed payee was used as `Starting Balance`");
    let renamed = server.get_json(&payee_url(&plan, &original_id));
    assert_eq!(renamed["name"], "Opening Balance");
}

#[test]
fn spec_5_3_starting_balance_is_cleared() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, balance) in [
        ("checking", 2500000),
        ("creditCard", -350000),
        ("lineOfCredit", -1),
        ("otherAsset", 1000),
        ("otherLiability", -250000000),
        ("savings", -1_000_000_000_000_i64),
        ("cash", 1_000_000_000_000_i64),
    ] {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, balance);
        let want = json!({"balance": balance, "cleared_balance": balance, "uncleared_balance": 0});
        assert_matches(&account, &want, &format!("POST {ty} {balance}"));
        let got = server.get_json(&account_url(&plan, &id_of(&account)));
        assert_matches(&got, &want, &format!("GET {ty} {balance}"));
    }
}

#[test]
fn spec_5_3_tracking_account_starting_balance_is_a_transaction() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (name, ty, balance) in [("House", "otherAsset", 300000000), ("Mortgage", "otherLiability", -200000000)] {
        let account = server.add_account(&plan, name, ty, balance);
        let res = server.delete(&account_url(&plan, &id_of(&account)));
        assert_error(&res, 409, &format!("DELETE {ty} account with a starting balance"));
    }
    assert_eq!(starting_balance_payees(&server, &plan).len(), 1, "payees: {:?}", server.payees(&plan));
}

// ---- 5.4 ----

#[test]
fn spec_5_4_balance_sums_only_the_accounts_own_transactions() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let cases = [("A", 12340_i64), ("B", -220), ("C", 0), ("D", 2500000), ("E", -350000)];
    let created: Vec<Value> = cases.iter().map(|(name, b)| server.add_account(&plan, name, "checking", *b)).collect();
    let listed = server.accounts(&plan);
    for ((name, balance), account) in cases.iter().zip(&created) {
        let want = json!({"balance": balance, "cleared_balance": balance, "uncleared_balance": 0});
        let got = server.get_json(&account_url(&plan, &id_of(account)));
        assert_matches(&got, &want, &format!("GET {name}"));
        let in_list = listed.iter().find(|a| a["id"] == account["id"]).expect("account missing from list");
        assert_matches(in_list, &want, &format!("{name} in list"));
    }
}

#[test]
fn spec_5_4_balances_are_integers() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 2500000);
    let got = server.get_json(&account_url(&plan, &id_of(&account)));
    for field in ["balance", "cleared_balance", "uncleared_balance"] {
        assert!(got[field].is_i64() || got[field].is_u64(), "`{field}` isn't a JSON integer: {got}");
    }
}

// ---- 5.5 ----

#[test]
fn spec_5_5_creates_transfer_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 2500000);
    let id = id_of(&account);
    let payee_id = str_field(&account, "transfer_payee_id");
    let payee = server.get_json(&payee_url(&plan, &payee_id));
    assert_matches(
        &payee,
        &json!({"id": payee_id, "name": "Transfer : Checking", "transfer_account_id": id}),
        "GET transfer payee",
    );
    let listed = server.payees(&plan);
    assert!(listed.iter().any(|p| p["id"] == payee_id.as_str()), "transfer payee not listed: {listed:?}");
}

#[test]
fn spec_5_5_every_account_gets_its_own_transfer_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let mut payee_ids = Vec::new();
    for (ty, _) in TYPES {
        let name = format!("Acct {ty}");
        let account = server.add_account(&plan, &name, ty, 0);
        let payee_id = str_field(&account, "transfer_payee_id");
        let payee = server.get_json(&payee_url(&plan, &payee_id));
        assert_eq!(payee["name"], format!("Transfer : {name}"), "{ty}: {payee}");
        assert_eq!(payee["transfer_account_id"], account["id"], "{ty}: {payee}");
        assert!(!payee_ids.contains(&payee_id), "{ty}: transfer payee id {payee_id} reused");
        payee_ids.push(payee_id);
    }
    let listed = server.payees(&plan);
    assert_eq!(ids_of(&listed), payee_ids);
}

#[test]
fn spec_5_5_transfer_payee_name_uses_account_name_exactly() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let long = "a".repeat(100);
    for name in [" Spaced ", long.as_str(), "Café 😀", "Transfer : X", "e\u{301}", "tab\there", "Checking"] {
        let account = server.add_account(&plan, name, "savings", 0);
        assert_eq!(account["name"], name);
        let payee = server.get_json(&payee_url(&plan, &str_field(&account, "transfer_payee_id")));
        assert_eq!(payee["name"], format!("Transfer : {name}"), "account {name:?}");
    }
}

#[test]
fn spec_5_5_creates_credit_account_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.post(
        &accounts_url(&plan),
        &json!({"name": "Visa", "type": "creditCard", "starting_balance": -350000, "starting_date": "2026-10-01"}),
    );
    assert_status(&res, 201, "POST credit account");
    let account = res.json();
    let id = id_of(&account);
    let payee_id = str_field(&account, "transfer_payee_id");
    let category_id = str_field(&account, "payment_category_id");
    assert_ne!(payee_id, category_id);
    assert_matches(
        &account,
        &json!({"id": id, "name": "Visa", "type": "creditCard", "on_budget": true, "closed": false,
                "note": null, "balance": -350000, "cleared_balance": -350000, "uncleared_balance": 0,
                "transfer_payee_id": payee_id, "payment_category_id": category_id}),
        "POST response",
    );
}

#[test]
fn spec_5_5_credit_accounts_get_payment_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group_id = id_of(&internal_group(&server, &plan));
    for (name, ty) in [("Visa", "creditCard"), ("Overdraft line", "lineOfCredit")] {
        let account = server.add_account(&plan, name, ty, -1000);
        let category_id = str_field(&account, "payment_category_id");
        let category = server.get_json(&category_url(&plan, &category_id));
        assert_matches(
            &category,
            &json!({"id": category_id, "group_id": group_id, "name": name, "hidden": false, "note": null,
                    "payment_account_id": id_of(&account)}),
            &format!("payment category of {ty}"),
        );
    }
}

#[test]
fn spec_5_5_other_accounts_have_no_payment_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, _) in TYPES.iter().filter(|(ty, _)| !CREDIT_TYPES.contains(ty)) {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, 1000);
        assert_matches(&account, &json!({"payment_category_id": null}), &format!("POST {ty}"));
        let got = server.get_json(&account_url(&plan, &id_of(&account)));
        assert_matches(&got, &json!({"payment_category_id": null}), &format!("GET {ty}"));
    }
    let groups = server.groups(&plan);
    assert_eq!(groups.len(), 1, "groups: {groups:?}");
    assert_eq!(groups[0]["categories"], json!([]), "non-credit accounts made payment categories: {groups:?}");
}

#[test]
fn spec_5_5_transfer_payee_is_created_before_starting_balance_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.add_account(&plan, "Checking", "checking", 2500000);
    assert_eq!(names_of(&server.payees(&plan)), ["Transfer : Checking", "Starting Balance"]);
    server.add_account(&plan, "Savings", "savings", 1000);
    assert_eq!(names_of(&server.payees(&plan)), ["Transfer : Checking", "Starting Balance", "Transfer : Savings"]);
}

// ---- 5.6 ----

#[test]
fn spec_5_6_lists_accounts_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 2500000);
    let listed = server.get_json(&accounts_url(&plan));
    assert_matches(
        &listed,
        &json!([{"id": id_of(&account), "name": "Checking", "type": "checking", "on_budget": true, "closed": false,
                 "note": null, "balance": 2500000, "cleared_balance": 2500000, "uncleared_balance": 0,
                 "transfer_payee_id": str_field(&account, "transfer_payee_id"), "payment_category_id": null}]),
        "GET accounts",
    );
}

#[test]
fn spec_5_6_lists_accounts_oldest_first() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let specs = [
        ("Zeta", "savings", 0),
        ("Alpha", "creditCard", 0),
        ("Mid", "otherAsset", 5000),
        ("alpha", "cash", 0),
        ("Zeta", "checking", 100),
        ("Beta", "lineOfCredit", -100),
    ];
    let created: Vec<String> =
        specs.iter().map(|(name, ty, b)| id_of(&server.add_account(&plan, name, ty, *b))).collect();
    assert_eq!(ids_of(&server.accounts(&plan)), created, "list is not oldest first");

    server.patch_ok(&account_url(&plan, &created[0]), &json!({"name": "AAA"}));
    server.patch_ok(&account_url(&plan, &created[3]), &json!({"closed": true}));
    assert_eq!(ids_of(&server.accounts(&plan)), created, "rename or close reordered the list");

    assert_status(&server.delete(&account_url(&plan, &created[1])), 204, "DELETE Alpha");
    let newest = id_of(&server.add_account(&plan, "Newest", "checking", 0));
    let mut want = created.clone();
    want.remove(1);
    want.push(newest);
    assert_eq!(ids_of(&server.accounts(&plan)), want);
}

#[test]
fn spec_5_6_gets_one_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let created = [
        server.add_account(&plan, "Checking", "checking", 2500000),
        server.add_account(&plan, "Visa", "creditCard", -350000),
        server.create_account(&plan, &json!({"name": "House", "type": "otherAsset", "note": "valuation"})),
    ];
    for account in &created {
        let got = server.get_json(&account_url(&plan, &id_of(account)));
        assert_matches(&got, &spec_fields(account), &format!("GET {}", account["name"]));
    }
}

#[test]
fn spec_5_6_missing_account_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Visa", "creditCard", 0);
    let group = server.create_group(&plan, "Bills");
    for id in [
        "no-such-account".to_owned(),
        "CHECKING_ID".to_owned(),
        plan.clone(),
        str_field(&account, "transfer_payee_id"),
        str_field(&account, "payment_category_id"),
        id_of(&group),
        "ready_to_assign".to_owned(),
    ] {
        let res = server.get(&account_url(&plan, &id));
        assert_error(&res, 404, &format!("GET accounts/{id}"));
    }
}

#[test]
fn spec_5_6_new_plan_has_no_accounts() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let res = server.get(&accounts_url(&plan));
    assert_status(&res, 200, "GET accounts");
    assert_eq!(res.json(), json!([]));
}

#[test]
fn spec_5_6_accounts_of_missing_plan_return_404() {
    let server = Server::start();
    server.plan_id("Household");
    assert_error(&server.get("/plans/no-such-plan/accounts"), 404, "GET accounts of a missing plan");
    assert_error(&server.get("/plans/no-such-plan/accounts/x"), 404, "GET account of a missing plan");
}

// ---- 5.7 ----

#[test]
fn spec_5_7_renames_account_matching_example() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", -350000);
    let id = id_of(&visa);
    let res = server.patch(&account_url(&plan, &id), &json!({"name": "Visa Gold"}));
    assert_status(&res, 200, "PATCH name");
    let want = json!({"id": id, "name": "Visa Gold", "type": "creditCard", "on_budget": true, "closed": false,
                      "note": null, "balance": -350000, "cleared_balance": -350000, "uncleared_balance": 0,
                      "transfer_payee_id": visa["transfer_payee_id"], "payment_category_id": visa["payment_category_id"]});
    assert_matches(&res.json(), &want, "PATCH response");
    assert_matches(&server.get_json(&account_url(&plan, &id)), &want, "GET after PATCH");
}

#[test]
fn spec_5_7_rename_renames_transfer_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, _) in TYPES {
        let account = server.add_account(&plan, &format!("Old {ty}"), ty, 0);
        let payee_id = str_field(&account, "transfer_payee_id");
        for name in [format!("New {ty}"), format!("  Newer {ty} 😀 ")] {
            server.patch_ok(&account_url(&plan, &id_of(&account)), &json!({"name": name}));
            let payee = server.get_json(&payee_url(&plan, &payee_id));
            let want = json!({"id": payee_id, "name": format!("Transfer : {name}"), "transfer_account_id": account["id"]});
            assert_matches(&payee, &want, &format!("GET transfer payee of {ty}"));
            let listed = server.payees(&plan);
            let in_list = listed.iter().find(|p| p["id"] == payee_id.as_str()).expect("transfer payee not listed");
            assert_matches(in_list, &want, &format!("listed transfer payee of {ty}"));
        }
    }
}

#[test]
fn spec_5_7_rename_renames_payment_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, name) in [("creditCard", "Visa Gold"), ("lineOfCredit", "Bank line")] {
        let account = server.add_account(&plan, "Old name", ty, -1000);
        let category_id = str_field(&account, "payment_category_id");
        server.patch_ok(&account_url(&plan, &id_of(&account)), &json!({"name": name}));
        let category = server.get_json(&category_url(&plan, &category_id));
        assert_eq!(category["name"], name, "GET payment category of {ty}: {category}");
        assert_eq!(category["payment_account_id"], account["id"]);
        let group = internal_group(&server, &plan);
        let listed = group["categories"].as_array().unwrap();
        let in_list = listed.iter().find(|c| c["id"] == category_id.as_str()).expect("payment category not listed");
        assert_eq!(in_list["name"], name, "listed payment category of {ty}: {in_list}");
        let got_group = server.get_json(&group_url(&plan, &id_of(&group)));
        let in_group = got_group["categories"].as_array().unwrap().iter().find(|c| c["id"] == category_id.as_str()).cloned();
        assert_eq!(in_group.expect("payment category not in GET group")["name"], name);
    }
}

#[test]
fn spec_5_7_rename_changes_only_that_accounts_payee_and_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let twin = server.add_account(&plan, "Visa", "creditCard", 0);
    let user_payee = server.create_payee(&plan, "Transfer : Visa");
    let group = server.create_group(&plan, "Bills");
    let user_category = server.create_category(&plan, &id_of(&group), "Visa");

    server.patch_ok(&account_url(&plan, &id_of(&visa)), &json!({"name": "Gold"}));

    let name_of = |url: String| server.get_json(&url)["name"].clone();
    assert_eq!(name_of(payee_url(&plan, &str_field(&visa, "transfer_payee_id"))), "Transfer : Gold");
    assert_eq!(name_of(category_url(&plan, &str_field(&visa, "payment_category_id"))), "Gold");
    assert_eq!(name_of(account_url(&plan, &id_of(&twin))), "Visa", "the other account named Visa was renamed");
    assert_eq!(name_of(payee_url(&plan, &str_field(&twin, "transfer_payee_id"))), "Transfer : Visa");
    assert_eq!(name_of(category_url(&plan, &str_field(&twin, "payment_category_id"))), "Visa");
    assert_eq!(name_of(payee_url(&plan, &id_of(&user_payee))), "Transfer : Visa", "a user payee was renamed");
    assert_eq!(name_of(category_url(&plan, &id_of(&user_category))), "Visa", "a user category was renamed");
}

#[test]
fn spec_5_7_patches_note() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 2500000);
    let url = account_url(&plan, &id_of(&account));
    for note in [json!("Joint account"), json!("Other note"), json!(null), json!("Back again")] {
        let patched = server.patch_ok(&url, &json!({"note": note}));
        let want = json!({"name": "Checking", "note": note, "balance": 2500000, "closed": false});
        assert_matches(&patched, &want, &format!("PATCH note {note}"));
        assert_matches(&server.get_json(&url), &want, &format!("GET after note {note}"));
    }
}

#[test]
fn spec_5_7_patches_closed() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 0);
    let url = account_url(&plan, &id_of(&account));
    for closed in [false, true, false] {
        let patched = server.patch_ok(&url, &json!({"closed": closed}));
        assert_eq!(patched["closed"], closed, "PATCH closed {closed}: {patched}");
        assert_eq!(server.get_json(&url)["closed"], closed, "GET after closed {closed}");
    }
}

#[test]
fn spec_5_7_patches_closed_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 0);
    let url = account_url(&plan, &id_of(&account));
    server.patch_ok(&url, &json!({"closed": true}));
    let patched = server.patch_ok(&url, &json!({"name": "Old Checking", "note": "closed in 2026"}));
    assert_matches(&patched, &json!({"name": "Old Checking", "note": "closed in 2026", "closed": true}), "PATCH");
    let payee = server.get_json(&payee_url(&plan, &str_field(&account, "transfer_payee_id")));
    assert_eq!(payee["name"], "Transfer : Old Checking");
}

#[test]
fn spec_5_7_accepts_unchanged_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, on_budget) in TYPES {
        let account = server.add_account(&plan, "Account", ty, 0);
        let url = account_url(&plan, &id_of(&account));
        let patched = server.patch_ok(&url, &json!({"type": ty}));
        assert_matches(&patched, &json!({"type": ty, "on_budget": on_budget, "name": "Account"}), "PATCH same type");
        let patched = server.patch_ok(&url, &json!({"type": ty, "name": "Renamed"}));
        assert_matches(&patched, &json!({"type": ty, "name": "Renamed"}), "PATCH same type and name");
    }
}

#[test]
fn spec_5_7_rejects_different_type() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, on_budget) in TYPES {
        let account = server.add_account(&plan, "Account", ty, 0);
        let url = account_url(&plan, &id_of(&account));
        let others = TYPES.iter().map(|(t, _)| *t).filter(|t| *t != ty).chain(["bogus", "", "Checking"]);
        for other in others {
            assert_status(&server.patch(&url, &json!({"type": other})), 400, &format!("PATCH {ty} to type {other:?}"));
        }
        let got = server.get_json(&url);
        assert_matches(
            &got,
            &json!({"type": ty, "on_budget": on_budget, "payment_category_id": account["payment_category_id"]}),
            &format!("{ty} after rejected type changes"),
        );
    }
}

#[test]
fn spec_5_7_rejects_invalid_values() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.create_account(&plan, &json!({"name": "Checking", "type": "checking", "note": "n"}));
    let url = account_url(&plan, &id_of(&account));
    for body in [
        json!({"name": ""}),
        json!({"name": "  "}),
        json!({"name": "a".repeat(101)}),
        json!({"name": 5}),
        json!({"name": ["Checking"]}),
        json!({"note": 5}),
        json!({"note": true}),
        json!({"note": ["n"]}),
        json!({"closed": "true"}),
        json!({"closed": 1}),
        json!({"closed": 0}),
        json!({"closed": "false"}),
        json!({"closed": []}),
        json!({"type": 5}),
    ] {
        assert_status(&server.patch(&url, &body), 400, &format!("PATCH {body}"));
    }
    let got = server.get_json(&url);
    assert_matches(&got, &json!({"name": "Checking", "note": "n", "closed": false, "type": "checking"}), "after");
}

#[test]
fn spec_5_7_patch_missing_account_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 0);
    for id in ["no-such-account".to_owned(), str_field(&account, "transfer_payee_id"), plan.clone()] {
        let res = server.patch(&account_url(&plan, &id), &json!({"name": "x"}));
        assert_error(&res, 404, &format!("PATCH accounts/{id}"));
    }
    assert_eq!(server.get_json(&account_url(&plan, &id_of(&account)))["name"], "Checking");
}

// ---- 5.8 ----

#[test]
fn spec_5_8_closes_account_with_zero_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, _) in TYPES {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, 0);
        let url = account_url(&plan, &id_of(&account));
        let res = server.patch(&url, &json!({"closed": true}));
        assert_status(&res, 200, &format!("close {ty}"));
        assert_matches(&res.json(), &json!({"closed": true, "balance": 0}), &format!("close {ty}"));
        assert_eq!(server.get_json(&url)["closed"], true, "GET {ty} after closing");
    }
    for account in server.accounts(&plan) {
        assert_eq!(account["closed"], true, "list after closing: {account}");
    }
}

#[test]
fn spec_5_8_closing_account_with_balance_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, balance) in [("checking", 2500000), ("creditCard", -350000), ("otherAsset", 1), ("otherLiability", -1)] {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, balance);
        let url = account_url(&plan, &id_of(&account));
        assert_error(&server.patch(&url, &json!({"closed": true})), 409, &format!("close {ty} with balance {balance}"));
        assert_matches(&server.get_json(&url), &json!({"closed": false, "balance": balance}), &format!("{ty} after 409"));
    }
}

#[test]
fn spec_5_8_only_closing_needs_a_zero_balance() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Visa", "creditCard", -350000);
    let url = account_url(&plan, &id_of(&account));
    for body in [json!({"closed": false}), json!({"name": "Visa Gold", "note": "n"}), json!({"closed": false, "name": "Visa"})] {
        let res = server.patch(&url, &body);
        assert_status(&res, 200, &format!("PATCH {body} on an open account with a balance"));
        assert_eq!(res.json()["closed"], false);
    }
}

#[test]
fn spec_5_8_reopens_closed_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (ty, _) in TYPES {
        let account = server.add_account(&plan, &format!("My {ty}"), ty, 0);
        let url = account_url(&plan, &id_of(&account));
        server.patch_ok(&url, &json!({"closed": true}));
        server.patch_ok(&url, &json!({"closed": true}));
        let reopened = server.patch_ok(&url, &json!({"closed": false}));
        assert_eq!(reopened["closed"], false, "reopen {ty}: {reopened}");
        assert_eq!(server.get_json(&url)["closed"], false);
        assert_eq!(server.patch_ok(&url, &json!({"closed": true}))["closed"], true, "close {ty} again");
        assert_eq!(server.patch_ok(&url, &json!({"closed": false}))["closed"], false, "reopen {ty} again");
    }
}

// ---- 5.9 ----

#[test]
fn spec_5_9_delete_account_with_starting_balance_returns_409() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (name, ty, balance) in [("Visa", "creditCard", -350000), ("Checking", "checking", 2500000)] {
        let account = server.add_account(&plan, name, ty, balance);
        let url = account_url(&plan, &id_of(&account));
        assert_error(&server.delete(&url), 409, &format!("DELETE {name}"));
        assert_matches(&server.get_json(&url), &json!({"name": name, "balance": balance}), "GET after 409");
        server.get_json(&payee_url(&plan, &str_field(&account, "transfer_payee_id")));
        if let Some(category_id) = account["payment_category_id"].as_str() {
            server.get_json(&category_url(&plan, category_id));
        }
    }
    assert_eq!(server.accounts(&plan).len(), 2);
}

#[test]
fn spec_5_9_deletes_account_without_transactions() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let funded = server.add_account(&plan, "Funded", "savings", 1000);
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let user_payee = server.create_payee(&plan, "Corner Shop");
    let res = server.delete(&account_url(&plan, &id_of(&checking)));
    assert_status(&res, 204, "DELETE account");
    assert!(res.body.is_empty(), "DELETE returned a body: {:?}", res.text());
    assert_status(&server.get(&account_url(&plan, &id_of(&checking))), 404, "GET deleted account");
    assert_eq!(ids_of(&server.accounts(&plan)), vec![id_of(&funded)]);
    let payee_id = str_field(&checking, "transfer_payee_id");
    assert_status(&server.get(&payee_url(&plan, &payee_id)), 404, "GET deleted account's transfer payee");
    let payees = ids_of(&server.payees(&plan));
    assert!(!payees.contains(&payee_id), "deleted account's transfer payee still listed: {payees:?}");
    let want = vec![str_field(&funded, "transfer_payee_id"), id_of(&starting_balance_payees(&server, &plan)[0]), id_of(&user_payee)];
    assert_eq!(payees, want, "DELETE removed other payees");
}

#[test]
fn spec_5_9_deleting_credit_account_deletes_payment_category() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let amex = server.add_account(&plan, "Amex", "creditCard", 0);
    let line = server.add_account(&plan, "Line", "lineOfCredit", 0);
    for deleted in [&visa, &line] {
        assert_status(&server.delete(&account_url(&plan, &id_of(deleted))), 204, "DELETE credit account");
        let category_id = str_field(deleted, "payment_category_id");
        assert_status(&server.get(&category_url(&plan, &category_id)), 404, "GET deleted payment category");
    }
    let group = internal_group(&server, &plan);
    let amex_category = str_field(&amex, "payment_category_id");
    assert_eq!(ids_of(group["categories"].as_array().unwrap()), vec![amex_category.clone()]);
    let got = server.get_json(&group_url(&plan, &id_of(&group)));
    assert_eq!(ids_of(got["categories"].as_array().unwrap()), vec![amex_category.clone()]);
    assert_eq!(server.get_json(&category_url(&plan, &amex_category))["name"], "Amex");
}

#[test]
fn spec_5_9_deletes_closed_account() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Visa", "creditCard", 0);
    let url = account_url(&plan, &id_of(&account));
    server.patch_ok(&url, &json!({"closed": true}));
    assert_status(&server.delete(&url), 204, "DELETE closed account");
    assert_status(&server.get(&url), 404, "GET deleted account");
    assert!(server.accounts(&plan).is_empty());
}

#[test]
fn spec_5_9_delete_missing_account_returns_404() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 0);
    let url = account_url(&plan, &id_of(&account));
    assert_error(&server.delete(&account_url(&plan, "no-such-account")), 404, "DELETE missing account");
    let transfer_payee = str_field(&account, "transfer_payee_id");
    assert_error(&server.delete(&account_url(&plan, &transfer_payee)), 404, "DELETE accounts/<payee id>");
    assert_status(&server.delete(&url), 204, "first DELETE");
    assert_error(&server.delete(&url), 404, "second DELETE");
    assert_error(&server.patch(&url, &json!({"name": "Back"})), 404, "PATCH after DELETE");
    assert!(server.accounts(&plan).is_empty(), "PATCH after DELETE resurrected the account");
}
