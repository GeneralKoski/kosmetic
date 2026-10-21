//! Semantic rules: valid TOML that probably does not mean what was intended.

use kosmetic_diagnostics::Label;
use kosmetic_syntax::NodeKind;

use crate::{Level, Rule, RuleContext, RuleMeta};

/// Flags arrays whose elements have different types, like `[1, "2"]`. Valid
/// since TOML 1.0, but usually a typo, and many consumers reject it.
pub struct MixedArrayTypes;

static MIXED_ARRAY_TYPES: RuleMeta = RuleMeta {
    code: "semantic/mixed-array-types",
    description: "Disallow arrays whose elements have different types",
    default_level: Level::Warn,
};

fn type_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::String => "string",
        NodeKind::Integer => "integer",
        NodeKind::Float => "float",
        NodeKind::Boolean => "boolean",
        NodeKind::DateTime => "date-time",
        NodeKind::Array => "array",
        NodeKind::InlineTable => "inline table",
        _ => "invalid",
    }
}

impl Rule for MixedArrayTypes {
    fn meta(&self) -> &'static RuleMeta {
        &MIXED_ARRAY_TYPES
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        for array in ctx.parse.tree.descendants() {
            if array.kind != NodeKind::Array {
                continue;
            }
            let mut values = array.nodes().filter(|node| node.kind.is_value());
            let Some(first) = values.next() else {
                continue;
            };
            if let Some(other) = values.find(|value| value.kind != first.kind) {
                let message = format!(
                    "array mixes {} and {} values",
                    type_name(first.kind),
                    type_name(other.kind)
                );
                ctx.report(message, other.span).labels.push(Label {
                    span: first.span,
                    message: format!("the first element is a {}", type_name(first.kind)),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kosmetic_diagnostics::Span;

    use crate::Level;
    use crate::testing::lint;

    #[test]
    fn mixed_array_types() {
        let code = "semantic/mixed-array-types";
        let diagnostics = lint(code, "a = [1, 2, \"3\", 4.0]\n", None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "array mixes integer and string values"
        );
        assert_eq!(diagnostics[0].span, Span::new(11, 14));
        assert_eq!(diagnostics[0].labels[0].span, Span::new(5, 6));
        assert_eq!(
            lint(
                code,
                "a = [[1], [\"x\"], []]\nb = [{x = 1}, {}]\nc = []\n",
                None
            ),
            Vec::new()
        );
        assert_eq!(lint(code, "a = [1, 2.0]\n", Some(Level::Off)), Vec::new());
    }
}
