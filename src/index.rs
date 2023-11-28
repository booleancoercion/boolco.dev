use actix_session::Session;
use actix_web::{get, web::Data, Responder};
use askama::Template;

use std::sync::atomic::Ordering;

use crate::auth::{session_keys, LoggedInUser};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    visitors: i64,
    successful: Option<String>,
    logged_in: Option<String>,
}

#[get("/")]
async fn index(data: Data<crate::AppData>, session: Session) -> impl Responder {
    let visitors = data.state.visitors.fetch_add(1, Ordering::SeqCst) + 1;

    let successful = if let Ok(Some(what)) = session.get::<String>(session_keys::SUCCESSFUL) {
        session.remove(session_keys::SUCCESSFUL);
        Some(what)
    } else {
        None
    };

    let logged_in =
        if let Ok(Some(logged_in)) = session.get::<LoggedInUser>(session_keys::LOGGED_IN) {
            let name = data.db.get_username(logged_in.id).await;

            if let Some(name) = name {
                session.renew();
                Some(name)
            } else {
                session.remove(session_keys::LOGGED_IN);
                None
            }
        } else {
            None
        };

    IndexTemplate {
        visitors,
        successful,
        logged_in,
    }
}
