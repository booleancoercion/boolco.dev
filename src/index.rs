use actix_session::Session;
use actix_web::{
    get,
    web::{Data, ReqData},
    Responder,
};
use askama::Template;

use std::sync::atomic::Ordering;

use crate::auth::{middleware::Login, session_keys};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    visitors: i64,
    successful: Option<String>,
    logged_in: Option<String>,
}

#[get("/")]
async fn index(
    data: Data<crate::AppData>,
    session: Session,
    login: ReqData<Login>,
) -> impl Responder {
    let visitors = data.state.visitors.fetch_add(1, Ordering::SeqCst) + 1;

    let successful = if let Ok(Some(what)) = session.get::<String>(session_keys::SUCCESSFUL) {
        session.remove(session_keys::SUCCESSFUL);
        Some(what)
    } else {
        None
    };

    IndexTemplate {
        visitors,
        successful,
        logged_in: login.info().map(|x| x.name.clone()),
    }
}
