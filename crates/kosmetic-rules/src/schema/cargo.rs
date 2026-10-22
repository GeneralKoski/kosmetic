//! Rules for `Cargo.toml`, following the manifest reference of Cargo.

use kosmetic_diagnostics::Span;
use kosmetic_syntax::{NodeKind, Table, Value};

use super::semver::{is_version, is_version_req, is_wildcard_req};
use super::{Shape, check_shape, report_unknown_keys, string, table, tables, type_name};
use crate::{FileKind, Level, Rule, RuleContext, RuleMeta};

const ROOT_KEYS: &[&str] = &[
    "cargo-features",
    "package",
    "project",
    "lib",
    "bin",
    "example",
    "test",
    "bench",
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "dev_dependencies",
    "build_dependencies",
    "target",
    "features",
    "workspace",
    "profile",
    "patch",
    "replace",
    "lints",
    "badges",
    "hints",
];

/// Keys of `[package]` with the shape of their value and whether the value
/// can be inherited from the workspace with `{ workspace = true }`.
const PACKAGE_FIELDS: &[(&str, Shape, bool)] = &[
    ("name", Shape::Str, false),
    ("version", Shape::Str, true),
    ("authors", Shape::StrArray, true),
    ("edition", Shape::Str, true),
    ("rust-version", Shape::Str, true),
    ("description", Shape::Str, true),
    ("documentation", Shape::Str, true),
    ("readme", Shape::StrOrBool, true),
    ("homepage", Shape::Str, true),
    ("repository", Shape::Str, true),
    ("license", Shape::Str, true),
    ("license-file", Shape::Str, true),
    ("keywords", Shape::StrArray, true),
    ("categories", Shape::StrArray, true),
    ("workspace", Shape::Str, false),
    ("build", Shape::StrOrBool, false),
    ("links", Shape::Str, false),
    ("exclude", Shape::StrArray, true),
    ("include", Shape::StrArray, true),
    ("publish", Shape::BoolOrStrArray, true),
    ("default-run", Shape::Str, false),
    ("autolib", Shape::Bool, false),
    ("autobins", Shape::Bool, false),
    ("autoexamples", Shape::Bool, false),
    ("autotests", Shape::Bool, false),
    ("autobenches", Shape::Bool, false),
    ("resolver", Shape::Str, false),
];

const PACKAGE_EXTRA_KEYS: &[&str] = &["metadata", "im-a-teapot"];

const WORKSPACE_KEYS: &[&str] = &[
    "members",
    "exclude",
    "default-members",
    "resolver",
    "metadata",
    "package",
    "dependencies",
    "lints",
];

const WORKSPACE_PACKAGE_KEYS: &[&str] = &[
    "authors",
    "categories",
    "description",
    "documentation",
    "edition",
    "exclude",
    "homepage",
    "include",
    "keywords",
    "license",
    "license-file",
    "publish",
    "readme",
    "repository",
    "rust-version",
    "version",
];

/// `[lib]` and `[[bin]]`, `[[example]]`, `[[test]]`, `[[bench]]`.
const TARGET_FIELDS: &[(&str, Shape)] = &[
    ("name", Shape::Str),
    ("path", Shape::Str),
    ("crate-type", Shape::StrArray),
    ("crate_type", Shape::StrArray),
    ("test", Shape::Bool),
    ("doctest", Shape::Bool),
    ("bench", Shape::Bool),
    ("doc", Shape::Bool),
    ("plugin", Shape::Bool),
    ("proc-macro", Shape::Bool),
    ("proc_macro", Shape::Bool),
    ("harness", Shape::Bool),
    ("edition", Shape::Str),
    ("required-features", Shape::StrArray),
    ("doc-scrape-examples", Shape::Bool),
];

const TARGET_SECTIONS: &[&str] = &["bin", "example", "test", "bench"];

const PROFILE_KEYS: &[&str] = &[
    "opt-level",
    "debug",
    "split-debuginfo",
    "strip",
    "debug-assertions",
    "overflow-checks",
    "lto",
    "panic",
    "incremental",
    "codegen-units",
    "rpath",
    "inherits",
    "dir-name",
    "build-override",
    "package",
    "trim-paths",
    "rustflags",
    "hint-mostly-unused",
];

