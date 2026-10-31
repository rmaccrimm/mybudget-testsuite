//! Sections 2 to 4 as they apply to the things inside a plan (sections 5 to 7).

use std::collections::HashSet;

use mybudget_testsuite::{
    START_DATE, Server, Value, assert_error, assert_fields, assert_matches, assert_status, id_of, ids_of, json,
    names_of, spec_fields, str_field,
};

const ACCOUNT_FIELDS: [&str; 11] = [
    "id",
    "name",
    "type",
    "on_budget",
    "closed",
    "note",
    "balance",
    "cleared_balance",
    "uncleared_balance",
    "transfer_payee_id",
    "payment_category_id",
];
const PAYEE_FIELDS: [&str; 3] = ["id", "name", "transfer_account_id"];
const GROUP_FIELDS: [&str; 5] = ["id", "name", "hidden", "internal", "categories"];
const CATEGORY_FIELDS: [&str; 6] = ["id", "group_id", "name", "hidden", "note", "payment_account_id"];

/// One of everything in a plan, created through the API.
struct Fixture {
    plan: String,
    checking: Value,
    visa: Value,
    payee: Value,
    group: Value,
    category: Value,
    internal: Value,
}

impl Fixture {
    fn new(server: &Server, name: &str) -> Fixture {
        let plan = server.plan_id(name);
        let checking = server.add_account(&plan, "Checking", "checking", 2500000);
        let visa = server.add_account(&plan, "Visa", "creditCard", -350000);
        let payee = server.create_payee(&plan, "Corner Shop");
        let group = server.create_group(&plan, "Bills");
        let category = server.create_category(&plan, &id_of(&group), "Rent");
        let internal = server.groups(&plan)[0].clone();
        Fixture { plan, checking, visa, payee, group, category, internal }
    }

    fn path(&self, rest: &str) -> String {
        format!("/plans/{}{rest}", self.plan)
    }

    fn account(&self) -> String {
        self.path(&format!("/accounts/{}", id_of(&self.checking)))
    }

    fn visa_account(&self) -> String {
        self.path(&format!("/accounts/{}", id_of(&self.visa)))
    }

    fn payee(&self) -> String {
        self.path(&format!("/payees/{}", id_of(&self.payee)))
    }

    fn transfer_payee(&self) -> String {
        self.path(&format!("/payees/{}", str_field(&self.checking, "transfer_payee_id")))
    }

    fn group(&self) -> String {
        self.path(&format!("/category_groups/{}", id_of(&self.group)))
    }

    fn internal_group(&self) -> String {
        self.path(&format!("/category_groups/{}", id_of(&self.internal)))
    }

    fn category(&self) -> String {
        self.path(&format!("/categories/{}", id_of(&self.category)))
    }

    fn payment_category(&self) -> String {
        self.path(&format!("/categories/{}", str_field(&self.visa, "payment_category_id")))
    }

    /// Everything the plan's list endpoints return, to compare before and after.
    fn snapshot(&self, server: &Server) -> Value {
        json!({
            "accounts": server.accounts(&self.plan),
            "payees": server.payees(&self.plan),
            "categories": server.groups(&self.plan),
        })
    }
}

fn is_path_safe(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ---- 2.2 ----

#[test]
fn spec_2_2_undefined_paths_inside_a_plan_return_404() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let account = id_of(&f.checking);
    let category = id_of(&f.category);
    for rest in [
        "/accounts/".to_owned(),
        "/payees/".to_owned(),
        "/categories/".to_owned(),
        "/category_groups/".to_owned(),
        format!("/accounts/{account}/"),
        format!("/accounts//{account}"),
        "//accounts".to_owned(),
        "/Accounts".to_owned(),
        "/account".to_owned(),
        "/category-groups".to_owned(),
        "/categoryGroups".to_owned(),
        "/category_group".to_owned(),
        "/groups".to_owned(),
        format!("/accounts/{account}/payees"),
        format!("/categories/{category}/x"),
        format!("/category_groups/{}/categories", id_of(&f.group)),
        format!("/payees/{}/x", id_of(&f.payee)),
        "/nothing".to_owned(),
    ] {
        let path = f.path(&rest);
        assert_error(&server.get(&path), 404, &format!("GET {path}"));
    }
}

// ---- 3.1 ----

#[test]
fn spec_3_1_plan_content_responses_have_json_content_type() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let p = &f.plan;
    let responses = [
        ("POST account", 201, server.post(&f.path("/accounts"), &json!({"name": "Cash", "type": "cash"}))),
        ("POST payee", 201, server.post(&f.path("/payees"), &json!({"name": "Bakery"}))),
        ("POST group", 201, server.post(&f.path("/category_groups"), &json!({"name": "Fun"}))),
        ("POST category", 201, server.post(&f.path("/categories"), &json!({"group_id": id_of(&f.group), "name": "Water"}))),
        ("GET accounts", 200, server.get(&format!("/plans/{p}/accounts"))),
        ("GET account", 200, server.get(&f.account())),
        ("GET payees", 200, server.get(&format!("/plans/{p}/payees"))),
        ("GET payee", 200, server.get(&f.payee())),
        ("GET categories", 200, server.get(&format!("/plans/{p}/categories"))),
        ("GET category", 200, server.get(&f.category())),
        ("GET group", 200, server.get(&f.group())),
        ("PATCH account", 200, server.patch(&f.account(), &json!({"note": "x"}))),
        ("PATCH payee", 200, server.patch(&f.payee(), &json!({"name": "Shop"}))),
        ("PATCH group", 200, server.patch(&f.group(), &json!({"hidden": true}))),
        ("PATCH category", 200, server.patch(&f.category(), &json!({"hidden": true}))),
        ("DELETE account with a transaction", 409, server.delete(&f.account())),
        ("DELETE transfer payee", 400, server.delete(&f.transfer_payee())),
        ("GET missing account", 404, server.get(&f.path("/accounts/no-such-account"))),
        ("PUT accounts", 405, server.request_raw("PUT", &f.path("/accounts"), "{}")),
    ];
    for (what, status, res) in responses {
        assert_status(&res, status, what);
        assert!(res.is_json_content_type(), "{what}: Content-Type {:?}", res.content_type);
        res.json();
    }
}

#[test]
fn spec_3_1_plan_content_bodies_must_be_json_objects() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    let targets = [
        ("POST", f.path("/accounts")),
        ("POST", f.path("/payees")),
        ("POST", f.path("/category_groups")),
        ("POST", f.path("/categories")),
        ("PATCH", f.account()),
        ("PATCH", f.payee()),
        ("PATCH", f.group()),
        ("PATCH", f.category()),
    ];
    for (method, path) in &targets {
        let res = server.request_bytes(method, path, None);
        assert_error(&res, 400, &format!("{method} {path} with no body"));
        for body in ["", "{", "[]", "null", "\"Checking\"", "1", r#"{"name": "a\ud800"}"#, r#"{"name": "x",}"#] {
            let res = server.request_raw(method, path, body);
            assert_error(&res, 400, &format!("{method} {path} {body:?}"));
        }
    }
    assert_eq!(f.snapshot(&server), before, "rejected bodies changed the plan");
}

