mod ssl;

use actix_files::{Files, NamedFile};
use actix_web::{middleware, web, App, HttpRequest, HttpServer, Responder};

/// simple handle
async fn index(_: HttpRequest) -> impl Responder {
    NamedFile::open_async("index.html").await.unwrap()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let server = HttpServer::new(|| {
        App::new()
            // enable logger
            .wrap(middleware::Logger::default())
            // register simple handler, handle all methods
            .service(web::resource("/").to(index))
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
