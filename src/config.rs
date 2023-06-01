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

use std::{
  net::{IpAddr, Ipv4Addr, SocketAddr},
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct BehaviourSettings
{
  pub captcha: Option<u64>,
  pub fake_login: Option<u64>,
  pub expiration: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SessionStorage
{
  InMemory,
  Sql,
  Redis(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionSettings
{
  pub session_timeout_seconds: Option<u64>,
  pub cookie_name: Option<String>,
  pub backend: SessionStorage,
}

impl Default for SessionSettings
{
  fn default() -> Self
  {
    Self {
      backend: SessionStorage::InMemory,
      session_timeout_seconds: None,
      cookie_name: None,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Logging
{
  pub trace_filter: Option<String>,
  pub minimum_level: Option<LogLevel>,
  pub file: PathBuf,
}

impl Default for Logging
{
  fn default() -> Self
  {
    Self {
      trace_filter: None,
      minimum_level: Some(LogLevel::Info),
      file: Path::new("/var/log/ruuth/ruuth.log").to_path_buf(),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub enum LogLevel
{
  Debug,
  Trace,
  #[default]
  Info,
  Warning,
  Error,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum BindTo
{
  Tcp
  {
    bind: SocketAddr
  },
  Tls
  {
    bind: SocketAddr,
    public_key: PathBuf,
    private_key: PathBuf,
  },
  Unix
  {
    path: PathBuf
  },
}

impl Default for BindTo
{
  fn default() -> Self
  {
    Self::Tcp {
      bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3000),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct HostSettings
{
  pub cluster_secret: String,
  pub database_url: String,
  pub domain: String,
  pub bind: BindTo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings
{
  pub host: HostSettings,
  pub behaviour: BehaviourSettings,
  pub session: SessionSettings,
  pub logging: Option<Logging>,
}

impl Default for Settings
{
  fn default() -> Self
  {
    Self {
      host: Default::default(),
      behaviour: Default::default(),
      session: Default::default(),
      logging: Some(Default::default()),
    }
  }
}