// ---- 3.2 ----

#[test]
fn spec_3_2_plan_content_errors_have_error_body() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    assert_error(&server.get(&f.path("/payees/nope")), 404, "missing payee");
    assert_error(&server.get(&f.path("/category_groups/nope")), 404, "missing group");
    assert_error(&server.get(&f.path("/categories/nope")), 404, "missing category");
    assert_error(&server.request_raw("PUT", &f.path("/accounts"), "{}"), 405, "PUT accounts");
    assert_error(&server.post(&f.account(), &json!({})), 405, "POST account");
    assert_error(&server.post(&f.path("/accounts"), &json!({"name": "x", "type": "nope"})), 400, "bad type");
    assert_error(&server.patch(&f.group(), &json!({"hidden": "yes"})), 400, "bad hidden");
    assert_error(&server.patch(&f.payment_category(), &json!({"name": "x"})), 400, "rename payment category");
    assert_error(&server.patch(&f.internal_group(), &json!({"hidden": true})), 400, "hide internal group");
    assert_error(&server.delete(&f.internal_group()), 400, "delete internal group");
    assert_error(&server.post(&f.path("/payees"), &json!({"name": "Corner Shop"})), 409, "duplicate payee");
    assert_error(&server.patch(&f.visa_account(), &json!({"closed": true})), 409, "close with balance");
    assert_error(&server.delete(&f.group()), 409, "delete group with categories");
}

// ---- 3.3 ----

#[test]
fn spec_3_3_plan_content_creates_return_201_with_the_object() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.post(&format!("/plans/{plan}/accounts"), &json!({"name": "Checking", "type": "checking"}));
    assert_status(&account, 201, "POST account");
    assert_eq!(account.json()["name"], "Checking");
    let payee = server.post(&format!("/plans/{plan}/payees"), &json!({"name": "Corner Shop"}));
    assert_status(&payee, 201, "POST payee");
    assert_eq!(payee.json()["name"], "Corner Shop");
    let group = server.post(&format!("/plans/{plan}/category_groups"), &json!({"name": "Bills"}));
    assert_status(&group, 201, "POST group");
    assert_eq!(group.json()["name"], "Bills");
    let body = json!({"group_id": id_of(&group.json()), "name": "Rent"});
    let category = server.post(&format!("/plans/{plan}/categories"), &body);
    assert_status(&category, 201, "POST category");
    assert_eq!(category.json()["name"], "Rent");
}

#[test]
fn spec_3_3_plan_content_deletes_return_204_without_body() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.add_account(&plan, "Checking", "checking", 0);
    let payee = server.create_payee(&plan, "Corner Shop");
    let group = server.create_group(&plan, "Bills");
    for path in [
        format!("/plans/{plan}/accounts/{}", id_of(&account)),
        format!("/plans/{plan}/payees/{}", id_of(&payee)),
        format!("/plans/{plan}/category_groups/{}", id_of(&group)),
    ] {
        let res = server.delete(&path);
        assert_status(&res, 204, &format!("DELETE {path}"));
        assert!(res.body.is_empty(), "DELETE {path} returned a body: {:?}", res.text());
    }
}

#[test]
fn spec_3_3_plan_content_gets_and_patches_return_200() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    for path in [f.path("/accounts"), f.account(), f.path("/payees"), f.payee(), f.path("/categories"), f.category(), f.group()] {
        assert_status(&server.get(&path), 200, &format!("GET {path}"));
    }
    for (path, body) in [
        (f.account(), json!({"name": "Main"})),
        (f.payee(), json!({"name": "Shop"})),
        (f.group(), json!({"name": "Monthly"})),
        (f.category(), json!({"name": "Rent!"})),
    ] {
        let res = server.patch(&path, &body);
        assert_status(&res, 200, &format!("PATCH {path}"));
        assert_eq!(res.json()["name"], body["name"]);
    }
}

// ---- 3.4 ----

#[test]
fn spec_3_4_undefined_methods_on_plan_contents_return_405() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    let cases = [
        ("PUT", f.path("/accounts")),
        ("PATCH", f.path("/accounts")),
        ("DELETE", f.path("/accounts")),
        ("POST", f.account()),
        ("PUT", f.account()),
        ("PUT", f.path("/payees")),
        ("PATCH", f.path("/payees")),
        ("DELETE", f.path("/payees")),
        ("POST", f.payee()),
        ("PUT", f.payee()),
        ("PUT", f.path("/categories")),
        ("PATCH", f.path("/categories")),
        ("DELETE", f.path("/categories")),
        ("POST", f.category()),
        ("PUT", f.category()),
        ("PUT", f.path("/category_groups")),
        ("PATCH", f.path("/category_groups")),
        ("DELETE", f.path("/category_groups")),
        ("POST", f.group()),
        ("PUT", f.group()),
        ("OPTIONS", f.path("/accounts")),
        ("OPTIONS", f.group()),
    ];
    for (method, path) in &cases {
        let res = server.request_raw(method, path, r#"{"name": "Other", "type": "cash", "group_id": "x"}"#);
        assert_error(&res, 405, &format!("{method} {path}"));
    }
    assert_eq!(f.snapshot(&server), before, "405 requests changed the plan");
}

#[test]
fn spec_3_4_category_groups_collection_has_only_post() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    server.create_group(&plan, "Bills");
    let path = format!("/plans/{plan}/category_groups");
    assert_error(&server.get(&path), 405, "GET category_groups");
    assert_status(&server.head(&path), 405, "HEAD category_groups");
}

#[test]
fn spec_3_4_categories_have_no_delete() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    for path in [f.category(), f.payment_category()] {
        assert_error(&server.delete(&path), 405, &format!("DELETE {path}"));
        server.get_json(&path);
    }
    assert_error(&server.delete(&f.path("/categories/ready_to_assign")), 405, "DELETE categories/ready_to_assign");
}

#[test]
fn spec_3_4_method_not_allowed_wins_over_not_found_inside_plans() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    for (method, path) in [
        ("PUT", "/plans/no-such-plan/accounts".to_owned()),
        ("DELETE", "/plans/no-such-plan/payees".to_owned()),
        ("GET", "/plans/no-such-plan/category_groups".to_owned()),
        ("PATCH", "/plans/no-such-plan/categories".to_owned()),
        ("POST", "/plans/no-such-plan/accounts/x".to_owned()),
        ("POST", f.path("/accounts/no-such-account")),
        ("PUT", f.path("/payees/no-such-payee")),
        ("DELETE", f.path("/categories/no-such-category")),
        ("POST", f.path("/category_groups/no-such-group")),
    ] {
        assert_error(&server.request_raw(method, &path, "not json"), 405, &format!("{method} {path}"));
    }
}

