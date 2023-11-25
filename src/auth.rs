use actix_files::NamedFile;
use actix_web::web::{self, Data};
use actix_web::{get, post, HttpRequest, Responder};
use serde::{Deserialize, Serialize};

#[get("/login")]
async fn login_get() -> impl Responder {
    NamedFile::open_async("res/login_form.html").await
}

#[get("/register")]
async fn register_get() -> impl Responder {
    NamedFile::open_async("res/register_form.html").await
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

    if !password.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
        return false;
    }

    true
}

#[post("/login")]
async fn login_post(data: Data<crate::AppData>, form: web::Form<LoginForm>) -> impl Responder {
    if verify_user_password(&form.username, &form.password)
        && data.db.verify_user(&form.username, &form.password).await
    {
        "success"
    } else {
        "failure"
    }
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
) -> impl Responder {
    if let Some(form) = form {
        if verify_user_password(&form.username, &form.password)
            && data
                .db
                .insert_user(&form.ticket, &form.username, &form.password)
                .await
        {
            return "success";
        }
    } else if req.peer_addr().unwrap().ip().is_loopback() {
        return &*data.db.generate_registration_ticket().await.leak();
    }

    "failure"
}
