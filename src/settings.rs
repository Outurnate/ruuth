use std::{net::SocketAddr, fs::OpenOptions};
use clap::{Parser, Subcommand, Args};
use color_eyre::{eyre::{Result, Context}, Report};
use config::Config;
use serde::Deserialize;
use tracing::{metadata::LevelFilter, Subscriber};
use tracing_appender::non_blocking::{WorkerGuard, NonBlocking};
use tracing_log::{AsTrace, LogTracer};
use tracing_subscriber::{fmt::{self, format::{Pretty, Format, Compact, DefaultFields}}, filter::{Targets, Filtered}, prelude::*, registry::LookupSpan};
use tracing_subscriber::layer::Layer;

#[derive(Parser)]
#[clap(author, version, about, long_about = "An apache auth_request handler with no bells and/or whistles")]
#[clap(propagate_version = true)]
struct Cli
{
  #[clap(subcommand)]
  command: Command,

  /// Path to configuration
  #[clap(short, long, value_parser)]
  config: Option<String>,

  #[clap(flatten)]
  verbose: clap_verbosity_flag::Verbosity
}

trait IntoSubscriber<S: Subscriber + for<'span> LookupSpan<'span> + 'static>
{
  type Layer: Layer<S>;
  fn into_subscriber(self, guard_collector: &mut Vec<WorkerGuard>) -> Result<Self::Layer, Report>;
}

impl<S: Subscriber + for<'span> LookupSpan<'span> + 'static> IntoSubscriber<S> for clap_verbosity_flag::Verbosity
{
  fn into_subscriber(self, guard_collector: &mut Vec<WorkerGuard>) -> Result<Self::Layer, Report>
  {
    let targets = Targets::new().with_default(self.log_level_filter().as_trace());
    let (console_appender, guard) = tracing_appender::non_blocking(std::io::stdout());

    guard_collector.push(guard);

    Ok(fmt::layer::<S>()
      .with_level(true)
      .with_target(true)
      .with_thread_ids(false)
      .with_thread_names(false)
      .pretty()
      .with_writer(console_appender)
      .with_filter(targets))
  }

  type Layer = Filtered<fmt::Layer<S, Pretty, Format<Pretty>, NonBlocking>, Targets, S>;
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
  ResetMFA(ShowsQrCode)
}

#[derive(Args)]
pub struct ShowsQrCode
{
  /// Target username
  #[clap(short, long, value_parser)]
  pub username: String,

  /// If specified, display TOTP URL as a scannable QR code
  #[clap(short, long, value_parser, default_value_t = false)]
  pub show_qr_code: bool
}

#[derive(Args)]
pub struct RequiresUsername
{
  /// Target username
  #[clap(short, long, value_parser)]
  pub username: String
}

#[derive(Deserialize)]
pub struct KeyPair
{
  pub public_key: String,
  pub private_key: String
}

#[derive(Deserialize, Debug, Clone)]
pub struct BehaviourSettings
{
  pub captcha: Option<usize>,
  pub fake_login: Option<usize>,
  pub expiration: i64
}

#[derive(Deserialize, Debug)]
pub enum SessionStorage
{
  InMemory,
  Sql,
  Redis(String)
}

#[derive(Deserialize, Debug)]
pub struct SessionSettings
{
  pub session_timeout_seconds: Option<u64>,
  pub cookie_name: Option<String>,
  pub backend: SessionStorage
}

impl Default for SessionSettings
{
  fn default() -> Self
  {
    Self { backend: SessionStorage::InMemory, session_timeout_seconds: None, cookie_name: None }
  }
}

#[derive(Deserialize, Default)]
pub struct Logging
{
  trace_filter: Option<String>,
  minimum_level: Option<LogLevel>,
  file: String
}

impl<S: Subscriber + for<'span> LookupSpan<'span> + 'static> IntoSubscriber<S> for Logging
{
  fn into_subscriber(self, guard_collector: &mut Vec<WorkerGuard>) -> Result<Self::Layer, Report>
  {
    let targets = if let Some(filter_text) = self.trace_filter
    {
      filter_text.parse::<Targets>().wrap_err("error parsing log filter")?
    }
    else
    {
      Targets::new()
    }.with_default(match self.minimum_level
      {
        Some(LogLevel::Debug) => LevelFilter::DEBUG,
        Some(LogLevel::Trace) => LevelFilter::TRACE,
        Some(LogLevel::Info) => LevelFilter::INFO,
        Some(LogLevel::Warning) => LevelFilter::WARN,
        Some(LogLevel::Error) => LevelFilter::ERROR,
        None => LevelFilter::INFO
      });
    
    let (file_appender, guard) = tracing_appender::non_blocking(OpenOptions::new()
      .create(true)
      .write(true)
      .append(true)
      .open(self.file).wrap_err("error opening log file")?);
    
    guard_collector.push(guard);
    
    Ok(fmt::layer::<S>()
      .with_level(true)
      .with_target(true)
      .with_thread_ids(false)
      .with_thread_names(false)
      .compact()
      .with_writer(file_appender)
      .with_filter(targets))
  }

  type Layer = Filtered<fmt::Layer<S, DefaultFields, Format<Compact>, NonBlocking>, Targets, S>;
}

#[derive(Deserialize)]
pub enum LogLevel
{
  Debug,
  Trace,
  Info,
  Warning,
  Error
}

impl Default for LogLevel
{
  fn default() -> Self
  {
    LogLevel::Info
  }
}

#[derive(Deserialize)]
pub struct HostSettings
{
  pub bind: SocketAddr,
  pub cluster_secret: String,
  pub database_url: String,
  pub tls: Option<KeyPair>,
  pub realm: Option<String>,
  pub domain: String
}

#[derive(Deserialize)]
struct Settings
{
  host: HostSettings,
  behaviour: BehaviourSettings,
  session: SessionSettings,
  logging: Option<Logging>
}

pub fn parse_env() -> Result<(SessionSettings, HostSettings, BehaviourSettings, Command, Vec<WorkerGuard>)>
{
  color_eyre::install()?;
  LogTracer::init()?;

  let args = Cli::parse();
  let settings = Config::builder()
    .add_source(config::File::with_name(&args.config.unwrap_or(String::from("ruuth.toml"))))
    .add_source(config::Environment::with_prefix("RUUTH"))
    .build().wrap_err("failed to construct configuration")?;
  
  let settings: Settings = settings.try_deserialize().wrap_err("invalid configuration")?;

  let mut guards = Vec::new();

  let file_subscriber = settings.logging.map(|v| v.into_subscriber(&mut guards)).transpose()?;
  let console_subscriber = args.verbose.into_subscriber(&mut guards)?;

  let subscriber = tracing_subscriber::registry()
    .with(file_subscriber)
    .with(console_subscriber);
  
  tracing::subscriber::set_global_default(subscriber).wrap_err("failed to set global tracing subscriber")?;

  Ok((settings.session, settings.host, settings.behaviour, args.command, guards))
}