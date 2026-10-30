use mybudget_testsuite::Server;

/// SPEC 2.2
#[test]
fn unknown_path_returns_404() {
    let server = Server::start();
    let res = server.client().get(server.url("/no-such-route")).send().unwrap();
    assert_eq!(res.status(), 404);
}