#[test]
fn spec_3_4_not_found_wins_over_bad_request_inside_plans() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    for (method, path, body) in [
        ("POST", "/plans/no-such-plan/accounts".to_owned(), "not json"),
        ("POST", "/plans/no-such-plan/accounts".to_owned(), r#"{"name": "", "type": "nope"}"#),
        ("POST", "/plans/no-such-plan/payees".to_owned(), "{}"),
        ("POST", "/plans/no-such-plan/category_groups".to_owned(), "[]"),
        ("POST", "/plans/no-such-plan/categories".to_owned(), r#"{"group_id": "nope", "name": "x"}"#),
        ("PATCH", f.path("/accounts/no-such-account"), r#"{"name": ""}"#),
        ("PATCH", f.path("/accounts/no-such-account"), r#"{"type": "nope"}"#),
        ("PATCH", f.path("/payees/no-such-payee"), "not json"),
        ("PATCH", f.path("/category_groups/no-such-group"), r#"{"hidden": "yes"}"#),
        ("PATCH", f.path("/categories/no-such-category"), r#"{"group_id": "nope"}"#),
        ("PATCH", f.path("/categories/ready_to_assign"), r#"{"name": 5}"#),
    ] {
        assert_error(&server.request_raw(method, &path, body), 404, &format!("{method} {path} {body}"));
    }
}

#[test]
fn spec_3_4_not_found_wins_over_conflict_inside_plans() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let res = server.post("/plans/no-such-plan/payees", &json!({"name": "Corner Shop"}));
    assert_error(&res, 404, "POST taken payee name to a missing plan");
    let res = server.patch(&f.path("/payees/no-such-payee"), &json!({"name": "Corner Shop"}));
    assert_error(&res, 404, "PATCH missing payee to a taken name");
}

#[test]
fn spec_3_4_bad_request_wins_over_conflict_inside_plans() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    for body in [
        json!({"closed": true, "name": ""}),
        json!({"closed": true, "type": "savings"}),
        json!({"closed": true, "note": 5}),
        json!({"closed": true, "name": null}),
    ] {
        assert_error(&server.patch(&f.visa_account(), &body), 400, &format!("PATCH Visa {body}"));
    }
    let res = server.patch(&f.transfer_payee(), &json!({"name": "Corner Shop"}));
    assert_error(&res, 400, "rename transfer payee to a taken name");
    assert_error(&server.delete(&f.internal_group()), 400, "DELETE internal group holding a category");
    assert_eq!(f.snapshot(&server), before);
}

#[test]
fn spec_3_4_head_works_wherever_get_does_inside_plans() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    for path in [
        f.path("/accounts"),
        f.account(),
        f.path("/payees"),
        f.payee(),
        f.transfer_payee(),
        f.path("/categories"),
        f.category(),
        f.payment_category(),
        f.group(),
        f.internal_group(),
    ] {
        let res = server.head(&path);
        assert_status(&res, 200, &format!("HEAD {path}"));
        assert!(res.body.is_empty(), "HEAD {path} returned a body: {:?}", res.text());
    }
    for path in [f.path("/accounts/nope"), f.path("/categories/ready_to_assign"), "/plans/nope/payees".to_owned()] {
        let res = server.head(&path);
        assert_status(&res, 404, &format!("HEAD {path}"));
        assert!(res.body.is_empty(), "HEAD {path} returned a body: {:?}", res.text());
    }
}

// ---- 3.5 ----

#[test]
fn spec_3_5_ids_are_unique_across_plans_and_kinds() {
    let server = Server::start();
    let mut all = Vec::new();
    for name in ["One", "Two"] {
        let f = Fixture::new(&server, name);
        let starting_balance = server.payees(&f.plan).into_iter().find(|p| p["name"] == "Starting Balance");
        all.push(f.plan.clone());
        all.extend([
            id_of(&f.checking),
            id_of(&f.visa),
            str_field(&f.checking, "transfer_payee_id"),
            str_field(&f.visa, "transfer_payee_id"),
            str_field(&f.visa, "payment_category_id"),
            id_of(&starting_balance.expect("no Starting Balance payee")),
            id_of(&f.payee),
            id_of(&f.group),
            id_of(&f.category),
            id_of(&f.internal),
        ]);
    }
    let unique: HashSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "ids repeat: {all:?}");
    for id in &all {
        assert!(is_path_safe(id), "id {id:?} has characters other than ASCII letters, digits, - and _");
        assert_ne!(id, "ready_to_assign");
    }
}

#[test]
fn spec_3_5_each_plan_has_its_own_internal_group_id() {
    let server = Server::start();
    let a = server.plan_id("A");
    let b = server.plan_id("B");
    let group_a = id_of(&server.groups(&a)[0]);
    let group_b = id_of(&server.groups(&b)[0]);
    assert_ne!(group_a, group_b, "two plans' internal groups share an id");
    assert_ne!(group_a, a);
    assert_ne!(group_b, b);
}

#[test]
fn spec_3_5_ids_of_deleted_plan_contents_are_not_reused() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let mut deleted = Vec::new();
    for i in 0..3 {
        let visa = server.add_account(&plan, &format!("Visa {i}"), "creditCard", 0);
        let payee = server.create_payee(&plan, &format!("Payee {i}"));
        let group = server.create_group(&plan, &format!("Group {i}"));
        deleted.extend([
            id_of(&visa),
            str_field(&visa, "transfer_payee_id"),
            str_field(&visa, "payment_category_id"),
            id_of(&payee),
            id_of(&group),
        ]);
        assert_status(&server.delete(&format!("/plans/{plan}/accounts/{}", id_of(&visa))), 204, "DELETE account");
        assert_status(&server.delete(&format!("/plans/{plan}/payees/{}", id_of(&payee))), 204, "DELETE payee");
        assert_status(&server.delete(&format!("/plans/{plan}/category_groups/{}", id_of(&group))), 204, "DELETE group");
    }
    for i in 0..5 {
        let visa = server.add_account(&plan, &format!("New {i}"), "creditCard", 0);
        let group = server.create_group(&plan, &format!("New {i}"));
        for id in [
            id_of(&visa),
            str_field(&visa, "transfer_payee_id"),
            str_field(&visa, "payment_category_id"),
            id_of(&server.create_payee(&plan, &format!("New {i}"))),
            id_of(&group),
            id_of(&server.create_category(&plan, &id_of(&group), "New")),
        ] {
            assert!(!deleted.contains(&id), "deleted id {id} was reused");
        }
    }
}

#[test]
fn spec_3_5_ignores_client_supplied_ids() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let account = server.create_account(&plan, &json!({"id": "my-account", "name": "Checking", "type": "checking"}));
    let payee = server.create(&format!("/plans/{plan}/payees"), &json!({"id": "my-payee", "name": "Shop"}));
    let group = server.create(&format!("/plans/{plan}/category_groups"), &json!({"id": "my-group", "name": "Bills"}));
    let body = json!({"id": "my-category", "group_id": id_of(&group), "name": "Rent"});
    let category = server.create(&format!("/plans/{plan}/categories"), &body);
    for (obj, chosen, kind) in [
        (&account, "my-account", "accounts"),
        (&payee, "my-payee", "payees"),
        (&group, "my-group", "category_groups"),
        (&category, "my-category", "categories"),
    ] {
        assert_ne!(obj["id"], chosen, "server used the client's id");
        assert_error(&server.get(&format!("/plans/{plan}/{kind}/{chosen}")), 404, &format!("GET {kind}/{chosen}"));
    }
}

