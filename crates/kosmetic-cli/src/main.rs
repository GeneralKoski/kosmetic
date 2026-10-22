//! `kosmetic` command line.
//!
//! Exit codes: 0 no errors, 1 at least one error-level diagnostic,
//! 2 the run itself failed (unreadable file, bad arguments).

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use kosmetic_diagnostics::{Diagnostic, Severity, render_human, render_json};
use kosmetic_format::FormatError;
use kosmetic_rules::{Config, FileKind, FixError, all_rules, fix_source};

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
        /// Apply the automatic fixes to the files, then report what is left.
        #[arg(long)]
        fix: bool,
    },
    /// Format TOML files in place.
    Fmt {
        /// Files to format. `-` reads standard input and writes the result to
        /// standard output.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Do not write anything: list the files that would change and exit
        /// with code 1 if there are any.
        #[arg(long)]
        check: bool,
    },
    /// Run the language server, speaking LSP over standard input and output.
    Lsp,
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
            fix,
        } => {
            let color = color.enabled();
            match load_config(config, format, color) {
                Ok(config) => check(&paths, &config, format, color, fix),
                Err(code) => code,
            }
        }
        Command::Fmt { paths, check } => fmt(&paths, check),
        Command::Lsp => lsp(),
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

/// Writes the fixes of `path` back to it and returns the new source.
/// Files with errors are left as they are. Fails only if a fix would change
/// the meaning of the file or the file cannot be written.
fn apply_fix(path: &Path, source: String, config: &Config) -> Result<String, ExitCode> {
    let name = path.display();
    match fix_source(&source, config, FileKind::from_path(path)) {
        Ok(fixed) if fixed.applied == 0 => Ok(source),
        Ok(fixed) => {
            std::fs::write(path, &fixed.source).map_err(|err| {
                eprintln!("kosmetic: cannot write {name}: {err}");
                ExitCode::from(2)
            })?;
            let plural = if fixed.applied == 1 { "" } else { "s" };
            eprintln!(
                "kosmetic: fixed {} problem{plural} in {name}",
                fixed.applied
            );
            Ok(fixed.source)
        }
        Err(FixError::Invalid) => {
            eprintln!("kosmetic: {name} has errors, not fixing it");
            Ok(source)
        }
        Err(FixError::ChangesMeaning) => {
            eprintln!(
                "kosmetic: a fix would change the meaning of {name}, left untouched (this is a bug)"
            );
            Err(ExitCode::from(2))
        }
    }
}

fn check(paths: &[PathBuf], config: &Config, format: Format, color: bool, fix: bool) -> ExitCode {
    let mut has_errors = false;

    for path in paths {
        let mut source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("kosmetic: cannot read {}: {err}", path.display());
                return ExitCode::from(2);
            }
        };
        if fix {
            source = match apply_fix(path, source, config) {
                Ok(source) => source,
                Err(code) => return code,
            };
        }

        let diagnostics = kosmetic_rules::lint(&source, config, FileKind::from_path(path));

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

/// Formats every file. Exit codes: 0 all good, 1 with `--check` some file
/// would change, 2 a file could not be read, written or formatted.
fn fmt(paths: &[PathBuf], check: bool) -> ExitCode {
    let mut changed = 0;
    let mut failed = false;
    for path in paths {
        let stdin = path.as_os_str() == "-";
        let name = if stdin {
            "<stdin>".to_owned()
        } else {
            path.display().to_string()
        };
        let source = if stdin {
            let mut source = String::new();
            std::io::stdin().read_to_string(&mut source).map(|_| source)
        } else {
            std::fs::read_to_string(path)
        };
        let source = match source {
            Ok(source) => source,
            Err(err) => {
                eprintln!("kosmetic: cannot read {name}: {err}");
                failed = true;
                continue;
            }
        };
        let formatted = match kosmetic_format::format(&source) {
            Ok(formatted) => formatted,
            Err(FormatError::Invalid) => {
                eprintln!("kosmetic: {name} has errors, not formatting it");
                failed = true;
                continue;
            }
            Err(FormatError::ChangesMeaning) => {
                eprintln!(
                    "kosmetic: formatting would change the meaning of {name}, left untouched (this is a bug)"
                );
                failed = true;
                continue;
            }
        };
        if formatted != source {
            changed += 1;
            if check {
                eprintln!("kosmetic: would reformat {name}");
            }
        }
        if stdin {
            if !check {
                let _ = std::io::stdout().write_all(formatted.as_bytes());
            }
        } else if !check && formatted != source {
            if let Err(err) = std::fs::write(path, formatted) {
                eprintln!("kosmetic: cannot write {name}: {err}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::from(2)
    } else if check && changed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Serves the language server on the standard streams. Its own messages go
/// to stdout, so everything else it has to say goes to stderr.
fn lsp() -> ExitCode {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match kosmetic_lsp::run(stdin.lock(), stdout.lock()) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(err) => {
            eprintln!("kosmetic: language server stopped: {err}");
            ExitCode::from(2)
        }
    }
}
