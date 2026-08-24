// Sin consola en Windows en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    claude_burn_lib::run()
}