// ---- 3.6 ----

#[test]
fn spec_3_6_plan_content_responses_have_every_example_field() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let cash = server.add_account(&f.plan, "Cash", "cash", 0);
    for (what, obj) in [
        ("POST checking", f.checking.clone()),
        ("POST visa", f.visa.clone()),
        ("POST cash", cash.clone()),
        ("GET account", server.get_json(&f.account())),
        ("PATCH account", server.patch_ok(&f.account(), &json!({"note": null}))),
    ] {
        assert_fields(&obj, &ACCOUNT_FIELDS, what);
    }
    for account in server.accounts(&f.plan) {
        assert_fields(&account, &ACCOUNT_FIELDS, "listed account");
    }
    for (what, obj) in [
        ("POST payee", f.payee.clone()),
        ("GET payee", server.get_json(&f.payee())),
        ("GET transfer payee", server.get_json(&f.transfer_payee())),
        ("PATCH payee", server.patch_ok(&f.payee(), &json!({"name": "Shop"}))),
    ] {
        assert_fields(&obj, &PAYEE_FIELDS, what);
    }
    for payee in server.payees(&f.plan) {
        assert_fields(&payee, &PAYEE_FIELDS, "listed payee");
    }
    let mut groups = vec![
        ("POST group", f.group.clone()),
        ("GET group", server.get_json(&f.group())),
        ("GET internal group", server.get_json(&f.internal_group())),
        ("PATCH group", server.patch_ok(&f.group(), &json!({"hidden": true}))),
    ];
    groups.extend(server.groups(&f.plan).into_iter().map(|g| ("listed group", g)));
    for (what, group) in &groups {
        assert_fields(group, &GROUP_FIELDS, what);
        for category in group["categories"].as_array().unwrap() {
            assert_fields(category, &CATEGORY_FIELDS, &format!("category in {what}"));
        }
    }
    for (what, obj) in [
        ("POST category", f.category.clone()),
        ("GET category", server.get_json(&f.category())),
        ("GET payment category", server.get_json(&f.payment_category())),
        ("PATCH category", server.patch_ok(&f.category(), &json!({"note": null}))),
        ("PATCH payment category", server.patch_ok(&f.payment_category(), &json!({"hidden": true}))),
    ] {
        assert_fields(&obj, &CATEGORY_FIELDS, what);
    }
}

#[test]
fn spec_3_6_account_post_ignores_undefined_fields() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let other = server.plan_id("Other");
    let account = server.create_account(
        &plan,
        &json!({"name": "Checking", "type": "checking", "closed": true, "on_budget": false, "balance": 5,
                "cleared_balance": 5, "uncleared_balance": 5, "transfer_payee_id": "x",
                "payment_category_id": "y", "plan_id": other, "category_id": "ready_to_assign"}),
    );
    assert_matches(
        &account,
        &json!({"name": "Checking", "type": "checking", "closed": false, "on_budget": true, "balance": 0,
                "cleared_balance": 0, "uncleared_balance": 0, "payment_category_id": null}),
        "POST with undefined fields",
    );
    assert_ne!(account["transfer_payee_id"], "x");
    assert_eq!(server.accounts(&plan).len(), 1);
    assert!(server.accounts(&other).is_empty());
}

#[test]
fn spec_3_6_payee_post_ignores_transfer_account_id() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let checking = server.add_account(&plan, "Checking", "checking", 0);
    let payee = server.create(
        &format!("/plans/{plan}/payees"),
        &json!({"name": "Corner Shop", "transfer_account_id": id_of(&checking)}),
    );
    assert_matches(&payee, &json!({"name": "Corner Shop", "transfer_account_id": null}), "POST payee");
    let url = format!("/plans/{plan}/payees/{}", id_of(&payee));
    assert_eq!(server.patch_ok(&url, &json!({"name": "Shop"}))["name"], "Shop");
    assert_status(&server.delete(&url), 204, "DELETE the payee");
}

#[test]
fn spec_3_6_group_post_ignores_hidden_internal_and_categories() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let group = server.create(
        &format!("/plans/{plan}/category_groups"),
        &json!({"name": "Bills", "hidden": true, "internal": true, "categories": [{"name": "Rent"}]}),
    );
    assert_matches(&group, &json!({"name": "Bills", "hidden": false, "internal": false, "categories": []}), "POST");
    let res = server.post(&format!("/plans/{plan}/categories"), &json!({"group_id": id_of(&group), "name": "Rent"}));
    assert_status(&res, 201, "POST category into the new group");
}

#[test]
fn spec_3_6_category_post_ignores_hidden_and_payment_account_id() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let visa = server.add_account(&plan, "Visa", "creditCard", 0);
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let category = server.create(
        &format!("/plans/{plan}/categories"),
        &json!({"group_id": bills, "name": "Rent", "hidden": true, "payment_account_id": id_of(&visa)}),
    );
    assert_matches(&category, &json!({"group_id": bills, "hidden": false, "payment_account_id": null}), "POST");
    let url = format!("/plans/{plan}/categories/{}", id_of(&category));
    assert_eq!(server.patch_ok(&url, &json!({"name": "Rent 2"}))["name"], "Rent 2");
}

#[test]
fn spec_3_6_patches_ignore_undefined_fields() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    let other_account = id_of(&f.checking);
    for (path, body) in [
        (
            f.visa_account(),
            json!({"balance": 0, "cleared_balance": 0, "on_budget": false, "transfer_payee_id": "x",
                   "payment_category_id": null, "id": "y"}),
        ),
        (f.payee(), json!({"transfer_account_id": other_account, "id": "y"})),
        (f.transfer_payee(), json!({"transfer_account_id": null})),
        (f.group(), json!({"internal": true, "categories": []})),
        (f.internal_group(), json!({"internal": false, "categories": []})),
        (f.category(), json!({"payment_account_id": other_account, "id": "y"})),
        (f.payment_category(), json!({"payment_account_id": null})),
    ] {
        assert_status(&server.patch(&path, &body), 200, &format!("PATCH {path} {body}"));
    }
    assert_eq!(f.snapshot(&server), before, "undefined PATCH fields changed something");
}

