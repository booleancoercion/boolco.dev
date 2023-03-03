use actix_files::NamedFile;
use actix_web::{get, http::header, web, HttpRequest, HttpResponse};
use askama::Template;
use askama_actix::TemplateToResponse;
use serde::{Deserialize, Serialize};

macro_rules! og_struct {
    ($sname:ident, $($name:ident),*) => {
        #[derive(Serialize, Deserialize, Template)]
        #[template(path = "og.html")]
        struct $sname {
            $(
                $name: Option<String>,
            )*
        }

        impl $sname {
            fn all_nones(&self) -> bool {
                [$(
                    self.$name.is_none()
                ),*].into_iter().all(|x| x)
            }

            /// Converts all empty fields to `None`
            fn emptyize(&mut self) -> bool {
                let mut modified = false;
                $(
                    if let Some(val) = &self.$name {
                        if val.is_empty() {
                            self.$name = None;
                            modified = true;
                        }
                    }
                )*
                modified
            }
        }
    };
}

og_struct!(Og, title, r#type, url, image, description);

#[get("/og")]
async fn og(req: HttpRequest, og: web::Query<Og>) -> HttpResponse {
    let mut og = og.into_inner();
    if og.emptyize() {
        return HttpResponse::MovedPermanently()
            .insert_header((
                header::LOCATION,
                format!("/og?{}", serde_urlencoded::to_string(og).unwrap()),
            ))
            .finish();
    }

    if og.all_nones() {
        NamedFile::open_async("res/og_empty.html")
            .await
            .unwrap()
            .into_response(&req)
    } else {
        og.to_response()
    }
}
