/*
ruuth: simple auth_request backend
Copyright (C) 2022 Joe Dillon

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use askama::Template;
use axum::{
  extract::Query,
  headers::{self, Header, HeaderName},
  http::{HeaderValue, StatusCode},
  response::Redirect,
  routing::{get, post},
  Extension, Form, Router, TypedHeader,
};
use axum_server::tls_rustls::RustlsConfig;
use axum_sessions::extractors::WritableSession;
use color_eyre::eyre::{Context, Result};
use hyperlocal::UnixServerExt;
use serde::Deserialize;
use std::{
  fmt::{Debug, Display},
  iter::once,
  time::Duration,
};
use tokio::{join, spawn, task, time};
use tracing::{event, instrument};

use crate::{
  challenge_manager::{Base64Image, ChallengeManager},
  config::BindTo,
  session::{RouterExt, SessionBackendStorage},
  user_manager::UserManager,
};

#[derive(Deserialize, Debug)]
struct LoginResponse
{
  authenticity_token: String,
  username: String,
  password: String,
  passcode: String,
  captcha: Option<String>,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginChallengeRequest
{
  authenticity_token: String,
  captcha: Option<Base64Image>,
  url: Option<String>,
  error: Option<bool>,
  realm: String,
}

#[derive(Deserialize, Debug)]
struct LoginQuery
{
  url: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ChallengeQuery
{
  url: Option<String>,
  error: Option<bool>,
}

macro_rules! header {
  ($struct_name:ident, $header_value:expr) => {
    struct $struct_name(String);

    impl Header for $struct_name
    {
      fn name() -> &'static HeaderName
      {
        static NAME: HeaderName = HeaderName::from_static($header_value);
        &NAME
      }

      fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
      where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
      {
        let value = values.next().ok_or_else(headers::Error::invalid)?;

        Ok($struct_name(value.to_str().unwrap_or("").to_owned()))
      }

      fn encode<E: Extend<HeaderValue>>(&self, values: &mut E)
      {
        let value = HeaderValue::from_str(&self.0).unwrap_or(HeaderValue::from_static(""));
        values.extend(once(value));
      }
    }
  };
}

header!(XForwardedFor, "x-forwarded-for");

trait TracedError<T, E: Display>: Sized
{
  fn trace_error(self) -> Result<T, StatusCode>;
}

impl<T, E: Display> TracedError<T, E> for Result<T, E>
{
  fn trace_error(self) -> Result<T, StatusCode>
  {
    self.map_err(|err| {
      event!(tracing::Level::ERROR, "{}", err.to_string());
      StatusCode::INTERNAL_SERVER_ERROR
    })
  }
}

#[derive(Clone)]
pub struct WebServer<const N: usize>
{
  user_manager: UserManager,
  challenge_manager: ChallengeManager<N>,
  session_timeout_seconds: Option<u64>,
  realm: String,
}

impl<const N: usize> WebServer<N>
{
  pub fn new(
    user_manager: UserManager,
    challenge_manager: ChallengeManager<N>,
    session_timeout_seconds: Option<u64>,
    realm: String,
  ) -> Self
  {
    Self {
      user_manager,
      challenge_manager,
      session_timeout_seconds,
      realm,
    }
  }

  #[instrument(skip(self, storage, bind_to))]
  pub async fn run(self, storage: SessionBackendStorage, bind_to: BindTo) -> Result<()>
  {
    let challenge_manager = self.challenge_manager.clone();
    let router = Router::new()
      .route("/login", post(Self::login_handler))
      .route("/logout", post(Self::logout_handler))
      .route("/", get(Self::auth_handler))
      .route("/validate", get(Self::validate_handler))
      .layer_session(storage.clone())
      .layer(Extension(self));

    storage.migrate().await?;
    let cleanup = task::spawn(async move {
      let mut interval = time::interval(Duration::from_secs(3600));
      loop
      {
        interval.tick().await;
        if let Err(error) = storage.cleanup().await
        {
          event!(tracing::Level::ERROR, "{}", error);
        }
      }
    });

    let service = router.into_make_service();
    let (cleanup, challenge_cleanup, server) = join!(
      spawn(cleanup),
      spawn(challenge_manager.cleanup_task()),
      match bind_to
      {
        BindTo::Tls {
          bind,
          public_key,
          private_key,
        } =>
        {
          let config = RustlsConfig::from_pem_file(public_key, private_key).await?;
          spawn(async move {
            axum_server::bind_rustls(bind, config)
              .serve(service)
              .await
              .wrap_err("error in TLS/TCP server")
          })
        }
        BindTo::Tcp { bind } =>
        {
          spawn(async move {
            axum_server::bind(bind)
              .serve(service)
              .await
              .wrap_err("error in TCP server")
          })
        }
        BindTo::Unix { path } =>
        {
          spawn(async {
            hyper::Server::bind_unix(path)?
              .serve(service)
              .await
              .wrap_err("error in unix socket server")
          })
        }
      }
    );

    cleanup??;
    challenge_cleanup??;
    server??;

    Ok(())
  }

  #[instrument(skip(this, form))]
  async fn login_handler(
    Extension(this): Extension<Self>,
    mut session: WritableSession,
    TypedHeader(XForwardedFor(origin_host)): TypedHeader<XForwardedFor>,
    query: Query<LoginQuery>,
    form: Form<LoginResponse>,
  ) -> Result<Redirect, StatusCode>
  {
    if this
      .challenge_manager
      .validate(
        &mut session,
        &form.authenticity_token,
        &form.captcha,
        &origin_host,
      )
      .await
      .trace_error()?
      & this
        .user_manager
        .validate(form.username.clone(), &form.password, &form.passcode)
        .await
        .trace_error()?
    {
      session.regenerate();
      session.insert("logged_in", true).trace_error()?;
      this.extend_session(&mut session);
      Ok(Redirect::to(match &query.url
      {
        Some(url) => &url,
        None => "/",
      }))
    }
    else
    {
      this
        .challenge_manager
        .add_failure(origin_host)
        .await
        .trace_error()?;
      Ok(Redirect::to("/?error=true"))
    }
  }

  #[instrument]
  async fn logout_handler(mut session: WritableSession) -> Result<(), StatusCode>
  {
    session.insert("logged_in", false).trace_error()?;
    session.regenerate();
    Ok(())
  }

  fn extend_session(&self, session: &mut WritableSession)
  {
    if let Some(expires) = self.session_timeout_seconds
    {
      session.expire_in(Duration::from_secs(expires));
    }
  }

  #[instrument(skip(this))]
  async fn validate_handler(
    Extension(this): Extension<Self>,
    mut session: WritableSession,
  ) -> StatusCode
  {
    this.extend_session(&mut session);
    if session
      .get::<bool>("logged_in")
      .map_or(false, |logged_in| logged_in)
    {
      event!(tracing::Level::TRACE, "Auth passed");
      StatusCode::OK
    }
    else
    {
      event!(tracing::Level::TRACE, "Auth failed");
      StatusCode::UNAUTHORIZED
    }
  }

  #[instrument(skip(this))]
  //#[axum_macros::debug_handler]
  async fn auth_handler(
    Extension(this): Extension<Self>,
    mut session: WritableSession,
    TypedHeader(XForwardedFor(origin_host)): TypedHeader<XForwardedFor>,
    query: Query<ChallengeQuery>,
  ) -> Result<impl askama_axum::IntoResponse, StatusCode>
  {
    Ok(LoginChallengeRequest {
      authenticity_token: this
        .challenge_manager
        .issue_challenge(&mut session)
        .trace_error()?,
      captcha: this
        .challenge_manager
        .maybe_issue_captcha(&mut session, &origin_host)
        .await
        .trace_error()?,
      url: query.url.clone(),
      error: query.error.clone(),
      realm: this.realm.clone(),
    })
  }
}