const DEPENDENCY_FIELDS: &[(&str, Shape)] = &[
    ("version", Shape::Str),
    ("path", Shape::Str),
    ("git", Shape::Str),
    ("branch", Shape::Str),
    ("tag", Shape::Str),
    ("rev", Shape::Str),
    ("registry", Shape::Str),
    ("registry-index", Shape::Str),
    ("package", Shape::Str),
    ("features", Shape::StrArray),
    ("optional", Shape::Bool),
    ("default-features", Shape::Bool),
    ("default_features", Shape::Bool),
    ("workspace", Shape::Bool),
    ("public", Shape::Bool),
    ("target", Shape::Str),
    ("lib", Shape::Bool),
    ("artifact", Shape::StrOrBool),
    ("base", Shape::Str),
];

const DEPENDENCY_SECTIONS: &[&str] = &[
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "dev_dependencies",
    "build_dependencies",
];

const EDITIONS: &[&str] = &["2015", "2018", "2021", "2024"];

/// A dependency and where it is declared.
struct Dependency<'a> {
    name: &'a str,
    value: &'a Value,
    key_span: Span,
    value_span: Span,
}

/// The tables that map a dependency name to its definition: the three
/// `dependencies` sections, the platform specific ones, `workspace.dependencies`
/// and the `[patch]` registries.
fn dependency_tables(root: &Table) -> Vec<&Table> {
    fn sections<'a>(parent: &'a Table, found: &mut Vec<&'a Table>) {
        for name in DEPENDENCY_SECTIONS {
            found.extend(table(parent, name));
        }
    }

    let mut found = Vec::new();
    sections(root, &mut found);
    if let Some(workspace) = table(root, "workspace") {
        found.extend(table(workspace, "dependencies"));
    }
    if let Some(target) = table(root, "target") {
        for platform in target.entries.values() {
            if let Value::Table(platform) = platform {
                sections(platform, &mut found);
            }
        }
    }
    if let Some(patch) = table(root, "patch") {
        for registry in patch.entries.values() {
            if let Value::Table(registry) = registry {
                found.push(registry);
            }
        }
    }
    found
}

fn dependencies(root: &Table) -> Vec<Dependency<'_>> {
    dependency_tables(root)
        .into_iter()
        .flat_map(|section| {
            section.entries.iter().map(move |(name, value)| Dependency {
                name,
                value,
                key_span: section.key_span(name).unwrap_or_default(),
                value_span: section.value_span(name).unwrap_or_default(),
            })
        })
        .collect()
}

/// Key spans of the `[[name]]` headers, in file order.
fn array_header_spans(ctx: &RuleContext<'_>, name: &str) -> Vec<Span> {
    ctx.parse
        .tree
        .nodes()
        .filter(|node| node.kind == NodeKind::ArrayTable)
        .filter_map(|node| node.nodes().next()?.child(NodeKind::Key))
        .filter(|key| key.text(ctx.source) == name)
        .map(|key| key.span)
        .collect()
}

