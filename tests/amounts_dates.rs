//! SPEC 3.7 (amounts) and 3.8 (dates), through an account's `starting_balance` and
//! `starting_date` (5.2).

use mybudget_testsuite::{Response, Server, assert_status, json};

/// `POST /plans/{plan}/accounts` with `starting_balance` written as the raw JSON `amount`.
fn post_amount(server: &Server, plan: &str, amount: &str) -> Response {
    let body = format!(
        r#"{{"name": "Checking", "type": "checking", "starting_balance": {amount}, "starting_date": "2026-10-01"}}"#
    );
    server.request_raw("POST", &format!("/plans/{plan}/accounts"), &body)
}

/// `POST /plans/{plan}/accounts` with a non-zero starting balance dated `date`.
fn post_date(server: &Server, plan: &str, date: &str) -> Response {
    let body = json!({"name": "Checking", "type": "checking", "starting_balance": 1000, "starting_date": date});
    server.post(&format!("/plans/{plan}/accounts"), &body)
}

fn account_count(server: &Server, plan: &str) -> usize {
    server.accounts(plan).len()
}

// ---- 3.7 ----

#[test]
fn spec_3_7_accepts_integer_amounts_in_range() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for (raw, value) in [
        ("1", 1_i64),
        ("-1", -1),
        ("12340", 12340),
        ("-220", -220),
        ("2500000", 2500000),
        ("999999999999", 999_999_999_999),
        ("1000000000000", 1_000_000_000_000),
        ("-1000000000000", -1_000_000_000_000),
    ] {
        let res = post_amount(&server, &plan, raw);
        assert_status(&res, 201, &format!("starting_balance {raw}"));
        let account = res.json();
        assert_eq!(account["balance"], json!(value), "starting_balance {raw}: {account}");
        assert_eq!(account["cleared_balance"], json!(value), "starting_balance {raw}: {account}");
    }
    assert_eq!(account_count(&server, &plan), 8);
}

#[test]
fn spec_3_7_rejects_amounts_out_of_range() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for raw in [
        "1000000000001",
        "-1000000000001",
        "2000000000000",
        "9007199254740993",
        "9223372036854775807",
        "9223372036854775808",
        "-9223372036854775808",
        "-9223372036854775809",
        "18446744073709551615",
        "18446744073709551616",
        "100000000000000000000000000000",
    ] {
        assert_status(&post_amount(&server, &plan, raw), 400, &format!("starting_balance {raw}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}

#[test]
fn spec_3_7_rejects_amounts_with_fraction_or_exponent() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for raw in [
        "1.0",
        "1000.5",
        "0.5",
        "-0.5",
        "0.0",
        "-0.0",
        "2500000.000",
        "-350000.0",
        "1e3",
        "1E3",
        "1e+3",
        "1E-0",
        "10e-1",
        "1.5e3",
        "0e0",
        "1e12",
        "12340e0",
    ] {
        assert_status(&post_amount(&server, &plan, raw), 400, &format!("starting_balance {raw}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}

#[test]
fn spec_3_7_rejects_amounts_that_are_not_numbers() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for raw in [r#""1000""#, r#""12.34""#, "true", "[1000]", r#"{"milliunits": 1000}"#] {
        assert_status(&post_amount(&server, &plan, raw), 400, &format!("starting_balance {raw}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}

#[test]
fn spec_3_7_accepts_negative_zero_amount() {
    // `-0` is a JSON integer written without a fraction or exponent, and it's zero.
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = format!("/plans/{plan}/accounts");
    for body in [
        r#"{"name": "A", "type": "checking", "starting_balance": -0}"#,
        r#"{"name": "B", "type": "savings", "starting_balance": -0, "starting_date": "2026-10-01"}"#,
    ] {
        let res = server.request_raw("POST", &url, body);
        assert_status(&res, 201, &format!("POST {body}"));
        let account = res.json();
        assert_eq!(account["balance"], json!(0), "POST {body}: {account}");
    }
    assert_eq!(account_count(&server, &plan), 2);
}

// ---- 3.8 ----

#[test]
fn spec_3_8_accepts_real_dates_in_range() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let dates = [
        "1900-01-01",
        "2999-12-31",
        "2026-10-01",
        "2000-02-29",
        "2024-02-29",
        "2400-02-29",
        "1904-02-29",
        "2026-01-31",
        "2026-03-31",
        "2026-04-30",
        "2026-12-31",
        "1999-12-31",
        "2099-02-28",
        "2027-06-15",
        "2900-07-04",
    ];
    for date in dates {
        let res = post_date(&server, &plan, date);
        assert_status(&res, 201, &format!("starting_date {date}"));
        assert_eq!(res.json()["balance"], 1000);
    }
    assert_eq!(account_count(&server, &plan), dates.len());
}

#[test]
fn spec_3_8_rejects_dates_out_of_range() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for date in ["1899-12-31", "1899-01-01", "3000-01-01", "0000-01-01", "0001-01-01", "1000-06-15", "9999-12-31"] {
        assert_status(&post_date(&server, &plan, date), 400, &format!("starting_date {date}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}

#[test]
fn spec_3_8_rejects_dates_that_do_not_exist() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for date in [
        "2026-02-29",
        "1900-02-29",
        "2100-02-29",
        "2026-02-30",
        "2024-02-30",
        "2026-04-31",
        "2026-06-31",
        "2026-09-31",
        "2026-11-31",
        "2026-13-01",
        "2026-00-10",
        "2026-10-00",
        "2026-10-32",
        "2026-01-32",
        "2026-99-99",
    ] {
        assert_status(&post_date(&server, &plan, date), 400, &format!("starting_date {date}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}

#[test]
fn spec_3_8_rejects_dates_not_written_yyyy_mm_dd() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for date in [
        "2026-10-1",
        "2026-1-01",
        "2026-1-1",
        "26-10-01",
        "2026/10/01",
        "2026.10.01",
        "20261001",
        "2026-10",
        "2026",
        "",
        " ",
        " 2026-10-01",
        "2026-10-01 ",
        "\t2026-10-01",
        "2026-10-01\n",
        "2026-10-01T00:00:00",
        "2026-10-01T00:00:00Z",
        "2026-10-01Z",
        "2026-10-01+00:00",
        "+2026-10-01",
        "-2026-10-01",
        "02026-10-01",
        "2026-010-01",
        "2026-10-001",
        "2026--10-01",
        "2026-10-01-",
        "10/01/2026",
        "01-10-2026",
        "2026-W40-4",
        "2026-274",
        "2026-10-0a",
        "2026-1a-01",
        "2026-+1-01",
        "2026-10-+1",
        "2026- 1-01",
        "2026-10- 1",
        "２０２６-１０-０１",
        "2026-１0-01",
        "2026‐10‐01",
        "2026−10−01",
        "abcd-ef-gh",
    ] {
        assert_status(&post_date(&server, &plan, date), 400, &format!("starting_date {date:?}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}

#[test]
fn spec_3_8_rejects_dates_that_are_not_strings() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = format!("/plans/{plan}/accounts");
    for date in [json!(20261001), json!(2026), json!(true), json!(["2026-10-01"]), json!({"date": "2026-10-01"})] {
        let body = json!({"name": "Checking", "type": "checking", "starting_balance": 1000, "starting_date": date});
        assert_status(&server.post(&url, &body), 400, &format!("starting_date {date}"));
    }
    assert_eq!(account_count(&server, &plan), 0);
}
