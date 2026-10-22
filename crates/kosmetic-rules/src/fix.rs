use kosmetic_diagnostics::apply_fixes;
use kosmetic_syntax::{Table, build_model, parse};

use crate::{Config, FileKind, run};

/// Fixing is repeated until no rule has anything left to fix. Fixes of
/// different rules can touch the same text and are applied in later passes.
const MAX_PASSES: usize = 10;

/// A source with the automatic fixes applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixed {
    /// The new text. Equal to the input when nothing was fixed.
    pub source: String,
    /// How many fixes were applied.
    pub applied: usize,
}

/// Why a file was left untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixError {
    /// The file has syntax or semantic errors: fixes assume a valid document.
    Invalid,
    /// A fix would have changed what the file means. This is a bug in a
    /// rule, the file is left untouched.
    ChangesMeaning,
}

/// The values of a document, or `None` if it has any error.
fn model(source: &str) -> Option<Table> {
    let parse = parse(source);
    let (table, semantic) = build_model(source, &parse.tree);
    (parse.diagnostics.is_empty() && semantic.is_empty()).then_some(table)
}

/// Applies the fixes of every enabled rule to a valid document. `file` selects
/// the schema rules that apply.
///
/// After every pass the document is parsed again and must hold exactly the
/// same values as before: a fix can reformat a file, never change its meaning.
///
/// # Errors
///
/// [`FixError::Invalid`] if the file has errors, [`FixError::ChangesMeaning`]
/// if a pass would change the values of the document.
pub fn fix_source(source: &str, config: &Config, file: FileKind) -> Result<Fixed, FixError> {
    let original = model(source).ok_or(FixError::Invalid)?;
    let mut current = source.to_owned();
    let mut total = 0;
    for _ in 0..MAX_PASSES {
        let parse = parse(&current);
        let diagnostics = run(&current, &parse, config, file);
        let (next, applied) = apply_fixes(
            &current,
            diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic.fix.as_ref()),
        );
        if applied == 0 || next == current {
            break;
        }
        if model(&next).as_ref() != Some(&original) {
            return Err(FixError::ChangesMeaning);
        }
        current = next;
        total += applied;
    }
    Ok(Fixed {
        source: current,
        applied: total,
    })
}

#[cfg(test)]
mod tests {
    use super::{FixError, fix_source};
    use crate::{Config, FileKind, Level};

    fn fixed(source: &str) -> String {
        fix_source(source, &Config::default(), FileKind::Other)
            .unwrap()
            .source
    }

    #[test]
    fn fixes_several_rules_at_once() {
        assert_eq!(
            fixed("#c  \nname=\"x\"  \n\n\n\n\"a\" = 1"),
            "# c\nname = \"x\"\n\na = 1\n"
        );
    }

    #[test]
    fn fixes_interacting_rules_in_later_passes() {
        // The blank lines and the missing newline overlap with other fixes.
        assert_eq!(fixed("a=1\r\nb = 2\n\n\n"), "a = 1\r\nb = 2\r\n");
    }

    #[test]
    fn counts_applied_fixes() {
        let result = fix_source("a=1 \n", &Config::default(), FileKind::Other).unwrap();
        assert_eq!(result.applied, 2);
        assert_eq!(
            fix_source("a = 1\n", &Config::default(), FileKind::Other)
                .unwrap()
                .applied,
            0
        );
    }

    #[test]
    fn leaves_invalid_files_alone() {
        assert_eq!(
            fix_source("a=1 \nb = \n", &Config::default(), FileKind::Other),
            Err(FixError::Invalid)
        );
        assert_eq!(
            fix_source("a = 1\na = 2\n", &Config::default(), FileKind::Other),
            Err(FixError::Invalid)
        );
    }

    #[test]
    fn respects_the_configuration() {
        let mut config = Config::default();
        config
            .rules
            .insert("style/spaces-around-equals".to_owned(), Level::Off);
        assert_eq!(
            fix_source("a=1 \n", &config, FileKind::Other)
                .unwrap()
                .source,
            "a=1\n"
        );
    }

    #[test]
    fn keeps_multiline_strings_intact() {
        let source = "a = \"\"\"\nx  \n\n\n\ny\"\"\"\n";
        assert_eq!(fixed(source), source);
    }
}
