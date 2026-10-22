//! Rules for `pyproject.toml`, following PEP 517, PEP 621 and PEP 735.

use kosmetic_diagnostics::Span;
use kosmetic_syntax::{Table, Value};

use super::pep::{check_requirement, is_name, is_specifier_set, is_version};
use super::{
    Shape, check_shape, item_spans, report_unknown_keys, string, table, tables, type_name,
};
use crate::{FileKind, Level, Rule, RuleContext, RuleMeta};

const ROOT_KEYS: &[&str] = &["build-system", "project", "tool", "dependency-groups"];

const BUILD_SYSTEM_KEYS: &[&str] = &["requires", "build-backend", "backend-path"];

/// Keys of `[project]` and the shape of their value.
const PROJECT_FIELDS: &[(&str, Shape)] = &[
    ("name", Shape::Str),
    ("version", Shape::Str),
    ("description", Shape::Str),
    ("readme", Shape::StrOrTable),
    ("requires-python", Shape::Str),
    ("license", Shape::StrOrTable),
    ("license-files", Shape::StrArray),
    ("authors", Shape::Table),
    ("maintainers", Shape::Table),
    ("keywords", Shape::StrArray),
    ("classifiers", Shape::StrArray),
    ("urls", Shape::Table),
    ("scripts", Shape::Table),
    ("gui-scripts", Shape::Table),
    ("entry-points", Shape::Table),
    ("dependencies", Shape::StrArray),
    ("optional-dependencies", Shape::Table),
    ("dynamic", Shape::StrArray),
];

fn is_pyproject(ctx: &RuleContext<'_>) -> bool {
    ctx.file == FileKind::PyProject
}

/// The items of an array and their spans. A span that cannot be found falls
/// back to the whole array.
fn items<'a>(ctx: &RuleContext<'_>, table: &'a Table, key: &str) -> Vec<(&'a Value, Span)> {
    let Some(Value::Array(values)) = table.entries.get(key) else {
        return Vec::new();
    };
    let whole = table.value_span(key).unwrap_or_default();
    let spans = item_spans(&ctx.parse.tree, whole);
    values
        .iter()
        .enumerate()
        .map(|(i, value)| (value, spans.get(i).copied().unwrap_or(whole)))
        .collect()
}

// ---------------------------------------------------------------------------

/// Flags keys that `pyproject.toml` does not allow: unknown top-level tables
/// (tools belong under `[tool]`) and unknown keys in `[project]` and
/// `[build-system]`.
pub struct PyprojectUnknownKey;

static PYPROJECT_UNKNOWN_KEY: RuleMeta = RuleMeta {
    code: "schema/pyproject-unknown-key",
    description: "Disallow keys that pyproject.toml does not define",
    default_level: Level::Warn,
};

impl Rule for PyprojectUnknownKey {
    fn meta(&self) -> &'static RuleMeta {
        &PYPROJECT_UNKNOWN_KEY
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_pyproject(ctx) {
            return;
        }
        let root = ctx.model;
        report_unknown_keys(ctx, root, "the file", ROOT_KEYS);
        if let Some(project) = table(root, "project") {
            let known: Vec<_> = PROJECT_FIELDS.iter().map(|(key, _)| *key).collect();
            report_unknown_keys(ctx, project, "`[project]`", &known);
        }
        if let Some(build) = table(root, "build-system") {
            report_unknown_keys(ctx, build, "`[build-system]`", BUILD_SYSTEM_KEYS);
        }
    }
}

// ---------------------------------------------------------------------------

/// Flags what the standards require and the file lacks: the project name,
/// its version (unless it is dynamic) and the build requirements.
pub struct PyprojectMissingField;

static PYPROJECT_MISSING_FIELD: RuleMeta = RuleMeta {
    code: "schema/pyproject-missing-field",
    description: "Require the fields that pyproject.toml needs",
    default_level: Level::Error,
};

