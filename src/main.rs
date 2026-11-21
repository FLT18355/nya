mod audio;
mod bili;
mod cache;
mod config;
mod extract;
mod ui;
mod util;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        ui::home();
        return ExitCode::SUCCESS;
    };
    let rest = &args[1..];

    let result = match command.as_str() {
        "cache" | "c" => cache::run(rest),
        "bili" => bili::run(rest),
        "config" | "cfg" => config::run(rest),
        "x" | "extract" | "unpack" => extract::run(rest),
        "audio" | "a" => audio::run(rest),
        "help" | "-h" | "--help" => {
            ui::help();
            Ok(())
        }
        "version" | "-V" | "--version" | "-v" => {
            println!("{}", ui::version());
            Ok(())
        }
        other => {
            ui::unknown(other);
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            ui::fail(&message);
            ExitCode::from(1)
        }
    }
}
