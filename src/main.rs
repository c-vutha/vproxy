use std::{env, net::SocketAddr};

use axum::{
    extract::State,
    http::{Request, Uri},
    response::Response,
    routing::any,
    Router,
};
use hyper::{client::HttpConnector, Body};
use tokio::signal;
use tokio::task::JoinSet;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

type Client = hyper::client::Client<HttpConnector, Body>;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or("tower_http=trace,info".into()),
        )
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .compact()
        .init();

    let vproxy_vars: Vec<(String, String)> = env::vars()
        .filter(|(key, _)| key.starts_with("VPROXY_"))
        .collect();

    let mut set = JoinSet::new();

    for (config, url) in vproxy_vars {
        let port: u16 = config
            .rsplit("_")
            .next()
            .expect("Invalid configuration")
            .parse()
            .expect("Invalid port number");
        set.spawn(create_server(port, url.clone()));
    }

    while let Some(_) = set.join_next().await {}
}

#[derive(Clone)]
struct AppState {
    client: Client,
    url: String,
}

async fn create_server(port: u16, url: String) {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Proxy http://{} => {}", addr, &url);

    let client = Client::new();
    let shared_state = AppState { client, url };

    let app = Router::new()
        .route("/", any(handler))
        .route("/*path", any(handler))
        .layer(TraceLayer::new_for_http().on_request(()))
        .layer(CorsLayer::permissive())
        .with_state(shared_state);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap()
}

async fn handler(State(state): State<AppState>, mut req: Request<Body>) -> Response<Body> {
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(&path);

    let uri = format!("{}{}", state.url, path_query);
    *req.uri_mut() = Uri::try_from(uri).unwrap();

    state.client.request(req).await.unwrap()
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

    println!("Shuting down...");
}
