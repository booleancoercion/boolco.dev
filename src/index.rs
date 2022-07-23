use actix_web::{get, web::Data, Responder};
use askama::Template;

use std::sync::atomic::Ordering;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    visitors: u64,
}

#[get("/")]
async fn index(data: Data<super::AppState>) -> impl Responder {
    let visitors = data.visitors.fetch_add(1, Ordering::SeqCst) + 1;

    IndexTemplate { visitors }
}
