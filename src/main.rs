#![cfg_attr(feature = "prepare_db", allow(unused))]

mod db;

const DATABASE_FILE: &str = "data.sqlite";
const PEPPER_FILE: &str = "pepper";
#[cfg(feature = "prepare_db")]
const SCHEMA_FILE: &str = "schema.sqlite";

#[cfg(feature = "prepare_db")]
#[actix_web::main]
async fn main() {
    let _ = tokio::fs::remove_file(SCHEMA_FILE).await;
    let _ = tokio::fs::remove_file(DATABASE_FILE).await;
    db::prepare_db(SCHEMA_FILE).await;
}

#[cfg(not(feature = "prepare_db"))]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    inner::main().await
}

#[cfg(not(feature = "prepare_db"))]
pub use inner::*;

#[cfg(not(feature = "prepare_db"))]
#[path = ""]
mod inner {
    pub mod auth;
    pub mod discord_name;
    pub mod game;
    pub mod index;
    pub mod og;
    pub mod ssl;

    use crate::db::Db;
    use actix_files::{Files, NamedFile};
    use actix_web::web;
    use actix_web::{middleware, web::Data, App, HttpServer};
    use game::GameMessage;
    use log::info;
    use serde::{Deserialize, Serialize};
    use tokio::fs::File;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::Mutex;

    use std::collections::VecDeque;
    use std::sync::atomic::AtomicI64;
    use std::sync::Arc;

    #[derive(Serialize, Deserialize)]
    struct Config {
        bind_addr: Option<String>,
        workers: Option<usize>,
        ssl: Option<SslConfig>,
    }

    const fn bool_as_true() -> bool {
        true
    }

    #[derive(Serialize, Deserialize)]
    struct SslConfig {
        certificate: String,
        key: String,
        #[serde(default = "bool_as_true")]
        enabled: bool,
    }

    #[derive(Default, Debug)]
    struct AppState {
        visitors: AtomicI64,
        messages: Mutex<VecDeque<GameMessage>>,
    }

    #[derive(Debug)]
    pub struct AppData {
        state: AppState,
        dictionary: &'static [&'static str],
        db: Db,
    }

    pub async fn main() -> std::io::Result<()> {
        env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

        let config: Config = toml::from_str(
            &tokio::fs::read_to_string("config.toml")
                .await
                .expect("couldn't open config.toml"),
        )
        .expect("invalid config.toml format");

        let dict_file = BufReader::new(
            File::open("res/words_alpha.txt")
                .await
                .expect("dictionary file couldn't be opened"),
        );
        let mut dictionary = vec![];

        let mut lines = dict_file.lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            if line.len() < 3 {
                continue;
            }
            dictionary.push(&*Box::leak(line.into_boxed_str()));
        }

        let pepper = tokio::fs::read(crate::PEPPER_FILE)
            .await
            .expect("pepper file couldn't be opened");
        let db = Db::new(crate::DATABASE_FILE, pepper.leak()).await;

        let data = Data::new(AppData {
            state: load_state(&db).await,
            dictionary: dictionary.leak(),
            db,
        });

        let server = {
            let data = data.clone();
            let mut server = HttpServer::new(move || {
                App::new()
                    .app_data(data.clone())
                    // enable logger
                    .wrap(middleware::Logger::default())
                    // register simple handler, handle all methods
                    .service(index::index)
                    .service(game::game_get)
                    .service(game::game_post)
                    .service(og::og)
                    .service(discord_name::site)
                    .service(discord_name::api)
                    .service(auth::login_get)
                    .service(auth::login_post)
                    .service(auth::register_get)
                    .service(auth::register_post)
                    .service(Files::new("/static", "static").show_files_listing())
                    .route(
                        "/favicon.ico",
                        web::get().to(|| async { NamedFile::open_async("res/favicon.ico").await }),
                    )
            })
            .shutdown_timeout(10);

            if let Some(workers) = config.workers {
                server = server.workers(workers);
            }

            info!("starting HTTP server at http://localhost:8080");
            server.bind("localhost:8080")?
        };

        let server = if let Some(bind_addr) = &config.bind_addr {
            if let Some(SslConfig {
                certificate,
                key,
                enabled: true,
            }) = config.ssl
            {
                let ssl_config = ssl::load_rustls_config(&certificate, &key);

                info!("starting HTTPS server at https://{}", bind_addr);
                server.bind_rustls(bind_addr, ssl_config)?
            } else {
                info!("starting HTTP server at http://{}", bind_addr);
                server.bind(bind_addr)?
            }
        } else {
            server
        };

        let _ = server.run().await;

        let data = Arc::try_unwrap(data.into_inner()).unwrap();
        save_state(data.state, &data.db).await;
        Ok(())
    }

    async fn load_state(db: &Db) -> AppState {
        let visitors = db.get_visitors().await;
        let messages = db.get_messages().await;

        AppState {
            visitors: AtomicI64::new(visitors),
            messages: Mutex::new(messages.into()),
        }
    }

    async fn save_state(state: AppState, db: &Db) {
        let AppState { visitors, messages } = state;
        db.set_visitors(visitors.into_inner()).await;
        db.set_messages(messages.into_inner().make_contiguous())
            .await;
    }
}
