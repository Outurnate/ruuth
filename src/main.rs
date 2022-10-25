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

#![forbid(unsafe_code)]
#![allow(clippy::all)]

mod challenge_manager;
mod db;
mod entities;
mod session;
mod env_parser;
mod tui;
mod user_manager;
mod web;
mod config;

use challenge_manager::ChallengeManager;
use color_eyre::eyre::{Context, Result};
use db::connect;
use session::SessionBackendStorage;
use env_parser::{parse_env, Command};
use sha2::{Digest, Sha512};
use tui::{get_password, maybe_show_qr_code};
use user_manager::UserManager;
use web::WebServer;

#[tokio::main]
async fn main() -> Result<()>
{
  let (session_config, host_config, behaviour_config, command, _guards) = parse_env()?;

  let db = connect(&host_config.database_url).await?;

  let mut hasher = Sha512::new();
  hasher.update(host_config.cluster_secret.as_bytes());
  let secret = hasher.finalize().to_vec();
  let user_manager = UserManager::new(db.1.clone(), host_config.domain.clone(), secret.clone())
    .wrap_err("failed to initialize user manager")?;

  match command
  {
    Command::Run =>
    {
      WebServer::new(
        user_manager,
        ChallengeManager::<128>::new(db.1.clone(), behaviour_config).await,
        session_config.session_timeout_seconds,
        host_config.domain.clone(),
      )
      .run(
        SessionBackendStorage::from_settings(session_config, db.0, &secret, host_config.domain)?,
        host_config.bind,
      )
      .await?;
    }
    Command::AddUser(args) => maybe_show_qr_code(
      user_manager
        .register(args.username, get_password()?)
        .await
        .wrap_err("failed to create new user")?,
      args.show_qr_code,
    )?,
    Command::DeleteUser(args) => user_manager
      .delete(args.username)
      .await
      .wrap_err("failed to delete user")?,
    Command::ResetPassword(args) => user_manager
      .reset_password(args.username, get_password()?)
      .await
      .wrap_err("failed to reset password")?,
    Command::ResetMFA(args) => maybe_show_qr_code(
      user_manager
        .reset_mfa(args.username)
        .await
        .wrap_err("failed to reset MFA token")?,
      args.show_qr_code,
    )?,
  }

  Ok(())
}
