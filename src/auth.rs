use actix_session::Session;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::web::{self, Data};
use actix_web::{get, post, HttpRequest, HttpResponseBuilder, Responder};
use askama::Template;
use serde::{Deserialize, Serialize};

pub mod session_keys {
    pub const LOGGED_IN: &str = "logged_in";
    pub const SUCCESSFUL: &str = "successful";
}

#[derive(Serialize, Deserialize)]
pub struct LoggedInUser {
    pub id: i64,
}

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
async fn login_get(session: Session) -> impl Responder {
    if session
        .get::<LoggedInUser>(session_keys::LOGGED_IN)
        .unwrap()
        .is_some()
    {
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
async fn register_get(session: Session) -> impl Responder {
    if session
        .get::<LoggedInUser>(session_keys::LOGGED_IN)
        .unwrap()
        .is_some()
    {
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

fn verify_user_password(username: &str, password: &str) -> bool {
    if !(1..=64).contains(&username.len()) || !(8..=64).contains(&password.len()) {
        return false;
    }

    if !username
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }

    true
}

#[post("/login")]
async fn login_post(
    data: Data<crate::AppData>,
    form: web::Form<LoginForm>,
    session: Session,
) -> impl Responder {
    if session
        .get::<LoggedInUser>(session_keys::LOGGED_IN)
        .unwrap()
        .is_some()
    {
        return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", "/"))
            .finish();
    }

    if verify_user_password(&form.username, &form.password) {
        if let Some(id) = data.db.verify_user(&form.username, &form.password).await {
            session
                .insert(session_keys::SUCCESSFUL, "logged in")
                .unwrap();
            session
                .insert(session_keys::LOGGED_IN, LoggedInUser { id })
                .unwrap();
            session.renew();

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
    username: String,
    password: String,
    ticket: String,
}

#[post("/register")]
async fn register_post(
    req: HttpRequest,
    data: Data<crate::AppData>,
    form: Option<web::Form<RegisterForm>>,
    session: Session,
) -> impl Responder {
    if session
        .get::<LoggedInUser>(session_keys::LOGGED_IN)
        .unwrap()
        .is_some()
    {
        return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", "/"))
            .finish();
    }

    if let Some(form) = form {
        if verify_user_password(&form.username, &form.password) {
            if let Some(id) = data
                .db
                .register_user(&form.ticket, &form.username, &form.password)
                .await
            {
                session
                    .insert(session_keys::SUCCESSFUL, "registered")
                    .unwrap();
                session
                    .insert(session_keys::LOGGED_IN, LoggedInUser { id })
                    .unwrap();
                session.renew();

                return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
                    .insert_header(("Location", "/"))
                    .finish();
            }
        }
    } else if req.peer_addr().unwrap().ip().is_loopback() {
        return HttpResponseBuilder::new(StatusCode::OK)
            .body(data.db.generate_registration_ticket().await);
    }

    HttpResponseBuilder::new(StatusCode::FORBIDDEN)
        .content_type(ContentType::html())
        .body(RegisterTemplate { failed: true }.to_string())
}

#[post("/logout")]
async fn logout(session: Session) -> impl Responder {
    if session.remove(session_keys::LOGGED_IN).is_some() {
        session
            .insert(session_keys::SUCCESSFUL, "logged out")
            .unwrap();
    }

    HttpResponseBuilder::new(StatusCode::SEE_OTHER)
        .insert_header(("Location", "/"))
        .finish()
}