#[test]
fn spec_3_6_plan_content_lists_ignore_query_parameters() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    server.patch_ok(&f.group(), &json!({"hidden": true}));
    let before = f.snapshot(&server);
    let p = &f.plan;
    assert_eq!(server.list(&format!("/plans/{p}/accounts?type=savings&closed=true&limit=0")), before["accounts"].as_array().unwrap().clone());
    assert_eq!(server.list(&format!("/plans/{p}/payees?name=Shop&transfer=true")), before["payees"].as_array().unwrap().clone());
    assert_eq!(server.list(&format!("/plans/{p}/categories?hidden=false&group_id=x")), before["categories"].as_array().unwrap().clone());
    let got = server.get_json(&format!("{}?verbose=1", f.category()));
    assert_eq!(got["id"], f.category["id"]);
}

// ---- 3.9 ----

#[test]
fn spec_3_9_account_group_and_category_names_are_up_to_100_characters() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let p = &f.plan;
    let bills = id_of(&f.group);
    for name in ["a".repeat(100), "😀".repeat(100), "é".repeat(100)] {
        let account = server.create_account(p, &json!({"name": name, "type": "creditCard"}));
        assert_eq!(account["name"], name.as_str());
        let payee = server.get_json(&format!("/plans/{p}/payees/{}", str_field(&account, "transfer_payee_id")));
        assert_eq!(payee["name"], format!("Transfer : {name}"));
        let category = server.get_json(&format!("/plans/{p}/categories/{}", str_field(&account, "payment_category_id")));
        assert_eq!(category["name"], name.as_str());
        assert_eq!(server.create_group(p, &name)["name"], name.as_str());
        assert_eq!(server.create_category(p, &bills, &name)["name"], name.as_str());
        for path in [f.account(), f.group(), f.category()] {
            assert_eq!(server.patch_ok(&path, &json!({"name": name}))["name"], name.as_str());
        }
    }
    let before = f.snapshot(&server);
    for name in ["a".repeat(101), "😀".repeat(101), "e\u{301}".repeat(51)] {
        let res = server.post(&f.path("/accounts"), &json!({"name": name, "type": "checking"}));
        assert_status(&res, 400, "POST account with a long name");
        assert_status(&server.post(&f.path("/category_groups"), &json!({"name": name})), 400, "POST group");
        let res = server.post(&f.path("/categories"), &json!({"group_id": bills, "name": name}));
        assert_status(&res, 400, "POST category with a long name");
        for path in [f.account(), f.group(), f.category()] {
            assert_status(&server.patch(&path, &json!({"name": name})), 400, &format!("PATCH {path} long name"));
        }
    }
    assert_eq!(f.snapshot(&server), before);
}

#[test]
fn spec_3_9_payee_names_are_up_to_200_characters() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = format!("/plans/{plan}/payees");
    for name in ["a".repeat(101), "a".repeat(200), "😀".repeat(200), "e\u{301}".repeat(100)] {
        assert_eq!(server.create(&url, &json!({"name": name}))["name"], name.as_str());
    }
    let shop = server.create_payee(&plan, "Corner Shop");
    let shop_url = format!("{url}/{}", id_of(&shop));
    let long = "b".repeat(200);
    assert_eq!(server.patch_ok(&shop_url, &json!({"name": long}))["name"], long.as_str());
    for name in ["a".repeat(201), "😀".repeat(201), "e\u{301}".repeat(101)] {
        assert_status(&server.post(&url, &json!({"name": name})), 400, "POST payee with 201+ characters");
        assert_status(&server.patch(&shop_url, &json!({"name": name})), 400, "PATCH payee with 201+ characters");
    }
    assert_eq!(server.payees(&plan).len(), 5);
    assert_eq!(server.get_json(&shop_url)["name"], long.as_str());
}

#[test]
fn spec_3_9_notes_are_up_to_500_characters() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let bills = id_of(&f.group);
    for note in ["a".repeat(500), "😀".repeat(500), "e\u{301}".repeat(250)] {
        let account = server.create_account(&f.plan, &json!({"name": "A", "type": "cash", "note": note}));
        assert_eq!(account["note"], note.as_str());
        let category = server.create(&f.path("/categories"), &json!({"group_id": bills, "name": "C", "note": note}));
        assert_eq!(category["note"], note.as_str());
        for path in [f.account(), f.category(), f.payment_category()] {
            assert_eq!(server.patch_ok(&path, &json!({"note": note}))["note"], note.as_str());
        }
    }
    let before = f.snapshot(&server);
    for note in ["a".repeat(501), "😀".repeat(501), "e\u{301}".repeat(251)] {
        let res = server.post(&f.path("/accounts"), &json!({"name": "A", "type": "cash", "note": note}));
        assert_status(&res, 400, "POST account with a long note");
        let res = server.post(&f.path("/categories"), &json!({"group_id": bills, "name": "C", "note": note}));
        assert_status(&res, 400, "POST category with a long note");
        for path in [f.account(), f.category(), f.payment_category()] {
            assert_status(&server.patch(&path, &json!({"note": note})), 400, &format!("PATCH {path} long note"));
        }
    }
    assert_eq!(f.snapshot(&server), before);
}

#[test]
fn spec_3_9_notes_may_be_empty_or_blank_and_are_stored_exactly() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let bills = id_of(&f.group);
    for note in ["", " ", "\n", "  due on the 1st  ", "line 1\nline 2", "Cafe\u{301}"] {
        let account = server.create_account(&f.plan, &json!({"name": "A", "type": "cash", "note": note}));
        assert_eq!(account["note"], note, "POST account note {note:?}");
        assert_eq!(server.get_json(&f.path(&format!("/accounts/{}", id_of(&account))))["note"], note);
        let category = server.create(&f.path("/categories"), &json!({"group_id": bills, "name": "C", "note": note}));
        assert_eq!(category["note"], note, "POST category note {note:?}");
        for path in [f.account(), f.category()] {
            assert_eq!(server.patch_ok(&path, &json!({"note": note}))["note"], note, "PATCH {path} note {note:?}");
            assert_eq!(server.get_json(&path)["note"], note);
        }
    }
}

