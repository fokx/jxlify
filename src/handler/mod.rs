pub mod healthz;
pub mod image;
pub mod remote;

use axum::routing::get;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub use image::AppState;

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/healthz", get(healthz::healthz_handler))
        .fallback(image::image_handler)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
