use std::cmp::Reverse;

use actix_files::NamedFile;
use actix_web::{
    get,
    web::{self, Data, Json},
    Responder,
};
use serde::Deserialize;

#[get("/discord_name")]
async fn site() -> impl Responder {
    NamedFile::open_async("res/discord_name.html")
        .await
        .unwrap()
}

#[derive(Deserialize)]
struct Names {
    username: String,
    nickname: Option<String>,
}

#[get("/api/v1/discord_name")]
async fn api(data: Data<super::AppData>, names: web::Query<Names>) -> impl Responder {
    let Names { username, nickname } = names.0;
    let matches = tokio::task::spawn_blocking(move || {
        let mut matches = poingus::get_matches(&username, nickname.as_deref(), data.dictionary);
        matches.sort_unstable_by_key(|s| Reverse(s.len()));

        matches
    })
    .await
    .unwrap();

    Json(matches)
}
