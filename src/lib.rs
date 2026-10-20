//! XY Store: multi-store retail management system.

pub mod config;
pub mod controllers;
pub mod db;
pub mod errors;
pub mod middlewares;
pub mod models;
pub mod routes;
pub mod services;
pub mod state;
pub mod utils;

pub use routes::app;
