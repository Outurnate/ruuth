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

use clap::{Args, Parser, Subcommand};
use color_eyre::{
  eyre::{Context, Result},
  Report,
};
use config::Config;
use std::fs::OpenOptions;
use tracing::{metadata::LevelFilter, Subscriber};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_log::{AsTrace, LogTracer};
use tracing_subscriber::{
  filter::{Filtered, Targets},
  fmt::{
    self,
    format::{Compact, DefaultFields, Format, Pretty},
  },
  layer::Layer,
  prelude::*,
  registry::LookupSpan,
};

use crate::config::{
  BehaviourSettings, HostSettings, LogLevel, Logging, SessionSettings, Settings,
};

#[derive(Parser)]
#[clap(
  author,
  version,
  about,
  long_about = "ruuth Copyright (C) 2022 Joe Dillon\nThis program comes with ABSOLUTELY NO WARRANTY; for details see LICENSE.md.\nThis is free software, and you are welcome to redistribute it\nunder certain conditions; see LICENSE.md for details."
)]
#[clap(propagate_version = true)]
struct Cli
{
  #[clap(subcommand)]
  command: Command,

  /// Path to configuration
  #[clap(short, long, value_parser)]
  config: Option<String>,

  #[clap(flatten)]
  verbose: clap_verbosity_flag::Verbosity,
}

trait IntoSubscriber<S: Subscriber + for<'span> LookupSpan<'span> + 'static>
{
  type Layer: Layer<S>;
  fn into_subscriber(self, guard_collector: &mut Vec<WorkerGuard>) -> Result<Self::Layer, Report>;
}

impl<S: Subscriber + for<'span> LookupSpan<'span> + 'static> IntoSubscriber<S>
  for clap_verbosity_flag::Verbosity
{
  fn into_subscriber(self, guard_collector: &mut Vec<WorkerGuard>) -> Result<Self::Layer, Report>
  {
    let targets = Targets::new().with_default(self.log_level_filter().as_trace());
    let (console_appender, guard) = tracing_appender::non_blocking(std::io::stdout());

    guard_collector.push(guard);

    Ok(
      fmt::layer::<S>()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .pretty()
        .with_writer(console_appender)
        .with_filter(targets),
    )
  }

  type Layer = Filtered<fmt::Layer<S, Pretty, Format<Pretty>, NonBlocking>, Targets, S>;
}

impl<S: Subscriber + for<'span> LookupSpan<'span> + 'static> IntoSubscriber<S> for Logging
{
  fn into_subscriber(self, guard_collector: &mut Vec<WorkerGuard>) -> Result<Self::Layer, Report>
  {
    let targets = if let Some(filter_text) = self.trace_filter
    {
      filter_text
        .parse::<Targets>()
        .wrap_err("error parsing log filter")?
    }
    else
    {
      Targets::new()
    }
    .with_default(match self.minimum_level
    {
      Some(LogLevel::Debug) => LevelFilter::DEBUG,
      Some(LogLevel::Trace) => LevelFilter::TRACE,
      Some(LogLevel::Info) => LevelFilter::INFO,
      Some(LogLevel::Warning) => LevelFilter::WARN,
      Some(LogLevel::Error) => LevelFilter::ERROR,
      None => LevelFilter::INFO,
    });

    let (file_appender, guard) = tracing_appender::non_blocking(
      OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(self.file)
        .wrap_err("error opening log file")?,
    );

    guard_collector.push(guard);

    Ok(
      fmt::layer::<S>()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .compact()
        .with_writer(file_appender)
        .with_filter(targets),
    )
  }

  type Layer = Filtered<fmt::Layer<S, DefaultFields, Format<Compact>, NonBlocking>, Targets, S>;
}

#[derive(Subcommand)]
pub enum Command
{
  /// Start the web server daemon
  Run,
  /// Add a user to the configured database
  AddUser(ShowsQrCode),
  /// Delete a user from the configured database
  DeleteUser(RequiresUsername),
  /// Reset the password for a user
  ResetPassword(RequiresUsername),
  /// Generate a new TOTP secret for a user
  ResetMFA(ShowsQrCode),
}

#[derive(Args)]
pub struct ShowsQrCode
{
  /// Target username
  #[clap(short, long, value_parser)]
  pub username: String,

  /// If specified, display TOTP URL as a scannable QR code
  #[clap(short, long, value_parser, default_value_t = false)]
  pub show_qr_code: bool,
}

#[derive(Args)]
pub struct RequiresUsername
{
  /// Target username
  #[clap(short, long, value_parser)]
  pub username: String,
}

pub fn parse_env() -> Result<(
  SessionSettings,
  HostSettings,
  BehaviourSettings,
  Command,
  Vec<WorkerGuard>,
)>
{
  color_eyre::install()?;
  LogTracer::init()?;

  let args = Cli::parse();
  let settings = Config::builder()
    .add_source(config::File::with_name(
      &args.config.unwrap_or(String::from("ruuth.toml")),
    ))
    .add_source(config::Environment::with_prefix("RUUTH"))
    .build()
    .wrap_err("failed to construct configuration")?;

  let settings: Settings = settings
    .try_deserialize()
    .wrap_err("invalid configuration")?;

  let mut guards = Vec::new();

  let file_subscriber = settings
    .logging
    .map(|v| v.into_subscriber(&mut guards))
    .transpose()?;
  let console_subscriber = args.verbose.into_subscriber(&mut guards)?;

  let subscriber = tracing_subscriber::registry()
    .with(file_subscriber)
    .with(console_subscriber);

  tracing::subscriber::set_global_default(subscriber)
    .wrap_err("failed to set global tracing subscriber")?;

  Ok((
    settings.session,
    settings.host,
    settings.behaviour,
    args.command,
    guards,
  ))
}
