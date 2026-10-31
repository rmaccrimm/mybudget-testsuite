use mybudget_testsuite::{Server, Value, json};

fn plans(server: &Server) -> Vec<Value> {
    let res = server.get("/plans");
    assert_eq!(res.status, 200, "GET /plans: {}", res.text());
    res.json().as_array().expect("GET /plans did not return an array").clone()
}

fn ids(list: &[Value]) -> Vec<&str> {
    list.iter().map(|p| p["id"].as_str().expect("plan id is not a string")).collect()
}

// ---- 4.1 ----

#[test]
fn spec_4_1_creates_plan_with_default_currency() {
    let server = Server::start();
    let res = server.post("/plans", &json!({"name": "Household"}));
    assert_eq!(res.status, 201, "{}", res.text());
    let plan = res.json();
    assert!(plan["id"].is_string(), "id is not a string: {plan}");
    assert!(!plan["id"].as_str().unwrap().is_empty(), "id is empty");
    assert_eq!(plan["name"], "Household");
    assert_eq!(plan["currency_code"], "USD");
}

#[test]
fn spec_4_1_creates_plan_with_given_currency() {
    let server = Server::start();
    for code in ["EUR", "JPY", "GBP", "USD"] {
        let res = server.post("/plans", &json!({"name": "Household", "currency_code": code}));
        assert_eq!(res.status, 201, "currency {code}: {}", res.text());
        assert_eq!(res.json()["currency_code"], code);
    }
}

#[test]
fn spec_4_1_stores_any_three_uppercase_letters() {
    let server = Server::start();
    for code in ["ZZZ", "AAA", "QQQ"] {
        let res = server.post("/plans", &json!({"name": "Household", "currency_code": code}));
        assert_eq!(res.status, 201, "currency {code}: {}", res.text());
        assert_eq!(res.json()["currency_code"], code);
    }
}

#[test]
fn spec_4_1_rejects_invalid_currency_code() {
    let server = Server::start();
    for code in ["", "usd", "Usd", "US", "USDD", "U", "U1D", "123", "US ", " USD", "ÜSD", "ＵＳＤ", "U-D"] {
        let res = server.post("/plans", &json!({"name": "Household", "currency_code": code}));
        assert_eq!(res.status, 400, "currency {code:?}: {}", res.text());
    }
    assert!(plans(&server).is_empty());
}

#[test]
fn spec_4_1_requires_name() {
    let server = Server::start();
    let res = server.post("/plans", &json!({"currency_code": "USD"}));
    assert_eq!(res.status, 400, "{}", res.text());
    let res = server.post("/plans", &json!({}));
    assert_eq!(res.status, 400, "{}", res.text());
}

#[test]
fn spec_4_1_rejects_invalid_name() {
    let server = Server::start();
    for name in [json!(""), json!("   "), json!("a".repeat(101)), json!(5), json!(null), json!(false)] {
        let res = server.post("/plans", &json!({"name": name}));
        assert_eq!(res.status, 400, "name {name}: {}", res.text());
    }
}

// ---- 4.2 ----

#[test]
fn spec_4_2_lists_plans_matching_example() {
    let server = Server::start();
    let plan = server.create_plan(&json!({"name": "Household"}));
    let list = plans(&server);
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["id"], plan["id"]);
    assert_eq!(list[0]["name"], "Household");
    assert_eq!(list[0]["currency_code"], "USD");
}

#[test]
fn spec_4_2_lists_oldest_first() {
    let server = Server::start();
    let names = ["Zeta", "Alpha", "Mid", "alpha", "Beta", "Zeta"];
    let created: Vec<String> = names.iter().map(|n| server.plan_id(n)).collect();
    let list = plans(&server);
    assert_eq!(ids(&list), created, "list is not oldest first");
    let listed_names: Vec<&str> = list.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(listed_names, names);
}

#[test]
fn spec_4_2_list_keeps_order_after_rename_and_delete() {
    let server = Server::start();
    let created: Vec<String> = (0..5).map(|i| server.plan_id(&format!("Plan {i}"))).collect();
    let res = server.patch(&format!("/plans/{}", created[0]), &json!({"name": "ZZZ renamed"}));
    assert_eq!(res.status, 200);
    assert_eq!(server.delete(&format!("/plans/{}", created[2])).status, 204);
    let newest = server.plan_id("Newest");
    let expected = vec![
        created[0].as_str(),
        created[1].as_str(),
        created[3].as_str(),
        created[4].as_str(),
        newest.as_str(),
    ];
    assert_eq!(ids(&plans(&server)), expected);
}

#[test]
fn spec_4_2_gets_one_plan() {
    let server = Server::start();
    let created = server.create_plan(&json!({"name": "Household", "currency_code": "CAD"}));
    server.plan_id("Other");
    let id = created["id"].as_str().unwrap();
    let res = server.get(&format!("/plans/{id}"));
    assert_eq!(res.status, 200, "{}", res.text());
    let plan = res.json();
    assert_eq!(plan["id"], id);
    assert_eq!(plan["name"], "Household");
    assert_eq!(plan["currency_code"], "CAD");
}