fn is_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn is_rust_version(text: &str) -> bool {
    let parts: Vec<_> = text.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn inherits(value: &Value) -> bool {
    matches!(value, Value::Table(table) if table.entries.contains_key("workspace"))
}

fn is_cargo(ctx: &RuleContext<'_>) -> bool {
    ctx.file == FileKind::CargoManifest
}

// ---------------------------------------------------------------------------

/// Flags keys Cargo does not know, with a suggestion when it looks like a typo.
pub struct CargoUnknownKey;

static CARGO_UNKNOWN_KEY: RuleMeta = RuleMeta {
    code: "schema/cargo-unknown-key",
    description: "Disallow keys that Cargo does not know in Cargo.toml",
    default_level: Level::Warn,
};

impl Rule for CargoUnknownKey {
    fn meta(&self) -> &'static RuleMeta {
        &CARGO_UNKNOWN_KEY
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_cargo(ctx) {
            return;
        }
        let root = ctx.model;
        report_unknown_keys(ctx, root, "the manifest", ROOT_KEYS);

        if let Some(package) = table(root, "package") {
            let known: Vec<_> = PACKAGE_FIELDS
                .iter()
                .map(|(key, _, _)| *key)
                .chain(PACKAGE_EXTRA_KEYS.iter().copied())
                .collect();
            report_unknown_keys(ctx, package, "`[package]`", &known);
        }
        if let Some(workspace) = table(root, "workspace") {
            report_unknown_keys(ctx, workspace, "`[workspace]`", WORKSPACE_KEYS);
            if let Some(package) = table(workspace, "package") {
                report_unknown_keys(
                    ctx,
                    package,
                    "`[workspace.package]`",
                    WORKSPACE_PACKAGE_KEYS,
                );
            }
        }

        let target_keys: Vec<_> = TARGET_FIELDS.iter().map(|(key, _)| *key).collect();
        if let Some(lib) = table(root, "lib") {
            report_unknown_keys(ctx, lib, "`[lib]`", &target_keys);
        }
        for section in TARGET_SECTIONS {
            for target in root.entries.get(*section).map_or_else(Vec::new, tables) {
                report_unknown_keys(ctx, target, &format!("`[[{section}]]`"), &target_keys);
            }
        }

        if let Some(profiles) = table(root, "profile") {
            for (name, profile) in &profiles.entries {
                let Value::Table(profile) = profile else {
                    continue;
                };
                let place = format!("`[profile.{name}]`");
                report_unknown_keys(ctx, profile, &place, PROFILE_KEYS);
                if let Some(overrides) = table(profile, "build-override") {
                    let place = format!("`[profile.{name}.build-override]`");
                    report_unknown_keys(ctx, overrides, &place, PROFILE_KEYS);
                }
            }
        }

        let dependency_keys: Vec<_> = DEPENDENCY_FIELDS.iter().map(|(key, _)| *key).collect();
        for dependency in dependencies(root) {
            if let Value::Table(details) = dependency.value {
                let place = format!("dependency `{}`", dependency.name);
                report_unknown_keys(ctx, details, &place, &dependency_keys);
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Flags what Cargo needs and the manifest lacks: a package name, a name for
/// each target, a source for each dependency.
pub struct CargoMissingField;

static CARGO_MISSING_FIELD: RuleMeta = RuleMeta {
    code: "schema/cargo-missing-field",
    description: "Require the fields Cargo needs in Cargo.toml",
    default_level: Level::Error,
};

impl Rule for CargoMissingField {
    fn meta(&self) -> &'static RuleMeta {
        &CARGO_MISSING_FIELD
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_cargo(ctx) {
            return;
        }
        let root = ctx.model;
        let has = |key| root.entries.contains_key(key);
        if !has("package") && !has("project") && !has("workspace") {
            ctx.report(
                "the manifest has neither `[package]` nor `[workspace]`",
                Span::empty(0),
            )
            .help = Some("add a `[package]` with a name, or a `[workspace]`".to_owned());
        }

        if let Some(package) = table(root, "package") {
            if !package.entries.contains_key("name") {
                let span = root.key_span("package").unwrap_or_default();
                ctx.report("`[package]` is missing `name`", span);
            }
        }

        for section in TARGET_SECTIONS {
            let Some(value) = root.entries.get(*section) else {
                continue;
            };
            let headers = array_header_spans(ctx, section);
            let fallback = root.key_span(section).unwrap_or_default();
            for (i, target) in tables(value).into_iter().enumerate() {
                if !target.entries.contains_key("name") {
                    let span = headers.get(i).copied().unwrap_or(fallback);
                    ctx.report(format!("`[[{section}]]` is missing `name`"), span);
                }
            }
        }

        for dependency in dependencies(root) {
            let Value::Table(details) = dependency.value else {
                continue;
            };
            let sources = ["version", "path", "git", "workspace"];
            if !sources.iter().any(|key| details.entries.contains_key(*key)) {
                ctx.report(
                    format!(
                        "dependency `{}` must specify one of `version`, `path`, `git` or `workspace`",
                        dependency.name
                    ),
                    dependency.key_span,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Flags values Cargo rejects: wrong types, bad versions, unknown editions
/// and contradictory dependency sources.
pub struct CargoInvalidValue;

static CARGO_INVALID_VALUE: RuleMeta = RuleMeta {
    code: "schema/cargo-invalid-value",
    description: "Disallow values that Cargo rejects in Cargo.toml",
    default_level: Level::Error,
};

impl CargoInvalidValue {
    /// `[package]` or `[workspace.package]`.
    fn package_fields(ctx: &mut RuleContext<'_>, package: &Table, prefix: &str, shared: bool) {
        for (key, shape, inheritable) in PACKAGE_FIELDS {
            let Some(value) = package.entries.get(*key) else {
                continue;
            };
            if shared && !WORKSPACE_PACKAGE_KEYS.contains(key) {
                continue;
            }
            if *inheritable && !shared && inherits(value) {
                continue;
            }
            let path = format!("{prefix}.{key}");
            if !check_shape(ctx, package, &path, key, *shape) {
                continue;
            }
            let span = package.value_span(key).unwrap_or_default();
            let Some(text) = string(package, key) else {
                continue;
            };
            match *key {
                "name" if !is_package_name(text) => {
                    ctx.report(format!("`{text}` is not a valid package name"), span)
                        .help = Some(
                        "a name starts with a letter or `_` and has only letters, digits, `-` and `_`"
                            .to_owned(),
                    );
                }
                "version" if !is_version(text) => {
                    ctx.report(format!("`{text}` is not a valid semantic version"), span)
                        .help = Some("use `MAJOR.MINOR.PATCH`, for example \"0.1.0\"".to_owned());
                }
                "edition" if !EDITIONS.contains(&text) => {
                    ctx.report(format!("unknown edition `{text}`"), span).help =
                        Some(format!("editions are {}", EDITIONS.join(", ")));
                }
                "rust-version" if !is_rust_version(text) => {
                    ctx.report(format!("`{text}` is not a valid `rust-version`"), span)
                        .help = Some("use `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`".to_owned());
                }
                "resolver" if !["1", "2", "3"].contains(&text) => {
                    ctx.report(format!("unknown resolver `{text}`"), span).help =
                        Some("resolvers are \"1\", \"2\" and \"3\"".to_owned());
                }
                _ => {}
            }
        }
    }

    fn target(ctx: &mut RuleContext<'_>, target: &Table, place: &str) {
        for (key, shape) in TARGET_FIELDS {
            let path = format!("{place}.{key}");
            if !check_shape(ctx, target, &path, key, *shape) {
                continue;
            }
            if *key == "edition" {
                if let Some(text) = string(target, key).filter(|text| !EDITIONS.contains(text)) {
                    let span = target.value_span(key).unwrap_or_default();
                    ctx.report(format!("unknown edition `{text}`"), span).help =
                        Some(format!("editions are {}", EDITIONS.join(", ")));
                }
            }
        }
    }

    fn dependency(ctx: &mut RuleContext<'_>, dependency: &Dependency<'_>) {
        let name = dependency.name;
        match dependency.value {
            Value::String(requirement) => {
                if !is_version_req(requirement) {
                    ctx.report(
                        format!(
                            "invalid version requirement `{requirement}` for dependency `{name}`"
                        ),
                        dependency.value_span,
                    )
                    .help = Some("use a requirement like \"1.2\" or \">=1.2, <2\"".to_owned());
                }
            }
            Value::Table(details) => Self::dependency_details(ctx, name, details),
            other => {
                ctx.report(
                    format!(
                        "dependency `{name}` must be a version string or a table, found {}",
                        type_name(other)
                    ),
                    dependency.value_span,
                );
            }
        }
    }

    fn dependency_details(ctx: &mut RuleContext<'_>, name: &str, details: &Table) {
        let mut well_typed = true;
        for (key, shape) in DEPENDENCY_FIELDS {
            let path = format!("{name}.{key}");
            well_typed &= check_shape(ctx, details, &path, key, *shape);
        }
        if !well_typed {
            return;
        }
        let set = |key: &str| details.entries.contains_key(key);
        let key_span = |key: &str| details.key_span(key).unwrap_or_default();

        if let Some(requirement) = string(details, "version") {
            if !is_version_req(requirement) {
                ctx.report(
                    format!("invalid version requirement `{requirement}` for dependency `{name}`"),
                    details.value_span("version").unwrap_or_default(),
                )
                .help = Some("use a requirement like \"1.2\" or \">=1.2, <2\"".to_owned());
            }
        }
        if set("git") && set("path") {
            let diagnostic = ctx.report(
                format!("dependency `{name}` cannot have both `git` and `path`"),
                key_span("path"),
            );
            diagnostic.labels.push(kosmetic_diagnostics::Label {
                span: key_span("git"),
                message: "`git` is set here".to_owned(),
            });
        }
        let pins: Vec<_> = ["branch", "tag", "rev"]
            .into_iter()
            .filter(|key| set(key))
            .collect();
        if !set("git") {
            for key in &pins {
                ctx.report(
                    format!("`{key}` of dependency `{name}` requires `git`"),
                    key_span(key),
                );
            }
        }
        if pins.len() > 1 {
            ctx.report(
                format!("dependency `{name}` can have only one of `branch`, `tag` and `rev`"),
                key_span(pins[1]),
            );
        }
        if details.entries.get("workspace") == Some(&Value::Boolean(true)) {
            for key in ["version", "path", "git", "registry"] {
                if set(key) {
                    ctx.report(
                        format!("`workspace = true` of dependency `{name}` cannot be combined with `{key}`"),
                        key_span(key),
                    );
                }
            }
        }
    }
}

impl Rule for CargoInvalidValue {
    fn meta(&self) -> &'static RuleMeta {
        &CARGO_INVALID_VALUE
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_cargo(ctx) {
            return;
        }
        let root = ctx.model;

        if let Some(package) = table(root, "package") {
            Self::package_fields(ctx, package, "package", false);
        }
        if let Some(workspace) = table(root, "workspace") {
            for key in ["members", "exclude", "default-members"] {
                check_shape(
                    ctx,
                    workspace,
                    &format!("workspace.{key}"),
                    key,
                    Shape::StrArray,
                );
            }
            if let Some(package) = table(workspace, "package") {
                Self::package_fields(ctx, package, "workspace.package", true);
            }
        }

        if let Some(lib) = table(root, "lib") {
            Self::target(ctx, lib, "lib");
        }
        for section in TARGET_SECTIONS {
            for target in root.entries.get(*section).map_or_else(Vec::new, tables) {
                Self::target(ctx, target, section);
            }
        }

        if let Some(features) = table(root, "features") {
            for key in features.entries.keys() {
                check_shape(
                    ctx,
                    features,
                    &format!("features.{key}"),
                    key,
                    Shape::StrArray,
                );
            }
        }

        for dependency in dependencies(root) {
            Self::dependency(ctx, &dependency);
        }
    }
}

// ---------------------------------------------------------------------------

/// Flags dependencies that accept any version, `"*"`. crates.io refuses to
/// publish them.
pub struct CargoWildcardDependency;

static CARGO_WILDCARD_DEPENDENCY: RuleMeta = RuleMeta {
    code: "schema/cargo-wildcard-dependency",
    description: "Disallow `*` as the version requirement of a dependency",
    default_level: Level::Warn,
};

impl Rule for CargoWildcardDependency {
    fn meta(&self) -> &'static RuleMeta {
        &CARGO_WILDCARD_DEPENDENCY
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_cargo(ctx) {
            return;
        }
        for dependency in dependencies(ctx.model) {
            let (requirement, span) = match dependency.value {
                Value::String(requirement) => (requirement.as_str(), dependency.value_span),
                Value::Table(details) => match string(details, "version") {
                    Some(requirement) => (
                        requirement,
                        details.value_span("version").unwrap_or_default(),
                    ),
                    None => continue,
                },
                _ => continue,
            };
            if is_wildcard_req(requirement) {
                ctx.report(
                    format!("dependency `{}` accepts any version", dependency.name),
                    span,
                )
                .help = Some("pin a version, for example \"1\"".to_owned());
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Checks what crates.io requires to publish a package: a description, a
/// license, at most five keywords and categories. Off by default, since most
/// manifests are never published.
pub struct CargoPublishMetadata;

static CARGO_PUBLISH_METADATA: RuleMeta = RuleMeta {
    code: "schema/cargo-publish-metadata",
    description: "Require the metadata crates.io needs to publish a package",
    default_level: Level::Off,
};

impl Rule for CargoPublishMetadata {
    fn meta(&self) -> &'static RuleMeta {
        &CARGO_PUBLISH_METADATA
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        if !is_cargo(ctx) {
            return;
        }
        let root = ctx.model;
        let Some(package) = table(root, "package") else {
            return;
        };
        if package
            .entries
            .get("publish")
            .is_some_and(|value| !inherits(value) && *value != Value::Boolean(true))
        {
            return;
        }
        let package_span = root.key_span("package").unwrap_or_default();
        let has = |key: &str| package.entries.contains_key(key);

        if !has("description") {
            ctx.report(
                "`[package]` has no `description`, crates.io requires it",
                package_span,
            );
        }
        if !has("license") && !has("license-file") {
            ctx.report(
                "`[package]` has no `license` or `license-file`, crates.io requires one",
                package_span,
            );
        }
        for key in ["keywords", "categories"] {
            let Some(Value::Array(items)) = package.entries.get(key) else {
                continue;
            };
            let span = package.value_span(key).unwrap_or_default();
            if items.len() > 5 {
                ctx.report(
                    format!("crates.io accepts at most 5 `{key}`, found {}", items.len()),
                    span,
                );
            }
            if key == "keywords" {
                for item in items {
                    let Value::String(keyword) = item else {
                        continue;
                    };
                    let valid = keyword.chars().count() <= 20
                        && keyword.chars().next().is_some_and(char::is_alphanumeric)
                        && keyword
                            .chars()
                            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '+'));
                    if !valid {
                        ctx.report(format!("`{keyword}` is not a valid crates.io keyword"), span)
                            .help = Some(
                            "keywords have at most 20 characters: letters, digits, `_`, `-` and `+`"
                                .to_owned(),
                        );
                    }
                }
            }
        }
    }
}
