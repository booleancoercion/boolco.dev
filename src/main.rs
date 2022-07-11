use std::{fs::File, io::BufReader};

use actix_files::{Files, NamedFile};
use actix_web::{middleware, web, App, HttpRequest, HttpServer, Responder};
use rustls::{Certificate, PrivateKey, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};

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
        let config = load_rustls_config();

        log::info!("starting HTTPS server at http://0.0.0.0:443");
        server.bind_rustls(("0.0.0.0", 443), config)?
    }
    .run()
    .await
}

fn load_rustls_config() -> rustls::ServerConfig {
    // init server config builder with safe defaults
    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth();

    // load TLS key/cert files
    let cert_file = &mut BufReader::new(File::open("ssl/boolco_dev.crt").unwrap());
    let key_file = &mut BufReader::new(File::open("ssl/boolco_dev.key").unwrap());

    // convert files to key/cert objects
    let cert_chain = certs(cert_file)
        .unwrap()
        .into_iter()
        .map(Certificate)
        .collect();
    let mut keys: Vec<PrivateKey> = pkcs8_private_keys(key_file)
        .unwrap()
        .into_iter()
        .map(PrivateKey)
        .collect();

    // exit if no keys could be parsed
    if keys.is_empty() {
        eprintln!("Could not locate PKCS 8 private keys.");
        std::process::exit(1);
    }

    config.with_single_cert(cert_chain, keys.remove(0)).unwrap()
}
