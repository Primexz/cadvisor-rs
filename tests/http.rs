//! The HTTP layer serves immutable cached buffers and reports freshness.

mod common;

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use cadvisor_rs::{
    collector::Collector,
    metrics::{CONTENT_TYPE, Labels},
    server::{Snapshot, router},
};
use http_body_util::BodyExt;
use tokio::sync::watch;
use tower::ServiceExt;

#[tokio::test]
async fn endpoints_report_readiness_and_share_the_same_snapshot() {
    let (sender, receiver) = watch::channel(Arc::new(Snapshot::default()));
    let app = router(receiver, Duration::from_secs(30));
    let response = app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let response = app
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let temp = common::fixture();
    common::v2(temp.path(), "", 123);
    let sample = Collector::new(common::config(temp.path()))
        .unwrap()
        .collect(&Default::default(), false)
        .unwrap();
    sender.send_replace(Arc::new(
        Snapshot::new(&sample, &Labels::default()).unwrap(),
    ));
    let response = app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut bodies = Vec::new();
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["content-type"], CONTENT_TYPE);
        bodies.push(response.into_body().collect().await.unwrap().to_bytes());
    }
    assert_eq!(bodies[0], bodies[1]);
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/v1/containers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["containers"][0]["id"], "/");
    let response = app
        .oneshot(Request::get("/unknown").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stale_snapshots_fail_readiness() {
    let temp = common::fixture();
    common::v2(temp.path(), "", 1);
    let sample = Collector::new(common::config(temp.path()))
        .unwrap()
        .collect(&Default::default(), false)
        .unwrap();
    let (_, receiver) = watch::channel(Arc::new(
        Snapshot::new(&sample, &Labels::default()).unwrap(),
    ));
    let app = router(receiver, Duration::ZERO);
    let response = app
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