#[test]
fn spec_3_9_plan_content_names_are_stored_exactly() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let bills = id_of(&f.group);
    for name in ["  Spaced  ", "a\tb", "Cafe\u{301}", "Café", "<b>&amp;</b> \"q\" \\ /", "\u{200b}"] {
        let account = server.create_account(&f.plan, &json!({"name": name, "type": "checking"}));
        assert_eq!(account["name"], name);
        assert_eq!(server.create_payee(&f.plan, name)["name"], name);
        let group = server.create_group(&f.plan, name);
        assert_eq!(server.get_json(&f.path(&format!("/category_groups/{}", id_of(&group))))["name"], name);
        let category = server.create_category(&f.plan, &bills, name);
        assert_eq!(server.get_json(&f.path(&format!("/categories/{}", id_of(&category))))["name"], name);
    }
}

#[test]
fn spec_3_9_plan_content_names_must_have_a_non_white_space_character() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    let bills = id_of(&f.group);
    for name in [" ", "\t", "\u{a0}", "\u{2003}\u{202f}", "\u{85}", "\u{b}\u{c}", "\u{1680}"] {
        let what = format!("{name:?}");
        assert_status(&server.post(&f.path("/accounts"), &json!({"name": name, "type": "cash"})), 400, &what);
        assert_status(&server.post(&f.path("/payees"), &json!({"name": name})), 400, &what);
        assert_status(&server.post(&f.path("/category_groups"), &json!({"name": name})), 400, &what);
        assert_status(&server.post(&f.path("/categories"), &json!({"group_id": bills, "name": name})), 400, &what);
        for path in [f.account(), f.payee(), f.group(), f.category()] {
            assert_status(&server.patch(&path, &json!({"name": name})), 400, &format!("PATCH {path} {what}"));
        }
    }
    assert_eq!(f.snapshot(&server), before);
}

#[test]
fn spec_3_9_accounts_groups_and_categories_may_share_names() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let a = server.add_account(&plan, "Checking", "checking", 0);
    let b = server.add_account(&plan, "Checking", "checking", 1000);
    assert_ne!(a["id"], b["id"]);
    let v1 = server.add_account(&plan, "Visa", "creditCard", 0);
    let v2 = server.add_account(&plan, "Visa", "creditCard", 0);
    assert_ne!(v1["payment_category_id"], v2["payment_category_id"]);
    let g1 = id_of(&server.create_group(&plan, "Bills"));
    let g2 = id_of(&server.create_group(&plan, "Bills"));
    let r1 = server.create_category(&plan, &g1, "Rent");
    let r2 = server.create_category(&plan, &g1, "Rent");
    let r3 = server.create_category(&plan, &g2, "Visa");
    assert_ne!(r1["id"], r2["id"]);
    server.patch_ok(&format!("/plans/{plan}/categories/{}", id_of(&r3)), &json!({"name": "Rent", "group_id": g1}));
    assert_eq!(names_of(&server.accounts(&plan)), ["Checking", "Checking", "Visa", "Visa"]);
    let groups = server.groups(&plan);
    assert_eq!(names_of(&groups), ["Credit Card Payments", "Bills", "Bills"]);
    assert_eq!(names_of(groups[0]["categories"].as_array().unwrap()), ["Visa", "Visa"]);
    assert_eq!(names_of(groups[1]["categories"].as_array().unwrap()), ["Rent", "Rent", "Rent"]);
}

// ---- 3.10 ----

#[test]
fn spec_3_10_null_for_non_nullable_plan_content_fields_returns_400() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    let bills = id_of(&f.group);
    for (path, body) in [
        (f.path("/accounts"), json!({"name": null, "type": "checking"})),
        (f.path("/accounts"), json!({"name": "A", "type": null})),
        (f.path("/accounts"), json!({"name": "A", "type": "checking", "starting_balance": null})),
        (f.path("/accounts"), json!({"name": "A", "type": "checking", "starting_balance": null, "starting_date": START_DATE})),
        (f.path("/accounts"), json!({"name": "A", "type": "checking", "starting_balance": 1000, "starting_date": null})),
        (f.path("/payees"), json!({"name": null})),
        (f.path("/category_groups"), json!({"name": null})),
        (f.path("/categories"), json!({"group_id": null, "name": "Rent"})),
        (f.path("/categories"), json!({"group_id": bills, "name": null})),
    ] {
        assert_status(&server.post(&path, &body), 400, &format!("POST {path} {body}"));
    }
    for (path, body) in [
        (f.account(), json!({"name": null})),
        (f.account(), json!({"closed": null})),
        (f.account(), json!({"type": null})),
        (f.payee(), json!({"name": null})),
        (f.group(), json!({"name": null})),
        (f.group(), json!({"hidden": null})),
        (f.category(), json!({"name": null})),
        (f.category(), json!({"hidden": null})),
        (f.category(), json!({"group_id": null})),
        (f.payment_category(), json!({"hidden": null})),
    ] {
        assert_status(&server.patch(&path, &body), 400, &format!("PATCH {path} {body}"));
    }
    assert_eq!(f.snapshot(&server), before);
}

#[test]
fn spec_3_10_null_note_clears_it() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    for path in [f.account(), f.visa_account(), f.category(), f.payment_category()] {
        assert_eq!(server.patch_ok(&path, &json!({"note": "something"}))["note"], "something");
        let patched = server.patch_ok(&path, &json!({"note": null}));
        assert_matches(&patched, &json!({"note": null}), &format!("PATCH {path} note null"));
        assert_matches(&server.get_json(&path), &json!({"note": null}), &format!("GET {path} after clearing note"));
    }
}

#[test]
fn spec_3_10_plan_content_patch_changes_only_present_fields() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    server.patch_ok(&f.account(), &json!({"note": "n"}));
    let patched = server.patch_ok(&f.account(), &json!({"name": "Main"}));
    assert_matches(&patched, &json!({"name": "Main", "note": "n", "closed": false, "balance": 2500000}), "account");
    server.patch_ok(&f.category(), &json!({"note": "n", "hidden": true}));
    let patched = server.patch_ok(&f.category(), &json!({"name": "Rent 2"}));
    let want = json!({"name": "Rent 2", "note": "n", "hidden": true, "group_id": id_of(&f.group)});
    assert_matches(&patched, &want, "category");
    server.patch_ok(&f.group(), &json!({"hidden": true}));
    let patched = server.patch_ok(&f.group(), &json!({"name": "Monthly"}));
    assert_matches(&patched, &json!({"name": "Monthly", "hidden": true}), "group");
    assert_eq!(server.patch_ok(&f.group(), &json!({"hidden": false}))["name"], "Monthly");
}

#[test]
fn spec_3_10_plan_content_patch_without_known_fields_changes_nothing() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    for path in [
        f.account(),
        f.visa_account(),
        f.payee(),
        f.transfer_payee(),
        f.group(),
        f.internal_group(),
        f.category(),
        f.payment_category(),
    ] {
        let was = server.get_json(&path);
        for body in [json!({}), json!({"nickname": "x"})] {
            let res = server.patch(&path, &body);
            assert_status(&res, 200, &format!("PATCH {path} {body}"));
            assert_matches(&res.json(), &spec_fields(&was), &format!("PATCH {path} {body}"));
        }
    }
    assert_eq!(f.snapshot(&server), before);
}

