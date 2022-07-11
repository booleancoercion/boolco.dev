mod game;
mod index;
mod ssl;

use actix_files::Files;
use actix_web::{middleware, web::Data, App, HttpServer};
use tokio::sync::Mutex;

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::atomic::AtomicU64;

#[derive(Default)]
struct AppState {
    visitors: AtomicU64,
    messages: Mutex<VecDeque<(String, String, IpAddr)>>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let state = Data::new(AppState::default());

    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            // enable logger
            .wrap(middleware::Logger::default())
            // register simple handler, handle all methods
            .service(index::index)
            .service(game::game_get)
            .service(game::game_post)
            .service(Files::new("/static", "static").show_files_listing())
    });

    if cfg!(debug_assertions) {
        log::info!("starting HTTP server at http://127.0.0.1:80");
        server.bind(("0.0.0.0", 80))?
    } else {
        let config = ssl::load_rustls_config();

        log::info!("starting HTTPS server at http://0.0.0.0:443");
        server.bind_rustls(("0.0.0.0", 443), config)?
    }
    .run()
    .await
}
