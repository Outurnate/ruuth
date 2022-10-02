use minify_html::{Cfg, minify};

fn main()
{
  let mut cfg = Cfg::new();
  cfg.minify_css = true;
  cfg.minify_js = true;
  std::fs::write("templates/login.html", minify(&std::fs::read("templates/src/login.html").unwrap(), &cfg)).unwrap();
  println!("cargo:rerun-if-changed=templates/src/login.html");
}