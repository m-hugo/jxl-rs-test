#![warn(clippy::all, rust_2018_idioms)]
#![allow(unused_imports, unused_variables, unused_mut, dead_code)]

mod dec;
mod webp_loader;
mod jxl_loader;
mod http_loader;
mod app;

pub use app::TemplateApp;
