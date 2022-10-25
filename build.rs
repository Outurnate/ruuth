#[path = "src/config.rs"]
mod config;

use minify_html::{minify, Cfg};

fn main()
{
  let mut cfg = Cfg::new();
  cfg.minify_css = true;
  cfg.minify_js = true;
  std::fs::write(
    "templates/login.html",
    minify(&std::fs::read("templates/src/login.html").unwrap(), &cfg),
  )
  .unwrap();
  println!("cargo:rerun-if-changed=templates/src/login.html");

  let sample_config = toml::to_string(&config::Settings::default()).unwrap();
  std::fs::write("pkg/ruuth.toml.default", sample_config).unwrap();
  println!("cargo:rerun-if-changed=src/config.rs");
}
