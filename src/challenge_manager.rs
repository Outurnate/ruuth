use std::time::{SystemTime, Duration};

use axum_sessions::{extractors::WritableSession, async_session::serde_json};
use captcha::{RngCaptcha, filters::{Noise, Grid, Wave, Dots}};
use rand::{Rng, rngs::ThreadRng, thread_rng};
use rand_core::RngCore;
use sea_orm::{DatabaseConnection, Set, ActiveModelTrait, DbErr, ColumnTrait, EntityTrait, QueryFilter, PaginatorTrait};
use color_eyre::eyre::{Result, Context, eyre};
use tokio::{task::{self, JoinHandle}, time};
use tracing::{event, instrument};

use crate::{entities::{prelude::*, ban_tracker}, settings::BehaviourSettings, session::WritableSessionExt};

pub struct Base64Image
{
  pub w: u32,
  pub h: u32,
  pub base64: String
}

#[derive(Clone)]
pub struct ChallengeManager<const N: usize>
{
  db: DatabaseConnection,
  thresholds: BehaviourSettings
}

impl<const N: usize> ChallengeManager<N>
{
  pub async fn new(db: DatabaseConnection, thresholds: BehaviourSettings) -> Self
  {
    Self
    {
      db,
      thresholds
    }
  }

  pub fn issue_challenge(&self, session: &mut WritableSession) -> Result<String, serde_json::Error>
  {
    let mut challenge = [0; N];
    thread_rng().fill_bytes(&mut challenge);
    let token = base64::encode(challenge);
    session.insert("authenticity_token", &token)?;
    Ok(token)
  }

  pub async fn maybe_issue_captcha(&self, session: &mut WritableSession, host: &str) -> Result<Option<Base64Image>>
  {
    match self.thresholds.captcha
    {
      Some(threshold) if self.failure_count(host).await? > threshold =>
      {
        let num_chars = thread_rng().gen_range(4..7);
        let w = 220;
        let h = 120;
        let mut captcha = RngCaptcha::<ThreadRng>::from_rng(thread_rng());
        captcha
          .add_chars(num_chars)
          .apply_filter(Noise::new(0.3))
          .apply_filter(Grid::new(6, 6))
          .apply_filter(Wave::new(2.0, 10.0)).view(w, h)
          .apply_filter(Dots::new(15).max_radius(7).min_radius(4));
        session.insert("captcha_solution", captcha.chars_as_string()).wrap_err("failed to insert captcha solution into session")?;
        let base64 = captcha.as_base64().ok_or_else(|| eyre!("error encoding png"))?;
        Ok(Some(Base64Image { w, h, base64 }))
      }
      _ => Ok(None),
    }
  }

  pub fn cleanup_task(&self) -> JoinHandle<()>
  {
    let expiration = self.thresholds.expiration;
    let db = self.db.clone();
    task::spawn(async move
    {
      let mut interval = time::interval(Duration::from_secs(3600));
      loop
      {
        interval.tick().await;

        let cutoff = i64::saturating_sub(Self::now(), expiration);
        if let Err(error) = ban_tracker::Entity::delete_many()
          .filter(ban_tracker::Column::FailureTimestamp.lt(cutoff))
          .exec(&db)
          .await
        {
          event!(tracing::Level::ERROR, "{}", error);
        }
      }
    })
  }

  fn now() -> i64
  {
    if let Ok(duration) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
    {
      (duration.as_secs() / 60) as i64
    }
    else
    {
      0
    }
  }

  #[instrument(skip(self))]
  pub async fn validate(&self, session: &mut WritableSession, token: &str, captcha_text: &Option<String>, host: &str) -> Result<bool, DbErr>
  {
    let csrf_valid = session.take::<String>("authenticity_token").map_or(false, |authenticity_token| authenticity_token == token);
    let captcha_valid = session.take::<String>("captcha_solution").map_or(true, |solution| if let Some(captcha_text) = captcha_text { solution == *captcha_text } else { false });
    let banned = match self.thresholds.fake_login
      {
        Some(threshold) => self.failure_count(host).await? > threshold,
        None => false
      };
    let valid = csrf_valid && captcha_valid && !banned;
    event!(tracing::Level::INFO, "csrf passed: {}, captcha passed: {}, banned: {}", csrf_valid, captcha_valid, banned);
    Ok(valid)
  }

  #[instrument(skip(self))]
  pub async fn add_failure(&self, host: String) -> Result<(), DbErr>
  {
    ban_tracker::ActiveModel
    {
      host: Set(host),
      failure_timestamp: Set(Self::now()),
      ..Default::default()
    }.insert(&self.db).await?;
    Ok(())
  }

  #[instrument(skip(self))]
  async fn failure_count(&self, host: &str) -> Result<usize, DbErr>
  {
    let cutoff = i64::saturating_sub(Self::now(), self.thresholds.expiration);
    let failures = BanTracker::find()
      .filter(ban_tracker::Column::Host.eq(host))
      .filter(ban_tracker::Column::FailureTimestamp.gte(cutoff))
      .count(&self.db).await;
    if let Ok(failures) = failures
    {
      event!(tracing::Level::INFO, "counted failures: {}", failures);
    }
    failures
  }
}