use mybudget_testsuite::Server;

/// SPEC 2.1
#[test]
fn health_returns_200_ok() {
    let server = Server::start();
    let res = server.client().get(server.url("/health")).send().unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "ok");
}
