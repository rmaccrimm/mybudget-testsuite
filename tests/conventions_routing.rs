//! Sections 3.4 and 3.6 at the HTTP level, across every kind of path: what a `HEAD`
//! response carries besides its status, and which endpoints read the transaction list's
//! query parameters.

use mybudget_testsuite::{Budget, Server, assert_matches, assert_status, entry_path, id_of, json, month_path, spec_fields};

/// The plan's paths that take `GET`, with the status `GET` gives each: one of every kind
/// of object, a month and a month entry, things that don't exist, and requests that are
/// wrong in themselves.
fn get_paths(server: &Server, b: &Budget) -> Vec<(String, u16)> {
    let txn = id_of(&server.create_transaction(&b.plan, &b.spend(-100)));
    let payee = id_of(&server.create_payee(&b.plan, "Corner Shop"));
    vec![
        ("/plans".to_owned(), 200),
        (b.path(""), 200),
        (b.path("/accounts"), 200),
        (b.path(&format!("/accounts/{}", b.checking.id)), 200),
        (b.path("/payees"), 200),
        (b.path(&format!("/payees/{payee}")), 200),
        (b.path("/categories"), 200),
        (b.path(&format!("/categories/{}", b.groceries)), 200),
        (b.path(&format!("/category_groups/{}", b.group)), 200),
        (b.path("/transactions"), 200),
        (b.path("/transactions?approved=true&since_date=2026-10-01"), 200),
        (b.txn_path(&txn), 200),
        (month_path(&b.plan, "2026-10"), 200),
        (entry_path(&b.plan, "2026-10", &b.groceries), 200),
        (entry_path(&b.plan, "2026-10", &b.visa_payment), 200),
        ("/no-such-path".to_owned(), 404),
        ("/plans/no-such-plan".to_owned(), 404),
        (b.path("/accounts/no-such-account"), 404),
        (b.txn_path("no-such-transaction"), 404),
        (entry_path(&b.plan, "2026-10", "ready_to_assign"), 404),
        (month_path(&b.plan, "2026-13"), 400),
        (b.path("/transactions?approved=maybe"), 400),
        (b.path("/transactions?until_date=2026-02-30"), 400),
    ]
}

#[test]
fn spec_3_4_head_gets_the_get_content_type() {
    let server = Server::start();
    let b = Budget::new(&server);
    for (path, status) in get_paths(&server, &b) {
        let get = server.get(&path);
        assert_status(&get, status, &format!("GET {path}"));
        assert!(get.is_json_content_type(), "GET {path}: Content-Type {:?}", get.content_type);
        let head = server.head(&path);
        assert_status(&head, status, &format!("HEAD {path}"));
        assert!(head.body.is_empty(), "HEAD {path} returned a body: {:?}", head.text());
        assert_eq!(head.content_type, get.content_type, "HEAD {path} has another Content-Type than GET");
    }
}

/// Path segments made of characters a URL path may hold without percent-encoding, none of
/// which a generated id ever has (3.5).
const ODD_SEGMENTS: [&str; 16] =
    ["a.b", "a~b", "a:b", "a@b", "a!b", "a$b", "a'b", "a(b)", "a*b", "a+b", "a,b", "a;b", "a=b", ":plan", "*", "a&b"];

#[test]
fn spec_3_4_odd_ids_in_paths_name_nothing() {
    let server = Server::start();
    let b = Budget::new(&server);
    for odd in ODD_SEGMENTS {
        for path in [
            format!("/plans/{odd}"),
            format!("/plans/{odd}/accounts"),
            b.path(&format!("/accounts/{odd}")),
            b.path(&format!("/payees/{odd}")),
            b.path(&format!("/categories/{odd}")),
            b.path(&format!("/category_groups/{odd}")),
            b.txn_path(odd),
            entry_path(&b.plan, "2026-10", odd),
            entry_path(odd, "2026-10", &b.groceries),
        ] {
            let res = server.get(&path);
            mybudget_testsuite::assert_error(&res, 404, &format!("GET {path}"));
            let res = server.patch(&path, &json!({}));
            // Every one of these paths takes PATCH except the accounts list, which doesn't (405).
            let want = if path.ends_with("/accounts") { 405 } else { 404 };
            mybudget_testsuite::assert_error(&res, want, &format!("PATCH {path}"));
            let res = server.request_raw("PUT", &path, "{}");
            mybudget_testsuite::assert_error(&res, 405, &format!("PUT {path}"));
        }
    }
    // Nothing was created or changed by any of it.
    assert_eq!(server.list("/plans").len(), 1);
    assert_eq!(server.transactions(&b.plan).len(), 0);
}

#[test]
fn spec_3_6_single_transaction_requests_ignore_list_query_parameters() {
    let server = Server::start();
    let b = Budget::new(&server);
    // Each of the list's parameters with a value it would refuse (8.5), and ids that
    // match nothing.
    let query = "?approved=maybe&since_date=bad&until_date=2026-02-30&account_id=nope&payee_id=nope&category_id=nope";

    let res = server.post(&b.path(&format!("/transactions{query}")), &b.spend(-100));
    assert_status(&res, 201, "POST with the list's parameters");
    let id = id_of(&res.json());
    let url = b.txn_path(&id);

    let plain = server.get_json(&url);
    let res = server.get(&format!("{url}{query}"));
    assert_status(&res, 200, "GET one with the list's parameters");
    assert_eq!(spec_fields(&res.json()), spec_fields(&plain));
    assert_status(&server.head(&format!("{url}{query}")), 200, "HEAD one with the list's parameters");

    let res = server.patch(&format!("{url}{query}"), &json!({"memo": "kept"}));
    assert_status(&res, 200, "PATCH with the list's parameters");
    assert_matches(&res.json(), &json!({"id": id, "memo": "kept", "amount": -100}), "PATCH with the list's parameters");

    let res = server.get(&format!("{}{query}", entry_path(&b.plan, "2026-10", &b.groceries)));
    assert_status(&res, 200, "GET month entry with the list's parameters");
    assert_matches(&res.json(), &json!({"activity": -100}), "month entry");

    assert_status(&server.delete(&format!("{url}{query}")), 204, "DELETE with the list's parameters");
    assert_status(&server.get(&url), 404, "GET after DELETE");
}