impl Rule for PyprojectMissingField {
    fn meta(&self) -> &'static RuleMeta {
        &PYPROJECT_MISSING_FIELD
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_pyproject(ctx) {
            return;
        }
        let root = ctx.model;
        if let Some(project) = table(root, "project") {
            let span = root.key_span("project").unwrap_or_default();
            if !project.entries.contains_key("name") {
                ctx.report("`[project]` is missing `name`", span);
            }
            let dynamic = match project.entries.get("dynamic") {
                Some(Value::Array(items)) => items.contains(&Value::String("version".to_owned())),
                _ => false,
            };
            if !project.entries.contains_key("version") && !dynamic {
                ctx.report("`[project]` is missing `version`", span).help =
                    Some("set it, or list \"version\" in `dynamic`".to_owned());
            }
        }
        if let Some(build) = table(root, "build-system") {
            if !build.entries.contains_key("requires") {
                let span = root.key_span("build-system").unwrap_or_default();
                ctx.report("`[build-system]` is missing `requires`", span);
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Flags values that packaging tools reject: bad names, versions and
/// requirements, wrong types, and fields set both statically and in `dynamic`.
pub struct PyprojectInvalidValue;

static PYPROJECT_INVALID_VALUE: RuleMeta = RuleMeta {
    code: "schema/pyproject-invalid-value",
    description: "Disallow values that packaging tools reject in pyproject.toml",
    default_level: Level::Error,
};

impl PyprojectInvalidValue {
    fn requirements(ctx: &mut RuleContext<'_>, table: &Table, key: &str) {
        for (value, span) in items(ctx, table, key) {
            let Value::String(text) = value else {
                continue;
            };
            if let Err(why) = check_requirement(text) {
                ctx.report(format!("invalid requirement `{text}`: {why}"), span)
                    .help = Some("write it like `name>=1.0` (PEP 508)".to_owned());
            }
        }
    }

    fn project(ctx: &mut RuleContext<'_>, project: &Table, additive_dynamic: bool) {
        for (key, shape) in PROJECT_FIELDS {
            if matches!(*key, "authors" | "maintainers") {
                Self::people(ctx, project, key);
            } else {
                check_shape(ctx, project, &format!("project.{key}"), key, *shape);
            }
        }

        let span = |key: &str| project.value_span(key).unwrap_or_default();
        if let Some(name) = string(project, "name").filter(|name| !is_name(name)) {
            ctx.report(
                format!("`{name}` is not a valid project name"),
                span("name"),
            )
            .help = Some(
                "use letters, digits, `.`, `_` and `-`, starting and ending with a letter or digit"
                    .to_owned(),
            );
        }
        if let Some(version) = string(project, "version").filter(|version| !is_version(version)) {
            ctx.report(
                format!("`{version}` is not a valid PEP 440 version"),
                span("version"),
            )
            .help = Some("use something like \"1.2.3\", \"1.0rc1\" or \"2024.1.post2\"".to_owned());
        }
        if let Some(python) = string(project, "requires-python").filter(|p| !is_specifier_set(p)) {
            ctx.report(
                format!("`{python}` is not a valid version specifier"),
                span("requires-python"),
            )
            .help = Some("use something like \">=3.9\" or \">=3.9,<4\"".to_owned());
        }
        Self::requirements(ctx, project, "dependencies");
        Self::optional_dependencies(ctx, project);
        Self::tables_of_strings(ctx, project);
        Self::dynamic(ctx, project, additive_dynamic);
    }

    /// `authors` and `maintainers`: an array of `{ name, email }` tables.
    fn people(ctx: &mut RuleContext<'_>, project: &Table, key: &str) {
        let Some(value) = project.entries.get(key) else {
            return;
        };
        let span = project.value_span(key).unwrap_or_default();
        let people = tables(value);
        let is_list = matches!(value, Value::Array(_) | Value::ArrayOfTables(_));
        if !is_list || people.len() != item_count(value) {
            ctx.report(
                format!("`project.{key}` must be an array of tables with `name` and `email`"),
                span,
            );
            return;
        }
        for person in people {
            report_unknown_keys(
                ctx,
                person,
                &format!("an entry of `project.{key}`"),
                &["name", "email"],
            );
            for field in ["name", "email"] {
                check_shape(
                    ctx,
                    person,
                    &format!("project.{key}.{field}"),
                    field,
                    Shape::Str,
                );
            }
        }
    }

    /// `[project.optional-dependencies]`: extra name to list of requirements.
    fn optional_dependencies(ctx: &mut RuleContext<'_>, project: &Table) {
        let Some(extras) = table(project, "optional-dependencies") else {
            return;
        };
        for (extra, value) in &extras.entries {
            let span = extras.key_span(extra).unwrap_or_default();
            if !is_name(extra) {
                ctx.report(format!("`{extra}` is not a valid extra name"), span);
            }
            if check_shape(
                ctx,
                extras,
                &format!("project.optional-dependencies.{extra}"),
                extra,
                Shape::StrArray,
            ) && matches!(value, Value::Array(_))
            {
                Self::requirements(ctx, extras, extra);
            }
        }
    }

    /// `urls`, `scripts` and `gui-scripts` map names to strings,
    /// `entry-points` maps group names to such tables.
    fn tables_of_strings(ctx: &mut RuleContext<'_>, project: &Table) {
        for key in ["urls", "scripts", "gui-scripts"] {
            let Some(inner) = table(project, key) else {
                continue;
            };
            for name in inner.entries.keys() {
                check_shape(
                    ctx,
                    inner,
                    &format!("project.{key}.{name}"),
                    name,
                    Shape::Str,
                );
            }
        }
        let Some(groups) = table(project, "entry-points") else {
            return;
        };
        for (group, value) in &groups.entries {
            let span = groups.key_span(group).unwrap_or_default();
            if matches!(group.as_str(), "console_scripts" | "gui_scripts") {
                ctx.report(format!("`{group}` cannot be an entry point group"), span)
                    .help = Some("use `[project.scripts]` or `[project.gui-scripts]`".to_owned());
            }
            let Value::Table(entries) = value else {
                check_shape(
                    ctx,
                    groups,
                    &format!("project.entry-points.{group}"),
                    group,
                    Shape::Table,
                );
                continue;
            };
            for name in entries.entries.keys() {
                let path = format!("project.entry-points.{group}.{name}");
                check_shape(ctx, entries, &path, name, Shape::Str);
            }
        }
    }

    /// `dynamic` lists fields a build backend fills in: they cannot also be
    /// set in the file, and `name` can never be dynamic. Poetry is the
    /// exception: it adds its own values to the ones in the file.
    fn dynamic(ctx: &mut RuleContext<'_>, project: &Table, additive: bool) {
        for (value, span) in items(ctx, project, "dynamic") {
            let Value::String(field) = value else {
                continue;
            };
            let known = PROJECT_FIELDS
                .iter()
                .any(|(key, _)| key == field && !matches!(*key, "name" | "dynamic"));
            if field == "name" {
                ctx.report("`name` cannot be dynamic", span);
            } else if !known {
                ctx.report(
                    format!("`{field}` is not a field that can be dynamic"),
                    span,
                );
            } else if let Some(set) = project.key_span(field).filter(|_| !additive) {
                ctx.report(
                    format!("`{field}` is listed in `dynamic` but also set"),
                    set,
                )
                .labels
                .push(kosmetic_diagnostics::Label {
                    span,
                    message: "listed as dynamic here".to_owned(),
                });
            }
        }
    }

    fn build_system(ctx: &mut RuleContext<'_>, build: &Table) {
        check_shape(
            ctx,
            build,
            "build-system.requires",
            "requires",
            Shape::StrArray,
        );
        Self::requirements(ctx, build, "requires");
        check_shape(
            ctx,
            build,
            "build-system.build-backend",
            "build-backend",
            Shape::Str,
        );
        check_shape(
            ctx,
            build,
            "build-system.backend-path",
            "backend-path",
            Shape::StrArray,
        );
        if string(build, "build-backend").is_some_and(str::is_empty) {
            let span = build.value_span("build-backend").unwrap_or_default();
            ctx.report("`build-system.build-backend` is empty", span);
        }
    }

    /// `[dependency-groups]`: each group is a list of requirements and
    /// `{ include-group = "name" }` tables.
    fn dependency_groups(ctx: &mut RuleContext<'_>, groups: &Table) {
        for (group, value) in &groups.entries {
            let path = format!("dependency-groups.{group}");
            if !matches!(value, Value::Array(_)) {
                check_shape(ctx, groups, &path, group, Shape::StrArray);
                continue;
            }
            for (item, span) in items(ctx, groups, group) {
                match item {
                    Value::String(text) => {
                        if let Err(why) = check_requirement(text) {
                            ctx.report(format!("invalid requirement `{text}`: {why}"), span)
                                .help = Some("write it like `name>=1.0` (PEP 508)".to_owned());
                        }
                    }
                    Value::Table(include)
                        if include.entries.len() == 1
                            && string(include, "include-group").is_some() => {}
                    other => {
                        ctx.report(
                            format!(
                                "an item of `{path}` must be a requirement or `{{ include-group = \"...\" }}`, found {}",
                                type_name(other)
                            ),
                            span,
                        );
                    }
                }
            }
        }
    }
}

/// Number of items of an array value.
fn item_count(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.len(),
        Value::ArrayOfTables(tables) => tables.len(),
        _ => 0,
    }
}

impl Rule for PyprojectInvalidValue {
    fn meta(&self) -> &'static RuleMeta {
        &PYPROJECT_INVALID_VALUE
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_pyproject(ctx) {
            return;
        }
        let root = ctx.model;
        if let Some(project) = table(root, "project") {
            let poetry = table(root, "build-system")
                .and_then(|build| string(build, "build-backend"))
                .is_some_and(|backend| backend.starts_with("poetry."));
            Self::project(ctx, project, poetry);
        }
        if let Some(build) = table(root, "build-system") {
            Self::build_system(ctx, build);
        }
        if let Some(groups) = table(root, "dependency-groups") {
            Self::dependency_groups(ctx, groups);
        }
    }
}
