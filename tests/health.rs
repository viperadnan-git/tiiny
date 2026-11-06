mod common;

#[tokio::test]
async fn healthz_ok() {
    let app = common::test_app(&[]).await;
    let res = app.server.get("/healthz").await;
    res.assert_status_ok();
    res.assert_text("ok");
}
