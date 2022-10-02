use askama::Template;
use axum::{routing::{get, post}, http::{StatusCode, HeaderValue}, Router, Extension, Form, TypedHeader, headers::{Header, HeaderName, self}, extract::Query, response::Redirect};
use axum_server::tls_rustls::RustlsConfig;
use axum_sessions::extractors::WritableSession;
use serde::Deserialize;
use tracing::{instrument, event};
use std::{net::SocketAddr, iter::once, fmt::{Debug, Display}, time::Duration};
use color_eyre::eyre::Result;
use tokio::{task, time, join};

use crate::{user_manager::UserManager, settings::KeyPair, challenge_manager::{ChallengeManager, Base64Image}, session::{SessionBackendStorage, RouterExt}};

#[derive(Deserialize, Debug)]
struct LoginResponse
{
  authenticity_token: String,
  username: String,
  password: String,
  passcode: String,
  captcha: Option<String>
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginChallengeRequest
{
  authenticity_token: String,
  captcha: Option<Base64Image>,
  url: Option<String>,
  error: Option<bool>,
  realm: String
}

#[derive(Deserialize, Debug)]
struct LoginQuery
{
  url: Option<String>
}

#[derive(Deserialize, Debug)]
struct ChallengeQuery
{
  url: Option<String>,
  error: Option<bool>
}

macro_rules! header
{
  ($struct_name:ident, $header_value:expr) =>
  {
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
        I: Iterator<Item = &'i HeaderValue>
      {
        let value = values
          .next()
          .ok_or_else(headers::Error::invalid)?;

        Ok($struct_name(value.to_str().unwrap_or("").to_owned()))
      }

      fn encode<E: Extend<HeaderValue>>(&self, values: &mut E)
      {
        let value = HeaderValue::from_str(&self.0).unwrap_or(HeaderValue::from_static(""));
        values.extend(once(value));
      }
    }
  }
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
    self.map_err(|err|
    {
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
  realm: String
}

impl<const N: usize> WebServer<N>
{
  pub fn new(user_manager: UserManager, challenge_manager: ChallengeManager<N>, session_timeout_seconds: Option<u64>, realm: String) -> Self
  {
    Self { user_manager, challenge_manager, session_timeout_seconds, realm }
  }

  #[instrument(skip(self, storage, addr, tls_keypair))]
  pub async fn run(self, storage: SessionBackendStorage, addr: SocketAddr, tls_keypair: Option<KeyPair>) -> Result<()>
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
    let cleanup = task::spawn(async move
    {
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
    let (cleanup, challenge_cleanup, server) = if let Some(keypair) = tls_keypair
    {
      let config = RustlsConfig::from_pem_file(keypair.public_key, keypair.private_key).await?;
      join!(cleanup, challenge_manager.cleanup_task(), axum_server::bind_rustls(addr, config).serve(service))
    }
    else
    {
      join!(cleanup, challenge_manager.cleanup_task(), axum_server::bind(addr).serve(service))
    };

    cleanup?;
    challenge_cleanup?;
    server?;

    Ok(())
  }

  #[instrument(skip(this, form))]
  async fn login_handler(
    Extension(this): Extension<Self>,
    mut session: WritableSession,
    TypedHeader(XForwardedFor(origin_host)): TypedHeader<XForwardedFor>,
    form: Form<LoginResponse>,
    query: Query<LoginQuery>
  ) -> Result<Redirect, StatusCode>
  {
    if
      this.challenge_manager.validate(&mut session, &form.authenticity_token, &form.captcha, &origin_host).await.trace_error()? &
      this.user_manager.validate(form.username.clone(), &form.password, &form.passcode).await.trace_error()?
    {
      session.regenerate();
      session.insert("logged_in", true).trace_error()?;
      this.extend_session(&mut session);
      Ok(Redirect::to(match &query.url { Some(url) => &url, None => "/" }))
    }
    else
    {
      this.challenge_manager.add_failure(origin_host).await.trace_error()?;
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

  fn extend_session(
    &self,
    session: &mut WritableSession
  )
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
    if session.get::<bool>("logged_in").map_or(false, |logged_in| logged_in)
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
  async fn auth_handler(
    Extension(this): Extension<Self>,
    mut session: WritableSession,
    TypedHeader(XForwardedFor(origin_host)): TypedHeader<XForwardedFor>,
    query: Query<ChallengeQuery>
  ) -> Result<impl askama_axum::IntoResponse, StatusCode>
  {
    Ok(LoginChallengeRequest
    {
      authenticity_token: this.challenge_manager.issue_challenge(&mut session).trace_error()?,
      captcha: this.challenge_manager.maybe_issue_captcha(&mut session, &origin_host).await.trace_error()?,
      url: query.url.clone(),
      error: query.error.clone(),
      realm: this.realm.clone()
    })
  }
}