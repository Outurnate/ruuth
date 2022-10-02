use std::time::{SystemTime, UNIX_EPOCH};
use argon2::{password_hash::SaltString, Argon2, PasswordVerifier, PasswordHash};
use argon2::PasswordHasher;
use askama::filters::urlencode;
use base32::Alphabet;
use rand::thread_rng;
use sea_orm::{EntityTrait, ModelTrait};
use qrcode::{render::unicode, QrCode, types::QrError};
use rand_core::CryptoRngCore;
use sea_orm::{DatabaseConnection, Set, ActiveModelTrait};
use color_eyre::eyre::{Result, eyre};
use totp_lite::{Sha1, totp_custom, DEFAULT_STEP};
use tracing::{instrument, event};

use crate::entities::user;
use crate::entities::prelude::*;

fn fill_bytes<R: CryptoRngCore, const N: usize>(rng: &mut R) -> [u8; N]
{
  let mut arr = [0; N];
  rng.fill_bytes(&mut arr);
  arr
}

pub struct TotpSecret([u8; 128]);

impl TotpSecret
{
  pub fn new() -> Self
  {
    Self(fill_bytes(&mut thread_rng()))
  }

  pub fn get_setup_code(&self, username: &str, issuer: &str) -> SetupCode
  {
    SetupCode(format!("otpauth://totp/{issuer}:{username}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits=6&period=30",
      secret = base32::encode(Alphabet::RFC4648 { padding: true }, &self.0),
      issuer = urlencode(issuer).unwrap_or_default(),
      username = urlencode(username).unwrap_or_default()))
  }
}

impl Into<Vec<u8>> for TotpSecret
{
  fn into(self) -> Vec<u8>
  {
    self.0.to_vec()
  }
}

pub struct SetupCode(String);

impl SetupCode
{
  pub fn get_qr_code(&self) -> Result<String, QrError>
  {
    Ok(QrCode::new(&self.0)?.render::<unicode::Dense1x2>()
      .dark_color(unicode::Dense1x2::Light)
      .light_color(unicode::Dense1x2::Dark)
      .build())
  }

  pub fn get_raw_code(&self) -> String
  {
    self.0.clone()
  }
}

fn create_hasher<'a>(pepper: &'a [u8]) -> Result<Argon2<'a>, argon2::Error>
{
  Argon2::new_with_secret(pepper, argon2::Algorithm::Argon2id, argon2::Version::V0x13, argon2::Params::default())
}

#[derive(Clone)]
pub struct UserManager
{
  db: DatabaseConnection,
  issuer: String,
  pepper: Vec<u8>
}

impl UserManager
{
  pub fn new(db: DatabaseConnection, issuer: String, pepper: Vec<u8>) -> Result<Self>
  {
    Ok(Self
    {
      db,
      issuer,
      pepper
    })
  }

  pub async fn register(&self, username: String, password: String) -> Result<SetupCode>
  {
    let totp_secret = TotpSecret::new();
    let setup_code = totp_secret.get_setup_code(&username, &self.issuer);
    user::ActiveModel
    {
      username: Set(username),
      password_hash: Set(self.hash_password(password)?),
      totp_secret: Set(totp_secret.into())
    }.insert(&self.db).await?;

    Ok(setup_code)
  }

  fn hash_password(&self, password: String) -> Result<String>
  {
    Ok(create_hasher(&self.pepper)?.hash_password(password.as_bytes(), &SaltString::generate(&mut thread_rng()))?.to_string())
  }

  async fn get_user(&self, username: String) -> Result<user::Model>
  {
    User::find_by_id(username.clone()).one(&self.db).await?.ok_or_else(|| eyre!("User {} not found!", username))
  }

  pub async fn delete(&self, username: String) -> Result<()>
  {
    self.get_user(username).await?.delete(&self.db).await?;
    Ok(())
  }

  pub async fn reset_password(&self, username: String, password: String) -> Result<()>
  {
    let mut user: user::ActiveModel = self.get_user(username).await?.into();
    user.password_hash = Set(self.hash_password(password)?);
    user.update(&self.db).await?;
    Ok(())
  }

  pub async fn reset_mfa(&self, username: String) -> Result<SetupCode>
  {
    let secret = TotpSecret::new();
    let setup_code = secret.get_setup_code(&username, &self.issuer);
    let mut user: user::ActiveModel = self.get_user(username).await?.into();
    user.totp_secret = Set(secret.into());
    user.update(&self.db).await?;
    Ok(setup_code)
  }

  fn create_fake_user(&self) -> Result<user::Model>
  {
    Ok(user::Model
    {
      username: "kevin".to_owned(),
      password_hash: self.hash_password("hunter2".to_owned())?,
      totp_secret: vec![0; 128]
    })
  }

  #[instrument(skip(self, password, passcode))]
  pub async fn validate(&self, username: String, password: &str, passcode: &str) -> Result<bool>
  {
    // get the user, or get a fake one if we got a bad username
    let user = User::find_by_id(username).one(&self.db).await?;
    let faked = user.is_none();
    let fake_user = self.create_fake_user()?;
    let user = user.unwrap_or(fake_user);

    // validate the totp
    let seconds: u64 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let passcode_valid = totp_custom::<Sha1>(DEFAULT_STEP, 6, &user.totp_secret, seconds) == passcode;

    // validate the password
    let known_hash = PasswordHash::new(&user.password_hash)?;
    let password_valid = match create_hasher(&self.pepper)?.verify_password(password.as_bytes(), &known_hash)
    {
      Err(err) =>
      {
        event!(tracing::Level::INFO, "{}", err.to_string());
        false
      }
      Ok(_) => true,
    };
    event!(tracing::Level::INFO, "username found: {}, passcode valid: {}, password valid: {}", !faked, passcode_valid, password_valid);
    Ok(!faked && passcode_valid && password_valid)
  }
}