#![forbid(unsafe_code)]
#![allow(clippy::all)]

mod user_manager;
mod web;
mod settings;
mod challenge_manager;
mod entities;
mod db;
mod tui;
mod session;

use challenge_manager::ChallengeManager;
use db::connect;
use session::SessionBackendStorage;
use settings::{parse_env, Command};
use sha2::Sha512;
use tui::{maybe_show_qr_code, get_password};
use user_manager::UserManager;
use color_eyre::eyre::{Result, Context};
use web::WebServer;
use sha2::Digest;

#[tokio::main]
async fn main() -> Result<()>
{
  let (session_config, host_config, behaviour_config, command, _guards) = parse_env()?;

  let db = connect(&host_config.database_url).await?;

  let mut hasher = Sha512::new();
  hasher.update(host_config.cluster_secret.as_bytes());
  let secret = hasher.finalize().to_vec();
  let user_manager = UserManager::new(db.1.clone(), host_config.domain.clone(), secret.clone()).wrap_err("failed to initialize user manager")?;

  match command
  {
    Command::Run =>
    {
      WebServer::new(user_manager, ChallengeManager::<128>::new(db.1.clone(), behaviour_config).await, session_config.session_timeout_seconds, host_config.realm.unwrap_or_else(|| host_config.domain.clone()))
        .run(SessionBackendStorage::from_settings(session_config, db.0, &secret, host_config.domain)?, host_config.bind, host_config.tls).await?;
    },
    Command::AddUser(args) => maybe_show_qr_code(user_manager.register(args.username, get_password()?).await.wrap_err("failed to create new user")?, args.show_qr_code)?,
    Command::DeleteUser(args) => user_manager.delete(args.username).await.wrap_err("failed to delete user")?,
    Command::ResetPassword(args) => user_manager.reset_password(args.username, get_password()?).await.wrap_err("failed to reset password")?,
    Command::ResetMFA(args) => maybe_show_qr_code(user_manager.reset_mfa(args.username).await.wrap_err("failed to reset MFA token")?, args.show_qr_code)?
  }
  
  Ok(())
}