#[test]
fn spec_4_2_get_missing_plan_returns_404() {
    let server = Server::start();
    server.plan_id("Household");
    for id in ["no-such-plan", "PLAN_ID", "%20"] {
        let res = server.get(&format!("/plans/{id}"));
        assert_eq!(res.status, 404, "GET /plans/{id}: {}", res.text());
    }
}

// ---- 4.3 ----

#[test]
fn spec_4_3_patches_currency_code() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let res = server.patch(&format!("/plans/{id}"), &json!({"currency_code": "EUR"}));
    assert_eq!(res.status, 200, "{}", res.text());
    let plan = res.json();
    assert_eq!(plan["id"], id.as_str());
    assert_eq!(plan["name"], "Household");
    assert_eq!(plan["currency_code"], "EUR");
    assert_eq!(server.get(&format!("/plans/{id}")).json()["currency_code"], "EUR");
    assert_eq!(plans(&server)[0]["currency_code"], "EUR");
}

#[test]
fn spec_4_3_patches_name() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": "Home"}));
    assert_eq!(res.status, 200, "{}", res.text());
    let plan = res.json();
    assert_eq!(plan["id"], id.as_str());
    assert_eq!(plan["name"], "Home");
    assert_eq!(plan["currency_code"], "USD");
    assert_eq!(server.get(&format!("/plans/{id}")).json()["name"], "Home");
    assert_eq!(plans(&server)[0]["name"], "Home");
}

#[test]
fn spec_4_3_patches_both_fields() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": "Ferien", "currency_code": "CHF"}));
    assert_eq!(res.status, 200, "{}", res.text());
    let plan = server.get(&format!("/plans/{id}")).json();
    assert_eq!(plan["name"], "Ferien");
    assert_eq!(plan["currency_code"], "CHF");
}

#[test]
fn spec_4_3_patch_affects_only_that_plan() {
    let server = Server::start();
    let a = server.plan_id("A");
    let b = server.plan_id("B");
    server.patch(&format!("/plans/{a}"), &json!({"name": "A2", "currency_code": "EUR"}));
    let other = server.get(&format!("/plans/{b}")).json();
    assert_eq!(other["name"], "B");
    assert_eq!(other["currency_code"], "USD");
}

#[test]
fn spec_4_3_rejects_invalid_values() {
    let server = Server::start();
    let id = server.plan_id("Household");
    for body in [
        json!({"currency_code": "eur"}),
        json!({"currency_code": "EURO"}),
        json!({"currency_code": ""}),
        json!({"currency_code": 978}),
        json!({"name": ""}),
        json!({"name": " "}),
        json!({"name": "a".repeat(101)}),
        json!({"name": 1}),
    ] {
        let res = server.patch(&format!("/plans/{id}"), &body);
        assert_eq!(res.status, 400, "PATCH {body}: {}", res.text());
    }
    let plan = server.get(&format!("/plans/{id}")).json();
    assert_eq!(plan["name"], "Household");
    assert_eq!(plan["currency_code"], "USD");
}

#[test]
fn spec_4_3_patch_missing_plan_returns_404() {
    let server = Server::start();
    server.plan_id("Household");
    let res = server.patch("/plans/no-such-plan", &json!({"name": "x"}));
    assert_eq!(res.status, 404, "{}", res.text());
}

// ---- 4.4 ----

#[test]
fn spec_4_4_deletes_plan() {
    let server = Server::start();
    let id = server.plan_id("Household");
    let keep = server.plan_id("Keep");
    let res = server.delete(&format!("/plans/{id}"));
    assert_eq!(res.status, 204, "{}", res.text());
    assert_eq!(server.get(&format!("/plans/{id}")).status, 404, "GET after delete");
    assert_eq!(ids(&plans(&server)), vec![keep.as_str()], "list after delete");
    assert_eq!(server.get(&format!("/plans/{keep}")).status, 200, "other plan after delete");
}

#[test]
fn spec_4_4_deleted_plan_is_gone_for_every_method() {
    let server = Server::start();
    let id = server.plan_id("Household");
    assert_eq!(server.delete(&format!("/plans/{id}")).status, 204);
    assert_eq!(server.delete(&format!("/plans/{id}")).status, 404, "second DELETE");
    let res = server.patch(&format!("/plans/{id}"), &json!({"name": "Back"}));
    assert_eq!(res.status, 404, "PATCH after delete");
    assert!(plans(&server).is_empty(), "PATCH after delete resurrected the plan");
}

#[test]
fn spec_4_4_delete_missing_plan_returns_404() {
    let server = Server::start();
    server.plan_id("Household");
    let res = server.delete("/plans/no-such-plan");
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(plans(&server).len(), 1);
}
