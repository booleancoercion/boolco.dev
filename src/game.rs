use std::net::IpAddr;

use actix_files::NamedFile;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::web::{Data, Form};
use actix_web::{get, post, HttpRequest, HttpResponse, Responder};
use askama::Template;
use log::info;
use serde::{Deserialize, Serialize};

const MESSAGE_LIMIT: usize = 10;

#[derive(Template)]
#[template(path = "../templates/game.html")]
struct GameTemplate<'a> {
    messages: Vec<&'a (String, String, IpAddr)>,
}

#[get("/game")]
async fn game_get(data: Data<super::AppState>) -> impl Responder {
    let body = {
        let messages = data.messages.lock().await;
        let messages = messages.iter().collect();

        GameTemplate { messages }.to_string()
    };

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(body)
}

#[derive(Serialize, Deserialize)]
struct GameParams {
    user_name: String,
    user_message: String,
}

#[post("/game")]
async fn game_post(
    req: HttpRequest,
    data: Data<super::AppState>,
    form: Form<GameParams>,
) -> impl Responder {
    let form = form.into_inner();
    let ip = req.peer_addr().unwrap().ip();

    let name = form.user_name.trim();
    let msg = form.user_message.trim();

    info!("{ip} answered the game form with {name}: {msg}");

    if name.is_empty() || msg.is_empty() {
        return NamedFile::open_async("static/game_bad_message.html")
            .await
            .unwrap()
            .customize()
            .with_status(StatusCode::BAD_REQUEST);
    }

    {
        let mut messages = data.messages.lock().await;

        if ip != IpAddr::from([127, 0, 0, 1]) && messages.iter().any(|(_, _, msg_ip)| msg_ip == &ip)
        {
            return NamedFile::open_async("static/game_greedy.html")
                .await
                .unwrap()
                .customize()
                .with_status(StatusCode::TOO_MANY_REQUESTS);
        }

        while messages.len() + 1 > MESSAGE_LIMIT {
            messages.pop_front();
        }

        messages.push_back((name.into(), msg.into(), ip))
    }

    NamedFile::open_async("static/game_success.html")
        .await
        .unwrap()
        .customize() // otherwise return type doesn't match
}
