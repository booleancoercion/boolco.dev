#![cfg_attr(feature = "prepare_db", allow(unused))]

#[cfg(not(feature = "prepare_db"))]
#[path = ""]
mod reexport_non_db_modules {
    pub mod auth;
    pub mod discord_name;
    pub mod game;
    pub mod index;
    pub mod og;
    pub mod ssl;
}
#[cfg(not(feature = "prepare_db"))]
use reexport_non_db_modules::*;

mod db;

use actix_files::{Files, NamedFile};
use actix_web::web;
use actix_web::{middleware, web::Data, App, HttpServer};
use db::Db;
use game::GameMessage;
use log::info;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

use std::collections::VecDeque;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;

#[derive(Default, Debug)]
struct AppState {
    visitors: AtomicI64,
    messages: Mutex<VecDeque<GameMessage>>,
}

#[derive(Debug)]
struct AppData {
    state: AppState,
    dictionary: &'static [&'static str],
    db: Db,
}

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
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

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

    let pepper = tokio::fs::read(PEPPER_FILE)
        .await
        .expect("pepper file couldn't be opened");
    let db = Db::new(DATABASE_FILE, pepper.leak()).await;

    let data = Data::new(AppData {
        state: load_state(&db).await,
        dictionary: dictionary.leak(),
        db,
    });

    let server = {
        let data = data.clone();
        HttpServer::new(move || {
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
        .shutdown_timeout(10)
    };

    let _ = if cfg!(debug_assertions) {
        info!("starting HTTP server at http://localhost:8080");
        server.bind("localhost:8080")?
    } else {
        let config = ssl::load_rustls_config();

        info!("starting HTTPS server at https://[::]:443");
        server.bind_rustls("[::]:443", config)?
    }
    .run()
    .await;

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
