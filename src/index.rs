use actix_web::http::{header::ContentType, StatusCode};
use actix_web::{get, web::Data, HttpResponseBuilder, Responder};
use askama::Template;

use std::sync::atomic::Ordering;

#[derive(Template)]
#[template(path = "../templates/index.html")]
struct IndexTemplate {
    visitors: u64,
}

#[get("/")]
async fn index(data: Data<super::AppState>) -> impl Responder {
    let visitors = data.visitors.fetch_add(1, Ordering::SeqCst) + 1;

    HttpResponseBuilder::new(StatusCode::OK)
        .content_type(ContentType::html())
        .body(IndexTemplate { visitors }.to_string())
}
