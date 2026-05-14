use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use super::channel::WebhookChannel;
use super::handler::{health_handler, webhook_handler};

/// 创建路由
pub fn create_routes(channel: Arc<WebhookChannel>) -> Router {
    Router::new()
        .route("/webhook", post(webhook_handler))
        .route("/health", get(health_handler))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        .layer(CorsLayer::permissive())
        .with_state(channel)
}
