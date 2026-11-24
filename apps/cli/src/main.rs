//! `astryx`: the Rust port's counterpart of upstream's agent-facing CLI.
//!
//! Every answer is short by default; `--dense` is shorter still, for coding
//! agents that read every byte a command prints.

mod commands;
mod manifest;

use std::process::ExitCode;

const HELP: &str = "astryx: Astryx for GPUI

USAGE
  astryx component --list [--all]          components by category (ported ones marked *)
  astryx component <Name> [--dense] [--package core|lab|charts|richtext]
                                           upstream props and guidance, and the Rust API
  astryx tokens [filter] [--theme <name>] [--mode light|dark]
                                           CSS token, Rust field and value
  astryx themes                            the themes compiled in
  astryx docs [topic]                      guides: gpui, conventions, customization, effects, porting
  astryx parity [--missing]                how much of upstream is ported; fails on unmapped props
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let positional: Vec<&str> = {
        let mut out = Vec::new();
        let mut skip = false;
        for arg in args.iter().skip(1) {
            if skip {
                skip = false;
            } else if matches!(arg.as_str(), "--theme" | "--mode" | "--package") {
                skip = true;
            } else if !arg.starts_with("--") {
                out.push(arg.as_str());
            }
        }
        out
    };

    let result = match args.first().map(String::as_str) {
        Some("component") if flag("--list") => commands::component_list(flag("--all")),
        Some("component") => match positional.first() {
            Some(name) => commands::component(name, flag("--dense"), option("--package")),
            None => Err("component needs a name, or --list".into()),
        },
        Some("tokens") => commands::tokens(
            positional.first().copied(),
            option("--theme").unwrap_or("default"),
            option("--mode").unwrap_or("light"),
        ),
        Some("themes") => commands::themes(),
        Some("docs") => commands::docs(positional.first().copied()),
        Some("parity") => commands::parity(flag("--missing")),
        Some("help" | "--help" | "-h") | None => {
            print!("{HELP}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{HELP}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
