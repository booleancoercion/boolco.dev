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
use log::info;
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[derive(Default, Debug)]
struct AppState {
    visitors: AtomicU64,
    messages: Mutex<VecDeque<(String, String, IpAddr)>>,
}

#[derive(Serialize, Deserialize, Default)]
struct NonSyncAppState {
    visitors: u64,
    messages: VecDeque<(String, String, IpAddr)>,
}

#[derive(Debug)]
struct AppData {
    state: AppState,
    dictionary: &'static [&'static str],
    db: Db,
}

const STATE_FILE: &str = "state.json";
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

    let dict_file = BufReader::new(File::open("res/words_alpha.txt").await?);
    let mut dictionary = vec![];

    let mut lines = dict_file.lines();
    while let Some(line) = lines.next_line().await.unwrap() {
        if line.len() < 3 {
            continue;
        }
        dictionary.push(&*Box::leak(line.into_boxed_str()));
    }

    let pepper = tokio::fs::read(PEPPER_FILE).await.unwrap();

    let data = Data::new(AppData {
        state: load_state().await,
        dictionary: dictionary.leak(),
        db: Db::new(DATABASE_FILE, pepper.leak()).await,
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
        log::info!("starting HTTP server at http://localhost:8080");
        server.bind("localhost:8080")?
    } else {
        let config = ssl::load_rustls_config();

        log::info!("starting HTTPS server at https://[::]:443");
        server.bind_rustls("[::]:443", config)?
    }
    .run()
    .await;

    save_state(Arc::try_unwrap(data.into_inner()).unwrap().state).await;
    Ok(())
}

async fn load_state() -> AppState {
    let maybe_state = tokio::task::spawn_blocking(|| {
        use std::fs::File;
        use std::io::BufReader;

        File::open(STATE_FILE)
            .ok()
            .map(BufReader::new)
            .map(serde_json::from_reader::<_, NonSyncAppState>)
    })
    .await
    .unwrap();

    match maybe_state.and_then(Result::ok).map(Into::into) {
        Some(state) => {
            info!("Found and loaded state.");
            state
        }
        None => {
            info!("Couldn't load state - resetting to default.");
            Default::default()
        }
    }
}

async fn save_state(state: AppState) {
    let state: NonSyncAppState = state.into();

    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(STATE_FILE)
            .unwrap();

        serde_json::to_writer_pretty(&mut file, &state).unwrap();
    })
    .await
    .unwrap();

    info!("Successfully saved state.")
}

impl From<AppState> for NonSyncAppState {
    fn from(state: AppState) -> Self {
        let AppState { visitors, messages } = state;

        Self {
            visitors: visitors.into_inner(),
            messages: messages.into_inner(),
        }
    }
}

impl From<NonSyncAppState> for AppState {
    fn from(state: NonSyncAppState) -> Self {
        let NonSyncAppState { visitors, messages } = state;

        Self {
            visitors: visitors.into(),
            messages: Mutex::new(messages),
        }
    }
}
