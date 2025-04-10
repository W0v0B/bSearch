// src-tauri/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bsearch_lib::run;

fn main() {
    run()
}