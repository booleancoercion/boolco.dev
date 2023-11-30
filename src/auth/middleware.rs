use std::cell::RefCell;
use std::future::{self, Ready};
use std::rc::Rc;

use actix_session::SessionExt;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::web::Data;
use actix_web::{Error, HttpMessage};
use futures_util::future::LocalBoxFuture;
use serde::{Deserialize, Serialize};

use crate::db::UserPermissions;

// There are two steps in middleware processing.
// 1. Middleware initialization, middleware factory gets called with
//    next service in chain as parameter.
// 2. Middleware's call method gets called with normal request.
pub struct Auth;

// Middleware factory is `Transform` trait
// `S` - type of the next service
// `B` - type of response's body
impl<S: 'static, B> Transform<S, ServiceRequest> for Auth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        future::ready(Ok(AuthMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthMiddleware<S> {
    // This is special: We need this to avoid lifetime issues.
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let session = req.get_session();
            let data = req.app_data::<Data<crate::AppData>>().unwrap();

            let info = if let Some(logged_in) = session
                .get::<LoggedInUserSessionData>(crate::session_keys::LOGGED_IN)
                .unwrap()
            {
                let name = data.db.get_username(logged_in.id).await;
                if let Some(name) = name {
                    let perms = data.db.get_permissions(logged_in.id).await;
                    session.renew();
                    Some(UserInfo {
                        id: logged_in.id,
                        name,
                        perms: perms.unwrap_or_default(),
                    })
                } else {
                    session.remove(crate::session_keys::LOGGED_IN);
                    None
                }
            } else {
                None
            };

            req.extensions_mut().insert(Login {
                info,
                state: Rc::new(RefCell::new(LoginState::Unchanged)),
            });
            let res = service.call(req).await?;

            let login = res.request().extensions_mut().remove::<Login>().unwrap();
            match *login.state.borrow() {
                LoginState::ToLogin { id } => {
                    session
                        .insert(
                            crate::session_keys::LOGGED_IN,
                            LoggedInUserSessionData { id },
                        )
                        .unwrap();
                    session.renew();
                }
                LoginState::ToLogout => {
                    session.remove(crate::session_keys::LOGGED_IN);
                }
                LoginState::Unchanged => {}
            }

            Ok(res)
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LoggedInUserSessionData {
    id: i64,
}

#[derive(Clone, Debug)]
pub struct Login {
    info: Option<UserInfo>,
    state: Rc<RefCell<LoginState>>,
}

#[derive(Clone, Debug)]
pub struct UserInfo {
    pub id: i64,
    pub name: String,
    pub perms: UserPermissions,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LoginState {
    Unchanged,
    ToLogin { id: i64 },
    ToLogout,
}

impl Login {
    pub fn login(&self, id: i64) {
        *self.state.borrow_mut() = LoginState::ToLogin { id };
    }

    pub fn logout(&self) -> bool {
        *self.state.borrow_mut() = LoginState::ToLogout;
        self.info.is_some()
    }

    pub fn info(&self) -> Option<&UserInfo> {
        self.info.as_ref()
    }
}
