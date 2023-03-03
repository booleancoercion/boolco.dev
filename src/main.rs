mod discord_name;
mod game;
mod index;
mod og;
mod ssl;

use actix_files::{Files, NamedFile};
use actix_web::web;
use actix_web::{middleware, web::Data, App, HttpServer};
use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::BufReader;
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

const STATE_FILE: &str = "state.json";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let state = Data::new(load_state());

    let server = {
        let state = state.clone();
        HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                // enable logger
                .wrap(middleware::Logger::default())
                // register simple handler, handle all methods
                .service(index::index)
                .service(game::game_get)
                .service(game::game_post)
                .service(og::og)
                .service(discord_name::site)
                .service(discord_name::api)
                .service(Files::new("/static", "static").show_files_listing())
                .route(
                    "/favicon.ico",
                    web::get().to(|| async { NamedFile::open_async("res/favicon.ico").await }),
                )
        })
        .workers(4)
        .shutdown_timeout(10)
    };

    let _ = if cfg!(debug_assertions) {
        log::info!("starting HTTP server at http://[::1]:80");
        server.bind("[::1]:80")?
    } else {
        let config = ssl::load_rustls_config();

        log::info!("starting HTTPS server at https://[::]:443");
        server.bind_rustls("[::]:443", config)?
    }
    .run()
    .await;

    save_state(Arc::try_unwrap(state.into_inner()).unwrap());
    Ok(())
}

fn load_state() -> AppState {
    match File::open(STATE_FILE)
        .ok()
        .map(BufReader::new)
        .map(serde_json::from_reader::<_, NonSyncAppState>)
        .and_then(Result::ok)
        .map(Into::into)
    {
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

fn save_state(state: AppState) {
    let state: NonSyncAppState = state.into();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(STATE_FILE)
        .unwrap();

    serde_json::to_writer_pretty(&mut file, &state).unwrap();
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
