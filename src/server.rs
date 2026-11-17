//! Cached HTTP snapshots, freshness-aware readiness and graceful termination.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use bytes::Bytes;
use tokio::sync::watch;

use crate::{
    collector::Collection,
    metrics::{self, Labels},
};

/// Immutable payloads shared between the sampler and HTTP handlers.
pub struct Snapshot {
    metrics: Bytes,
    json: Bytes,
    sampled_at: Option<Instant>,
    success: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            metrics: metrics::failed(),
            json: Bytes::from_static(b"{\"error\":\"collection unavailable\"}"),
            sampled_at: None,
            success: false,
        }
    }
}

impl Snapshot {
    /// Encode both representations once, independently of the number of scrapers.
    ///
    /// # Errors
    /// Returns an error if a collection cannot be serialized as JSON.
    pub fn new(collection: &Collection, labels: &Labels) -> Result<Self, serde_json::Error> {
        Ok(Self {
            metrics: metrics::encode(collection, labels),
            json: Bytes::from(serde_json::to_vec(collection)?),
            sampled_at: Some(Instant::now()),
            success: collection.errors == 0,
        })
    }
}

/// Construct the HTTP service over an atomic watch-channel snapshot.
pub fn router(receiver: watch::Receiver<Arc<Snapshot>>, stale_after: Duration) -> Router {
    Router::new()
        .route("/", get(|| async { axum::response::Html("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>cadvisor-rs</title><h1>cadvisor-rs</h1><p>Linux container metrics.</p><ul><li><a href=\"/metrics\">Prometheus metrics</a></li><li><a href=\"/api/v1/containers\">Current JSON snapshot</a></li><li><a href=\"/readyz\">Readiness</a></li></ul></html>") }))
        .route("/metrics", get(scrape))
        .route("/api/v1/containers", get(containers))
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/readyz", get(ready))
        .with_state(AppState { receiver, stale_after })
}

#[derive(Clone)]
struct AppState {
    receiver: watch::Receiver<Arc<Snapshot>>,
    stale_after: Duration,
}

async fn scrape(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.receiver.borrow().clone();
    (
        [
            (header::CONTENT_TYPE, metrics::CONTENT_TYPE),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from(snapshot.metrics.clone()),
    )
}

async fn containers(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.receiver.borrow().clone();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from(snapshot.json.clone()),
    )
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.receiver.borrow().clone();
    if snapshot.success
        && snapshot
            .sampled_at
            .is_some_and(|at| at.elapsed() <= state.stale_after)
    {
        (StatusCode::OK, "ready\n")
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "no fresh successful collection\n",
        )
    }
}
