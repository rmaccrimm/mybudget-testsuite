use mybudget_testsuite::Server;

#[test]
fn spec_2_1_health_returns_200_ok() {
    let server = Server::start();
    let res = server.client().get(server.url("/health")).send().unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "ok");
}

#[test]
fn spec_2_2_unknown_path_returns_404() {
    let server = Server::start();
    let res = server.client().get(server.url("/no-such-path")).send().unwrap();
    assert_eq!(res.status(), 404);
}
