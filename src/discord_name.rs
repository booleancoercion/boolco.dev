use actix_files::NamedFile;
use actix_web::{get, Responder};

#[get("/discord_name")]
async fn site() -> impl Responder {
    NamedFile::open_async("res/discord_name.html")
        .await
        .unwrap()
}

#[get("/api/v1/discord_name")]
async fn api() -> impl Responder {
    "todo"
}