// ---- 3.11 ----

#[test]
fn spec_3_11_failed_account_post_creates_nothing() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let before = server.groups(&plan);
    for body in [
        json!({"name": "Visa", "type": "creditCard", "starting_balance": -350000}),
        json!({"name": "Visa", "type": "creditCard", "starting_balance": -350000, "starting_date": "2026-02-30"}),
        json!({"name": "Visa", "type": "creditCard", "starting_balance": -350000, "starting_date": START_DATE, "note": 5}),
        json!({"name": "Visa", "type": "creditCard", "starting_balance": 1.5, "starting_date": START_DATE}),
        json!({"name": "Visa", "type": "creditCard", "note": "x".repeat(501)}),
        json!({"name": "Visa", "type": "creditcard", "starting_balance": -1, "starting_date": START_DATE}),
        json!({"name": "", "type": "creditCard", "starting_balance": -1, "starting_date": START_DATE}),
        json!({"name": "Checking", "type": "checking", "starting_balance": 1000, "starting_date": "2026-13-01"}),
    ] {
        assert_status(&server.post(&format!("/plans/{plan}/accounts"), &body), 400, &format!("POST {body}"));
        assert!(server.accounts(&plan).is_empty(), "POST {body} created an account");
        assert!(server.payees(&plan).is_empty(), "POST {body} created payees: {:?}", server.payees(&plan));
        assert_eq!(server.groups(&plan), before, "POST {body} created a payment category");
    }
    let visa = server.add_account(&plan, "Visa", "creditCard", -350000);
    assert_eq!(names_of(&server.payees(&plan)), ["Transfer : Visa", "Starting Balance"]);
    let listed = server.groups(&plan)[0]["categories"].clone();
    assert_eq!(ids_of(listed.as_array().unwrap()), [str_field(&visa, "payment_category_id")]);
}

#[test]
fn spec_3_11_failed_account_patch_changes_nothing() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    server.patch_ok(&f.visa_account(), &json!({"note": "n"}));
    let before = f.snapshot(&server);
    for (body, status) in [
        (json!({"name": "Gold", "closed": true}), 409),
        (json!({"note": "changed", "closed": true}), 409),
        (json!({"name": "Gold", "type": "checking"}), 400),
        (json!({"name": "Gold", "note": 5}), 400),
        (json!({"note": "changed", "closed": "yes"}), 400),
        (json!({"note": null, "name": ""}), 400),
        (json!({"name": "Gold", "note": "x".repeat(501)}), 400),
    ] {
        assert_status(&server.patch(&f.visa_account(), &body), status, &format!("PATCH Visa {body}"));
        assert_eq!(f.snapshot(&server), before, "PATCH Visa {body} changed something");
    }
}

#[test]
fn spec_3_11_failed_category_and_group_patches_change_nothing() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    for (path, body) in [
        (f.category(), json!({"name": "X", "note": "n", "hidden": true, "group_id": "no-such-group"})),
        (f.category(), json!({"name": "X", "note": "n", "group_id": id_of(&f.internal)})),
        (f.category(), json!({"group_id": id_of(&f.group), "name": "X", "hidden": "yes"})),
        (f.payment_category(), json!({"note": "n", "hidden": true, "name": "Card"})),
        (f.payment_category(), json!({"note": "n", "hidden": true, "group_id": id_of(&f.group)})),
        (f.group(), json!({"name": "X", "hidden": "yes"})),
        (f.group(), json!({"hidden": true, "name": ""})),
        (f.internal_group(), json!({"hidden": true})),
        (f.payee(), json!({"name": "Starting Balance"})),
        (f.transfer_payee(), json!({"name": "Main"})),
    ] {
        let res = server.patch(&path, &body);
        assert!(res.status == 400 || res.status == 409, "PATCH {path} {body}: status {}", res.status);
        assert_eq!(f.snapshot(&server), before, "PATCH {path} {body} changed something");
    }
}

#[test]
fn spec_3_11_failed_deletes_change_nothing() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let before = f.snapshot(&server);
    let starting_balance = server.payees(&f.plan).into_iter().find(|p| p["name"] == "Starting Balance").unwrap();
    for path in [
        f.account(),
        f.visa_account(),
        f.group(),
        f.internal_group(),
        f.transfer_payee(),
        f.path(&format!("/payees/{}", id_of(&starting_balance))),
        f.path(&format!("/payees/{}", str_field(&f.visa, "transfer_payee_id"))),
    ] {
        let res = server.delete(&path);
        assert!(res.status == 400 || res.status == 409, "DELETE {path}: status {}", res.status);
        assert_eq!(f.snapshot(&server), before, "DELETE {path} changed something");
    }
}

#[test]
fn spec_3_11_concurrent_payee_creates_with_one_name_succeed_once() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let url = format!("/plans/{plan}/payees");
    let statuses: Vec<u16> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let (server, url) = (&server, &url);
                s.spawn(move || server.post(url, &json!({"name": "Corner Shop"})).status)
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(statuses.iter().filter(|&&s| s == 201).count(), 1, "statuses {statuses:?}");
    assert_eq!(statuses.iter().filter(|&&s| s == 409).count(), 15, "statuses {statuses:?}");
    assert_eq!(server.payees(&plan).len(), 1);
}

