use std::net::IpAddr;

use actix_files::NamedFile;
use actix_web::http::StatusCode;
use actix_web::web::{Data, Form};
use actix_web::{get, post, HttpRequest, Responder};
use askama::Template;
use log::info;
use serde::{Deserialize, Serialize};

const MESSAGE_LIMIT: usize = 10;
const MESSAGE_MAX_LENGTH: usize = 1000;

#[derive(Debug, Clone)]
pub struct GameMessage {
    pub name: String,
    pub content: String,
    pub ip: IpAddr,
}

#[derive(Template)]
#[template(path = "game.html")]
struct GameTemplate {
    messages: Vec<GameMessage>,
}

#[get("/game")]
async fn game_get(data: Data<crate::AppData>) -> impl Responder {
    let messages = data.state.messages.lock().await;
    let messages = messages.iter().cloned().collect();

    GameTemplate { messages }
}

#[derive(Serialize, Deserialize)]
struct GameParams {
    user_name: String,
    user_message: String,
}

#[post("/game")]
async fn game_post(
    req: HttpRequest,
    data: Data<crate::AppData>,
    form: Form<GameParams>,
) -> impl Responder {
    let form = form.into_inner();
    let ip = req.peer_addr().unwrap().ip();

    let name = form.user_name.trim();
    let msg = form.user_message.trim();

    info!("{ip} answered the game form with {name}: {msg}");

    let valid_range = 1usize..=MESSAGE_MAX_LENGTH; // bytes

    if !(valid_range.contains(&name.len()) && valid_range.contains(&msg.len())) {
        return NamedFile::open_async("res/game_bad_message.html")
            .await
            .unwrap()
            .customize()
            .with_status(StatusCode::BAD_REQUEST);
    }

    {
        let mut messages = data.state.messages.lock().await;

        if !ip.is_loopback()
            && messages
                .iter()
                .any(|GameMessage { ip: msg_ip, .. }| msg_ip == &ip)
        {
            return NamedFile::open_async("res/game_greedy.html")
                .await
                .unwrap()
                .customize()
                .with_status(StatusCode::TOO_MANY_REQUESTS);
        }

        while messages.len() + 1 > MESSAGE_LIMIT {
            messages.pop_front();
        }

        messages.push_back(GameMessage {
            name: name.into(),
            content: msg.into(),
            ip,
        })
    }

    NamedFile::open_async("res/game_success.html")
        .await
        .unwrap()
        .customize() // otherwise return type doesn't match
}
