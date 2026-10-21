//! `kosmetic` command line.
//!
//! Exit codes: 0 no errors, 1 at least one error-level diagnostic,
//! 2 the run itself failed (unreadable file, bad arguments).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use kosmetic_diagnostics::{Severity, render_human, render_json};
use kosmetic_rules::{Config, all_rules};

#[derive(Parser)]
#[command(name = "kosmetic", version, about = "A fast, hand-written TOML linter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lint one or more TOML files.
    Check {
        /// Files to lint.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
    /// List every available rule.
    Rules,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { paths, format } => check(&paths, format),
        Command::Rules => {
            for rule in all_rules() {
                let meta = rule.meta();
                println!("{:<32} {}", meta.code, meta.description);
            }
            ExitCode::SUCCESS
        }
    }
}

fn check(paths: &[PathBuf], format: Format) -> ExitCode {
    // TODO(M3): discover and load `.kosmetic.toml`.
    let config = Config::default();
    let mut has_errors = false;

    for path in paths {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("kosmetic: cannot read {}: {err}", path.display());
                return ExitCode::from(2);
            }
        };

        let parse = kosmetic_syntax::parse(&source);
        let mut diagnostics = parse.diagnostics.clone();
        diagnostics.extend(kosmetic_syntax::build_model(&source, &parse.tree).1);
        diagnostics.extend(kosmetic_rules::run(&source, &parse, &config));
        diagnostics.sort_by_key(|diagnostic| diagnostic.span);

        has_errors |= diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error);

        let name = path.display().to_string();
        let rendered = match format {
            Format::Human => render_human(&name, &source, &diagnostics),
            Format::Json => render_json(&name, &source, &diagnostics),
        };
        print!("{rendered}");
        if matches!(format, Format::Json) {
            println!();
        }
    }

    if has_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