#[test]
fn spec_3_11_concurrent_payee_renames_to_one_name_succeed_once() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let urls: Vec<String> = (0..16)
        .map(|i| format!("/plans/{plan}/payees/{}", id_of(&server.create_payee(&plan, &format!("Payee {i}")))))
        .collect();
    let statuses: Vec<u16> = std::thread::scope(|s| {
        let handles: Vec<_> = urls
            .iter()
            .map(|url| {
                let server = &server;
                s.spawn(move || server.patch(url, &json!({"name": "Corner Shop"})).status)
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(statuses.iter().filter(|&&s| s == 200).count(), 1, "statuses {statuses:?}");
    assert_eq!(statuses.iter().filter(|&&s| s == 409).count(), 15, "statuses {statuses:?}");
    let named = server.payees(&plan).into_iter().filter(|p| p["name"] == "Corner Shop").count();
    assert_eq!(named, 1);
}

#[test]
fn spec_3_11_concurrent_starting_balances_share_one_payee() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let accounts: Vec<Value> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let (server, plan) = (&server, &plan);
                s.spawn(move || server.add_account(plan, &format!("Account {i}"), "checking", 1000 + i))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let payees = server.payees(&plan);
    let starting = payees.iter().filter(|p| p["name"] == "Starting Balance").count();
    assert_eq!(starting, 1, "payees: {payees:?}");
    assert_eq!(payees.len(), 17, "payees: {payees:?}");
    for account in &accounts {
        let got = server.get_json(&format!("/plans/{plan}/accounts/{}", id_of(account)));
        assert_eq!(got["balance"], account["balance"]);
    }
}

#[test]
fn spec_3_11_concurrent_category_creates_all_land_in_the_group() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    let bills = id_of(&server.create_group(&plan, "Bills"));
    let ids: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let (server, plan, bills) = (&server, &plan, &bills);
                s.spawn(move || id_of(&server.create_category(plan, bills, &format!("Category {i}"))))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let group = server.get_json(&format!("/plans/{plan}/category_groups/{bills}"));
    let listed: HashSet<String> = ids_of(group["categories"].as_array().unwrap()).into_iter().collect();
    assert_eq!(listed.len(), 16, "group: {group}");
    assert_eq!(listed, ids.into_iter().collect::<HashSet<_>>());
}

// ---- 3.12 ----

#[test]
fn spec_3_12_plan_content_lists_are_arrays() {
    let server = Server::start();
    let plan = server.plan_id("Household");
    for rest in ["accounts", "payees", "categories"] {
        let body = server.get_json(&format!("/plans/{plan}/{rest}"));
        assert!(body.is_array(), "GET {rest} returned {body}");
    }
    let group = server.groups(&plan)[0].clone();
    assert!(group["categories"].is_array(), "categories of a group: {group}");
}

// ---- 4.4 ----

#[test]
fn spec_4_4_deleting_plan_deletes_its_contents() {
    let server = Server::start();
    let f = Fixture::new(&server, "Household");
    let keep = Fixture::new(&server, "Keep");
    let kept = keep.snapshot(&server);
    assert_status(&server.delete(&format!("/plans/{}", f.plan)), 204, "DELETE plan");
    for path in [
        f.path("/accounts"),
        f.path("/payees"),
        f.path("/categories"),
        f.account(),
        f.payee(),
        f.transfer_payee(),
        f.group(),
        f.internal_group(),
        f.category(),
        f.payment_category(),
    ] {
        assert_error(&server.get(&path), 404, &format!("GET {path} after deleting its plan"));
    }
    assert_error(&server.post(&f.path("/payees"), &json!({"name": "Bakery"})), 404, "POST payee to a deleted plan");
    assert_eq!(keep.snapshot(&server), kept, "deleting one plan changed another");
}

// ---- 4.5 ----

#[test]
fn spec_4_5_paths_to_another_plans_contents_return_404() {
    let server = Server::start();
    let a = Fixture::new(&server, "A");
    let b = Fixture::new(&server, "B");
    let a_before = a.snapshot(&server);
    let b_before = b.snapshot(&server);
    let foreign = [
        format!("/accounts/{}", id_of(&a.checking)),
        format!("/accounts/{}", id_of(&a.visa)),
        format!("/payees/{}", id_of(&a.payee)),
        format!("/payees/{}", str_field(&a.checking, "transfer_payee_id")),
        format!("/category_groups/{}", id_of(&a.group)),
        format!("/category_groups/{}", id_of(&a.internal)),
        format!("/categories/{}", id_of(&a.category)),
        format!("/categories/{}", str_field(&a.visa, "payment_category_id")),
    ];
    for rest in &foreign {
        let path = b.path(rest);
        assert_error(&server.get(&path), 404, &format!("GET {path}"));
        assert_error(&server.patch(&path, &json!({"name": "Hijacked"})), 404, &format!("PATCH {path}"));
        if !rest.starts_with("/categories/") {
            assert_error(&server.delete(&path), 404, &format!("DELETE {path}"));
        }
    }
    assert_eq!(a.snapshot(&server), a_before, "requests through plan B changed plan A");
    assert_eq!(b.snapshot(&server), b_before, "requests through plan B changed plan B");
}

#[test]
fn spec_4_5_body_references_to_another_plans_groups_return_400() {
    let server = Server::start();
    let a = Fixture::new(&server, "A");
    let b = Fixture::new(&server, "B");
    let a_before = a.snapshot(&server);
    let b_before = b.snapshot(&server);
    for group in [id_of(&a.group), id_of(&a.internal)] {
        let res = server.post(&b.path("/categories"), &json!({"group_id": group, "name": "Sneaky"}));
        assert_error(&res, 400, "POST category into another plan's group");
        let res = server.patch(&b.category(), &json!({"group_id": group}));
        assert_error(&res, 400, "move a category into another plan's group");
    }
    assert_eq!(a.snapshot(&server), a_before);
    assert_eq!(b.snapshot(&server), b_before);
}

#[test]
fn spec_4_5_lists_show_only_the_plans_own_contents() {
    let server = Server::start();
    let a = Fixture::new(&server, "A");
    let b = server.plan_id("B");
    assert!(server.accounts(&b).is_empty(), "plan B lists accounts: {:?}", server.accounts(&b));
    assert!(server.payees(&b).is_empty(), "plan B lists payees: {:?}", server.payees(&b));
    let groups = server.groups(&b);
    assert_eq!(groups.len(), 1, "plan B lists groups: {groups:?}");
    assert_eq!(groups[0]["categories"], json!([]));
    assert_eq!(server.accounts(&a.plan).len(), 2);
    assert_eq!(server.groups(&a.plan).len(), 2);
}

#[test]
fn spec_4_5_payee_names_are_unique_only_within_a_plan() {
    let server = Server::start();
    let a = server.plan_id("A");
    let b = server.plan_id("B");
    let in_a = server.create_payee(&a, "Corner Shop");
    let in_b = server.create_payee(&b, "Corner Shop");
    assert_ne!(in_a["id"], in_b["id"]);
    let other = server.create_payee(&b, "Bakery");
    let res = server.patch(&format!("/plans/{a}/payees/{}", id_of(&in_a)), &json!({"name": "Bakery"}));
    assert_status(&res, 200, "rename to a name taken only in another plan");
    assert_eq!(server.get_json(&format!("/plans/{b}/payees/{}", id_of(&other)))["name"], "Bakery");
}

#[test]
fn spec_4_5_each_plan_has_its_own_starting_balance_payee() {
    let server = Server::start();
    let a = server.plan_id("A");
    let b = server.plan_id("B");
    server.add_account(&a, "Checking", "checking", 1000);
    server.add_account(&b, "Checking", "checking", 2000);
    let find = |plan: &str| {
        let found: Vec<Value> = server.payees(plan).into_iter().filter(|p| p["name"] == "Starting Balance").collect();
        assert_eq!(found.len(), 1, "plan {plan} payees: {:?}", server.payees(plan));
        id_of(&found[0])
    };
    let (sa, sb) = (find(&a), find(&b));
    assert_ne!(sa, sb, "two plans share a Starting Balance payee");
    assert_error(&server.get(&format!("/plans/{b}/payees/{sa}")), 404, "plan A's payee through plan B");
}
