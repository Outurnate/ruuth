use std::time::Duration;

use async_sqlx_session::{PostgresSessionStore, MySqlSessionStore, SqliteSessionStore};
use axum::Router;
use axum_sessions::extractors::WritableSession;
use axum_sessions::{async_session::SessionStore};
use axum_sessions::{async_session::{MemoryStore}, SessionLayer, SameSite};
use serde::de::DeserializeOwned;
use crate::db::DatabaseBackend;
use crate::settings::{SessionStorage, SessionSettings};
use color_eyre::eyre::{Result, Context};
use async_redis_session::RedisSessionStore;

#[derive(Clone)]
pub struct SessionLayerHelper<S: SessionStore + Clone>
{
  store: S,
  layer: SessionLayer<S>
}

impl<S: SessionStore + Clone> SessionLayerHelper<S>
{
  pub fn new(session_store: S, secret: &[u8], settings: SessionSettings, domain: String) -> Self
  {
    Self
    {
      store: session_store.clone(),
      layer: SessionLayer::new(session_store, &secret)
        .with_same_site_policy(SameSite::Strict)
        .with_cookie_domain(domain)
        .with_secure(true)
        .with_cookie_name(settings.cookie_name.unwrap_or("ruuth".to_owned()))
        .with_session_ttl(settings.session_timeout_seconds.map_or(None, |s| Some(Duration::from_secs(s))))
    }
  }
}

#[derive(Clone)]
pub enum SessionBackendStorage
{
  InMemory(SessionLayerHelper<MemoryStore>),
  MySql(SessionLayerHelper<MySqlSessionStore>),
  Postgres(SessionLayerHelper<PostgresSessionStore>),
  Sqlite(SessionLayerHelper<SqliteSessionStore>),
  Redis(SessionLayerHelper<RedisSessionStore>)
}

impl SessionBackendStorage
{
  pub fn from_settings(settings: SessionSettings, db: DatabaseBackend, secret: &[u8], domain: String) -> Result<Self>
  {
    Ok(match settings.backend
    {
      SessionStorage::InMemory => Self::InMemory(SessionLayerHelper::new(MemoryStore::new(), secret, settings, domain)),
      SessionStorage::Sql => match db
      {
        DatabaseBackend::MySql(pool) => Self::MySql(SessionLayerHelper::new(MySqlSessionStore::from_client(pool), secret, settings, domain)),
        DatabaseBackend::Postgres(pool) => Self::Postgres(SessionLayerHelper::new(PostgresSessionStore::from_client(pool), secret, settings, domain)),
        DatabaseBackend::Sqlite(pool) => Self::Sqlite(SessionLayerHelper::new(SqliteSessionStore::from_client(pool), secret, settings, domain)),
      },
      SessionStorage::Redis(ref url) => Self::Redis(SessionLayerHelper::new(RedisSessionStore::new(url.to_owned()).wrap_err("could not connect to redis instance")?, secret, settings, domain)),
    })
  }

  pub async fn migrate(&self) -> Result<(), sqlx::Error>
  {
    match self
    {
      Self::MySql(helper) => helper.store.migrate().await,
      Self::Postgres(helper) => helper.store.migrate().await,
      Self::Sqlite(helper) => helper.store.migrate().await,
      _ => Ok(())
    }
  }

  pub async fn cleanup(&self) -> Result<(), sqlx::Error>
  {
    match self
    {
      Self::MySql(helper) => helper.store.cleanup().await,
      Self::Postgres(helper) => helper.store.cleanup().await,
      Self::Sqlite(helper) => helper.store.cleanup().await,
      _ => Ok(())
    }
  }
}

pub trait WritableSessionExt
{
  fn take<T: DeserializeOwned>(&mut self, key: &str) -> Option<T>;
}

impl WritableSessionExt for WritableSession
{
  fn take<T: DeserializeOwned>(&mut self, key: &str) -> Option<T>
  {
    let result = self.get::<T>(key);
    self.remove(key);
    result
  }
}

pub trait RouterExt
{
  fn layer_session(self, session: SessionBackendStorage) -> Self;
}

impl RouterExt for Router
{
  fn layer_session(self, session: SessionBackendStorage) -> Self
  {
    match session
    {
      SessionBackendStorage::InMemory(helper) => self.layer(helper.layer),
      SessionBackendStorage::MySql(helper) => self.layer(helper.layer),
      SessionBackendStorage::Postgres(helper) => self.layer(helper.layer),
      SessionBackendStorage::Sqlite(helper) => self.layer(helper.layer),
      SessionBackendStorage::Redis(helper) => self.layer(helper.layer)
    }
  }
}