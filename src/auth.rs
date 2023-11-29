use actix_session::Session;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::web::{self, Data, ReqData};
use actix_web::{get, post, HttpRequest, HttpResponseBuilder, Responder};
use askama::Template;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::Login;

pub mod session_keys {
    pub const LOGGED_IN: &str = "logged_in";
    pub const SUCCESSFUL: &str = "successful";
}

pub mod middleware;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    failed: bool,
}

#[derive(Template)]
#[template(path = "register.html")]
struct RegisterTemplate {
    failed: bool,
}

#[get("/login")]
async fn login_get(login: ReqData<Login>) -> impl Responder {
    if login.info().is_some() {
        HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", "/"))
            .finish()
    } else {
        HttpResponseBuilder::new(StatusCode::OK)
            .content_type(ContentType::html())
            .body(LoginTemplate { failed: false }.to_string())
    }
}

#[get("/register")]
async fn register_get(login: ReqData<Login>) -> impl Responder {
    if login.info().is_some() {
        HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", "/"))
            .finish()
    } else {
        HttpResponseBuilder::new(StatusCode::OK)
            .content_type(ContentType::html())
            .body(RegisterTemplate { failed: false }.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

fn verify_username(username: &str) -> bool {
    (1..=64).contains(&username.len())
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn verify_password(password: &str) -> bool {
    (8..=64).contains(&password.len())
}

#[post("/login")]
async fn login_post(
    data: Data<crate::AppData>,
    form: web::Form<LoginForm>,
    session: Session,
    login: ReqData<Login>,
) -> impl Responder {
    if login.info().is_some() {
        return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", "/"))
            .finish();
    }

    if verify_username(&form.username) && verify_password(&form.password) {
        if let Some(id) = data.db.verify_user(&form.username, &form.password).await {
            session
                .insert(session_keys::SUCCESSFUL, "logged in")
                .unwrap();
            login.login(id);

            return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
                .insert_header(("Location", "/"))
                .finish();
        }
    }

    HttpResponseBuilder::new(StatusCode::FORBIDDEN)
        .content_type(ContentType::html())
        .body(LoginTemplate { failed: true }.to_string())
}

#[derive(Serialize, Deserialize)]
struct RegisterForm {
    password: String,
    ticket: String,
}

#[derive(Serialize, Deserialize)]
struct RegisterQuery {
    name: String,
}

#[post("/register")]
async fn register_post(
    req: HttpRequest,
    data: Data<crate::AppData>,
    form: Option<web::Form<RegisterForm>>,
    query: Option<web::Query<RegisterQuery>>,
    session: Session,
    login: ReqData<Login>,
) -> impl Responder {
    if login.info().is_some() {
        return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", "/"))
            .finish();
    }

    if let Some(form) = form {
        if verify_password(&form.password) && form.ticket.len() <= 512 {
            if let Some(id) = data.db.register_user(&form.ticket, &form.password).await {
                session
                    .insert(session_keys::SUCCESSFUL, "registered")
                    .unwrap();
                login.login(id);

                return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
                    .insert_header(("Location", "/"))
                    .finish();
            }
        }
    } else if req.peer_addr().unwrap().ip().is_loopback() {
        if let Some(query) = query {
            if let Some(ticket) = data.db.generate_registration_ticket(&query.name).await {
                return HttpResponseBuilder::new(StatusCode::OK).body(ticket);
            }
        }

        return HttpResponseBuilder::new(StatusCode::BAD_REQUEST).finish();
    }

    HttpResponseBuilder::new(StatusCode::FORBIDDEN)
        .content_type(ContentType::html())
        .body(RegisterTemplate { failed: true }.to_string())
}

#[post("/logout")]
async fn logout(session: Session, login: ReqData<Login>) -> impl Responder {
    if login.logout() {
        session
            .insert(session_keys::SUCCESSFUL, "logged out")
            .unwrap();
    }

    HttpResponseBuilder::new(StatusCode::SEE_OTHER)
        .insert_header(("Location", "/"))
        .finish()
}
