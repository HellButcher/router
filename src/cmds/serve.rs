use axum::Router;
use router_service::{Service, ServiceOptions};
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, signal};
use tower::ServiceBuilder;
use tower_http::{compression::CompressionLayer, services::ServeDir, trace::TraceLayer};

use crate::args::serve::*;

pub async fn serve(args: &ServeArgs) -> anyhow::Result<()> {
    let addr: SocketAddr = args.listen.parse()?;

    let service = Arc::new(Service::open(ServiceOptions {
        storage_dir: args.storage_dir.clone(),
        ..Default::default()
    })?);

    let app = Router::new()
        .nest("/api/v1/", router_server::make_service_router(service))
        .route(
            "/openapi.json",
            axum::routing::get(async || {
                axum::response::Json(router_server::openapi::get_openapi("/api/v1/"))
            }),
        )
        .fallback_service(ServeDir::new("frontend/dist"))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new()),
        );

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}
