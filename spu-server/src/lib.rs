#![feature(generators)]
#![recursion_limit = "256"]

mod error;
mod start;
mod config;
mod core;
mod services;
mod controllers;

//#[cfg(test)]
//mod tests;

use start::main_loop;
use self::error::InternalServerError;

pub fn start_main() {
    utils::init_logger();
    main_loop();
}
