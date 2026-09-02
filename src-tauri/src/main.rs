#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _program = args.next();
    if let Some(first) = args.next() {
        if first == abu_agent::rapidocr::RAPIDOCR_WORKER_ARG {
            return abu_agent::rapidocr::run_worker_entry(args);
        }
    }

    abu_agent::run();
    ExitCode::SUCCESS
}
