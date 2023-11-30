use actix_session::Session;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::web::{self, ReqData};
use actix_web::{get, post, HttpRequest, HttpResponseBuilder, Responder};
use askama::Template;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::middleware::Login;

#[derive(Template)]
#[template(path = "short.html")]
struct ShortTemplate {
    newshort: Option<String>,
    error: Option<String>,
    links: Vec<Link>,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub short: String,
    pub url: String,
    pub id: i64,
}

#[get("/short")]
async fn short_get(
    data: web::Data<crate::AppData>,
    login: ReqData<Login>,
    session: Session,
) -> impl Responder {
    if let Some(info) = login.info() {
        if info.perms.is_short() {
            let newshort = session
                .remove_as::<String>(crate::session_keys::NEW_SHORT)
                .map(Result::unwrap);

            let links = data.db.get_links(info.id).await;

            return HttpResponseBuilder::new(StatusCode::OK)
                .content_type(ContentType::html())
                .body(
                    ShortTemplate {
                        newshort,
                        error: None,
                        links,
                    }
                    .to_string(),
                );
        }
    }

    HttpResponseBuilder::new(StatusCode::NOT_FOUND).finish()
}

#[derive(Serialize, Deserialize)]
struct ShortForm {
    link: String,
    #[serde(deserialize_with = "empty_string_is_none")]
    shortstring: Option<String>,
}

fn empty_string_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(s))
    }
}

fn verify_shortstring(short: &str) -> bool {
    short
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && short.len() >= 2
        && short.len() <= 30
}

#[post("/short")]
async fn short_post(
    data: web::Data<crate::AppData>,
    login: ReqData<Login>,
    form: web::Form<ShortForm>,
    session: Session,
) -> impl Responder {
    if let Some(info) = login.info() {
        if info.perms.is_short() {
            if let Ok(url) = Url::parse(&form.link) {
                let scheme = url.scheme();
                if (scheme == "http" || scheme == "https")
                    && form
                        .shortstring
                        .as_deref()
                        .map(verify_shortstring)
                        .unwrap_or(true)
                {
                    let short = data
                        .db
                        .create_short_link(info.id, &form.link, form.shortstring.as_deref())
                        .await;
                    if let Some(short) = short {
                        session
                            .insert(crate::session_keys::NEW_SHORT, short)
                            .unwrap();

                        return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
                            .insert_header(("Location", "/short"))
                            .finish();
                    }
                }
            }
            return HttpResponseBuilder::new(StatusCode::BAD_REQUEST)
                .content_type(ContentType::html())
                .body(
                    ShortTemplate {
                        newshort: None,
                        error: Some("Invalid request. Please make sure that the URL is valid, its scheme is http/https, \
                        and that your short value is unique.".into()),
                        links: data.db.get_links(info.id).await
                    }.to_string()
                );
        }
    }

    HttpResponseBuilder::new(StatusCode::NOT_FOUND).finish()
}

#[get("/short/{link}")]
async fn short_link(
    req: HttpRequest,
    data: web::Data<crate::AppData>,
    link: web::Path<String>,
) -> impl Responder {
    if let Some(url) = data
        .db
        .get_short_link(&link, req.peer_addr().unwrap().ip())
        .await
    {
        HttpResponseBuilder::new(StatusCode::SEE_OTHER)
            .insert_header(("Location", url))
            .finish()
    } else {
        HttpResponseBuilder::new(StatusCode::NOT_FOUND).finish()
    }
}

#[derive(Deserialize)]
struct DeleteShortForm {
    short: String,
}

#[post("/delete_short")]
async fn delete_short(
    data: web::Data<crate::AppData>,
    login: ReqData<Login>,
    form: web::Form<DeleteShortForm>,
) -> impl Responder {
    if let Some(info) = login.info() {
        if info.perms.is_short()
            && data
                .db
                .delete_if_owns_short_link(info.id, &form.short)
                .await
        {
            return HttpResponseBuilder::new(StatusCode::SEE_OTHER)
                .insert_header(("Location", "/short"))
                .finish();
        }
    }

    HttpResponseBuilder::new(StatusCode::NOT_FOUND).finish()
}
