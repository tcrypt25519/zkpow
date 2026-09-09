//! Nextest filter-expression plumbing shared by the runner and the cargo plugin.

use guppy::{
    PackageId,
    graph::{PackageGraph, cargo::BuildPlatform},
};
use nextest_filtering::{
    BinaryQuery, CompiledExpr, EvalContext, Filterset, FiltersetKind, KnownGroups, ParseContext, TestQuery,
};
use nextest_metadata::{RustBinaryId, RustTestBinaryKind, TestCaseName};

/// Cargo target kind assumed when the artifact metadata does not name one.
pub const DEFAULT_TARGET_KIND: &str = "test";

pub fn parse(expr: &str, graph: &PackageGraph) -> Result<Filterset, String> {
    let parse_ctx = ParseContext::new(graph);
    let known_groups = KnownGroups::Known {
        custom_groups: std::collections::HashSet::new(),
    };
    Filterset::parse(expr.to_string(), &parse_ctx, FiltersetKind::Test, &known_groups).map_err(|err| {
        let messages = err.errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
        format!("invalid filter expression '{}': {messages}", err.input)
    })
}

/// One compiled binary, in the form a filterset is evaluated against.
pub struct Binary {
    package_id: PackageId,
    binary_id:  RustBinaryId,
    name:       String,
    kind:       RustTestBinaryKind,
}

impl Binary {
    pub fn new(package_name: &str, package_id: PackageId, name: String, kind: RustTestBinaryKind) -> Self {
        let binary_id = RustBinaryId::from_parts(package_name, &kind, &name);
        Self {
            package_id,
            binary_id,
            name,
            kind,
        }
    }

    fn query(&self) -> BinaryQuery<'_> {
        BinaryQuery {
            package_id:  &self.package_id,
            binary_id:   &self.binary_id,
            binary_name: &self.name,
            kind:        &self.kind,
            platform:    BuildPlatform::Target,
        }
    }

    /// `false` only when the filterset rules this binary out entirely; an indeterminate
    /// result keeps it, since individual test names may still match.
    pub fn can_match(&self, filterset: &Filterset) -> bool {
        let eval_cx = EvalContext {
            default_filter: &CompiledExpr::ALL,
        };
        filterset.matches_binary(&self.query(), &eval_cx) != Some(false)
    }

    pub fn matches_test(&self, filterset: &Filterset, test_name: &str) -> bool {
        let eval_cx = EvalContext {
            default_filter: &CompiledExpr::ALL,
        };
        let test_name = TestCaseName::new(test_name);
        let query = TestQuery {
            binary_query: self.query(),
            test_name:    &test_name,
        };
        filterset.matches_test(&query, &eval_cx)
    }
}
