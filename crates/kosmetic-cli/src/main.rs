//! `kosmetic` command line.
//!
//! Exit codes: 0 no errors, 1 at least one error-level diagnostic,
//! 2 the run itself failed (unreadable file, bad arguments).

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use kosmetic_diagnostics::{Diagnostic, Severity, render_human, render_json};
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
        /// When to color human output. `auto` colors a terminal unless `NO_COLOR` is set.
        #[arg(long, value_enum, default_value_t = Color::Auto)]
        color: Color,
        /// Configuration file. Default: the nearest `.kosmetic.toml` in the
        /// current directory or one of its parents.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// List every available rule.
    Rules,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

impl Color {
    fn enabled(self) -> bool {
        match self {
            Self::Auto => std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check {
            paths,
            format,
            color,
            config,
        } => {
            let color = color.enabled();
            match load_config(config, format, color) {
                Ok(config) => check(&paths, &config, format, color),
                Err(code) => code,
            }
        }
        Command::Rules => {
            for rule in all_rules() {
                let meta = rule.meta();
                println!("{:<32} {}", meta.code, meta.description);
            }
            ExitCode::SUCCESS
        }
    }
}

/// Loads the configuration. A file with errors stops the run with exit code
/// 2, after printing its diagnostics like those of any other file.
fn load_config(path: Option<PathBuf>, format: Format, color: bool) -> Result<Config, ExitCode> {
    let path = path.or_else(|| {
        let cwd = std::env::current_dir().ok()?;
        cwd.ancestors()
            .map(|dir| dir.join(".kosmetic.toml"))
            .find(|candidate| candidate.is_file())
    });
    let Some(path) = path else {
        return Ok(Config::default());
    };
    let source = std::fs::read_to_string(&path).map_err(|err| {
        eprintln!("kosmetic: cannot read {}: {err}", path.display());
        ExitCode::from(2)
    })?;
    let (config, diagnostics) = Config::from_toml(&source);
    if diagnostics.is_empty() {
        return Ok(config);
    }
    print_diagnostics(
        &path.display().to_string(),
        &source,
        &diagnostics,
        format,
        color,
    );
    Err(ExitCode::from(2))
}

fn print_diagnostics(
    name: &str,
    source: &str,
    diagnostics: &[Diagnostic],
    format: Format,
    color: bool,
) {
    match format {
        Format::Human => print!("{}", render_human(name, source, diagnostics, color)),
        Format::Json => println!("{}", render_json(name, source, diagnostics)),
    }
}

fn check(paths: &[PathBuf], config: &Config, format: Format, color: bool) -> ExitCode {
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
        diagnostics.extend(kosmetic_rules::run(&source, &parse, config));
        diagnostics.sort_by_key(|diagnostic| diagnostic.span);

        has_errors |= diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error);

        print_diagnostics(
            &path.display().to_string(),
            &source,
            &diagnostics,
            format,
            color,
        );
    }

    if has_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
