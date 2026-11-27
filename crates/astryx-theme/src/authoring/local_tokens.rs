//! A theme's local tokens and their contract: upstream `theme/localTokens.ts`.
//!
//! Theme-local tokens are custom properties of a theme family's own (any valid
//! custom-property name; a prefix does not establish ownership), declared in a
//! definition's `localTokens` rather than its `tokens`. A theme that declares
//! them, or extends one that did, is enrolled: every local token records the
//! theme that first declared it, and the theme records its lineage, so a
//! child may replace an inherited value but the names stay apart from the
//! Astryx tokens. Themes that never enroll keep the legacy behavior.
//!
//! [`assert_no_token_cycles`] refuses tokens that read each other.

use std::collections::{HashMap, HashSet};

use super::define::{CssMap, DefineThemeError, DefinedTheme, TokenInputs};
use super::js::is_js_whitespace;
use super::resolve_values::resolve_token_value;

/// A theme's resolved local-token contract (upstream
/// `ResolvedLocalTokenContract`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedLocalTokenContract {
    /// Every local token, inherited ones first, with its CSS value.
    pub local_tokens: CssMap,
    /// The theme that first declared each local token.
    pub owners: CssMap,
    /// The enrolled lineage, base first and this theme last.
    pub lineage: Vec<String>,
}

/// A character of a CSS name (upstream `CSS_NAME_CHARACTER`,
/// `(?:[-_a-zA-Z0-9]|\P{ASCII}|\\(?:[0-9a-fA-F]{1,6}[ \t\r\n\f]?|[^\r\n\f]))`)
/// at the start of `chars`: the number of characters it takes, or `None`.
/// The escape takes up to six hex digits and one whitespace after them, or one
/// character that is not a newline; taking the longest match is what the
/// pattern accepts.
fn name_character(chars: &[char]) -> Option<usize> {
    let first = *chars.first()?;
    if first.is_ascii_alphanumeric() || first == '-' || first == '_' || !first.is_ascii() {
        return Some(1);
    }
    if first != '\\' {
        return None;
    }
    let next = *chars.get(1)?;
    if next.is_ascii_hexdigit() {
        let digits = chars[1..]
            .iter()
            .take(6)
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        let after = 1 + digits;
        let space = chars
            .get(after)
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}'));
        return Some(after + usize::from(space));
    }
    if matches!(next, '\r' | '\n' | '\u{c}') {
        return None;
    }
    Some(2)
}

/// The length of the longest run of CSS name characters at the start of
/// `chars`, and the positions where each character ends.
fn name_run(chars: &[char]) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut at = 0;
    while let Some(taken) = name_character(&chars[at..]) {
        at += taken;
        ends.push(at);
    }
    ends
}

/// Whether `name` is a valid CSS custom-property name: `--` and CSS name
/// characters, with no NUL. Upstream `isValidCSSCustomPropertyName`.
pub fn is_valid_css_custom_property_name(name: &str) -> bool {
    if name.contains('\0') {
        return false;
    }
    let Some(rest) = name.strip_prefix("--") else {
        return false;
    };
    let chars: Vec<char> = rest.chars().collect();
    name_run(&chars).last().copied().unwrap_or(0) == chars.len()
}

/// Every custom property a CSS value references with `var(` (in any letter
/// case, upstream's `CSS_VAR_PATTERN` with its `i` flag): the name after
/// optional whitespace, where a comma or `)` follows it after optional
/// whitespace. Each name once, in order. Upstream
/// `collectCustomPropertyReferences` on a string.
fn collect_custom_property_references(value: &str, refs: &mut Vec<String>) {
    let chars: Vec<char> = value.chars().collect();
    let is_var = |at: usize| {
        chars.len() >= at + 4
            && chars[at].eq_ignore_ascii_case(&'v')
            && chars[at + 1].eq_ignore_ascii_case(&'a')
            && chars[at + 2].eq_ignore_ascii_case(&'r')
            && chars[at + 3] == '('
    };
    let mut at = 0;
    while at < chars.len() {
        if !is_var(at) {
            at += 1;
            continue;
        }
        let mut start = at + 4;
        while start < chars.len() && is_js_whitespace(chars[start]) {
            start += 1;
        }
        let matched = (chars.get(start) == Some(&'-') && chars.get(start + 1) == Some(&'-'))
            .then(|| {
                let name_start = start + 2;
                let ends = name_run(&chars[name_start..]);
                // The longest name the lookahead accepts, as the pattern's
                // backtracking finds it.
                std::iter::once(0)
                    .chain(ends)
                    .rev()
                    .map(|end| name_start + end)
                    .find(|&end| {
                        let mut next = end;
                        while next < chars.len() && is_js_whitespace(chars[next]) {
                            next += 1;
                        }
                        matches!(chars.get(next), Some(',' | ')'))
                    })
            })
            .flatten();
        match matched {
            Some(end) => {
                let name: String = chars[start..end].iter().collect();
                if !refs.contains(&name) {
                    refs.push(name);
                }
                at = end;
            }
            None => at += 1,
        }
    }
}

/// The tokens each token of a map references, among the map's own.
struct TokenGraph<'a> {
    dependencies: HashMap<&'a str, Vec<String>>,
}

impl TokenGraph<'_> {
    /// The tokens `name` references.
    fn deps(&self, name: &str) -> &[String] {
        self.dependencies.get(name).map_or(&[], Vec::as_slice)
    }

    /// A path from `start` back to itself through `component`, depth first in
    /// reference order. Upstream's `findCyclePath`.
    fn cycle_path(&self, start: &str, component: &HashSet<String>) -> Vec<String> {
        let mut path = vec![start.to_string()];
        let mut on_path: HashSet<String> = path.iter().cloned().collect();
        self.search(start, start, component, &mut path, &mut on_path);
        path
    }

    fn search(
        &self,
        name: &str,
        start: &str,
        component: &HashSet<String>,
        path: &mut Vec<String>,
        on_path: &mut HashSet<String>,
    ) -> bool {
        for dependency in self.deps(name) {
            if !component.contains(dependency) {
                continue;
            }
            if dependency == start {
                path.push(start.to_string());
                return true;
            }
            if on_path.contains(dependency) {
                continue;
            }
            path.push(dependency.clone());
            on_path.insert(dependency.clone());
            if self.search(dependency, start, component, path, on_path) {
                return true;
            }
            on_path.remove(dependency);
            path.pop();
        }
        false
    }
}

/// Tarjan's strongly connected components over a [`TokenGraph`], stopping at
/// the first component that is a cycle (and, with `relevant`, holds one of
/// those names).
struct Tarjan<'g, 'a> {
    graph: &'g TokenGraph<'a>,
    relevant: Option<&'g HashSet<String>>,
    next_index: usize,
    indexes: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    stack: Vec<String>,
    on_stack: HashSet<String>,
}

impl Tarjan<'_, '_> {
    /// Visits `name`: the members of the first cycle found.
    fn visit(&mut self, name: &str) -> Option<Vec<String>> {
        let index = self.next_index;
        self.next_index += 1;
        self.indexes.insert(name.to_string(), index);
        self.lowlinks.insert(name.to_string(), index);
        self.stack.push(name.to_string());
        self.on_stack.insert(name.to_string());

        let graph = self.graph;
        for dependency in graph.deps(name) {
            let low = self.lowlinks.get(name).copied().unwrap_or(index);
            if !self.indexes.contains_key(dependency) {
                if let Some(found) = self.visit(dependency) {
                    return Some(found);
                }
                let low = self.lowlinks.get(name).copied().unwrap_or(index);
                let theirs = self.lowlinks.get(dependency).copied().unwrap_or(0);
                self.lowlinks.insert(name.to_string(), low.min(theirs));
            } else if self.on_stack.contains(dependency) {
                let theirs = self.indexes.get(dependency).copied().unwrap_or(0);
                self.lowlinks.insert(name.to_string(), low.min(theirs));
            }
        }

        if self.lowlinks.get(name) != Some(&index) {
            return None;
        }

        let mut component = Vec::new();
        loop {
            let member = self.stack.pop().unwrap_or_else(|| name.to_string());
            self.on_stack.remove(&member);
            let done = member == name;
            component.push(member);
            if done {
                break;
            }
        }

        let has_cycle = component.len() > 1 || graph.deps(&component[0]).contains(&component[0]);
        let concerns = self
            .relevant
            .is_none_or(|relevant| component.iter().any(|member| relevant.contains(member)));
        (has_cycle && concerns).then_some(component)
    }
}

/// Fails when tokens of `token_values` read each other (upstream
/// `assertNoTokenCycles`), naming the first cycle found, as `a -> b -> a`.
///
/// A token depends on the tokens of the map its value references with `var()`
/// (in any letter case). The strongly connected components are found in key
/// order (Tarjan's algorithm); with `relevant_names`, only a cycle through one
/// of those names fails, and its path starts there. `context` prefixes the
/// message. Upstream also refuses a value that holds itself (an object inside
/// itself), which a string cannot, and takes `contextOf` for that message
/// alone.
pub fn assert_no_token_cycles(
    token_values: &CssMap,
    context: Option<&str>,
    relevant_names: Option<&HashSet<String>>,
) -> Result<(), DefineThemeError> {
    let mut dependencies = HashMap::new();
    for (name, value) in token_values.iter() {
        let mut refs = Vec::new();
        collect_custom_property_references(value, &mut refs);
        refs.retain(|reference| token_values.contains_key(reference));
        dependencies.insert(name, refs);
    }
    let graph = TokenGraph { dependencies };
    let mut tarjan = Tarjan {
        graph: &graph,
        relevant: relevant_names,
        next_index: 0,
        indexes: HashMap::new(),
        lowlinks: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
    };
    for name in token_values.keys() {
        if tarjan.indexes.contains_key(name) {
            continue;
        }
        let Some(component) = tarjan.visit(name) else {
            continue;
        };
        let start = component
            .iter()
            .find(|member| relevant_names.is_some_and(|relevant| relevant.contains(*member)))
            .unwrap_or(&component[0])
            .clone();
        let members: HashSet<String> = component.into_iter().collect();
        let cycle = graph.cycle_path(&start, &members).join(" -> ");
        let prefix = context
            .map(|context| format!("{context}: "))
            .unwrap_or_default();
        return Err(DefineThemeError(format!(
            "{prefix}Theme token cycle detected: {cycle}."
        )));
    }
    Ok(())
}

/// Fails unless a base's local-token metadata is whole and consistent: a
/// lineage ending with the base, local tokens and owners both present, every
/// token owned by a theme of the lineage and validly named, and every owner
/// entry naming a declared token. Upstream `assertInheritedContract`.
fn assert_inherited_contract(
    theme_name: &str,
    base: &DefinedTheme,
) -> Result<(), DefineThemeError> {
    let lineage = base.local_token_lineage.as_deref().unwrap_or_default();
    if lineage.last().map(String::as_str) != Some(base.name.as_str())
        || lineage.iter().any(String::is_empty)
    {
        return Err(DefineThemeError(format!(
            "defineTheme(\"{theme_name}\"): the base theme has invalid theme-local token lineage metadata."
        )));
    }
    let (Some(local_tokens), Some(owners)) = (&base.local_tokens, &base.local_token_owners) else {
        return Err(DefineThemeError(format!(
            "defineTheme(\"{theme_name}\"): the base theme has incomplete theme-local token metadata."
        )));
    };
    for name in local_tokens.keys() {
        let owner = owners.get(name).filter(|owner| !owner.is_empty());
        if !owner.is_some_and(|owner| lineage.contains(owner))
            || !is_valid_css_custom_property_name(name)
        {
            return Err(DefineThemeError(format!(
                "defineTheme(\"{theme_name}\"): inherited local token \"{name}\" does not match its exact lineage metadata."
            )));
        }
    }
    for name in owners.keys() {
        if !local_tokens.contains_key(name) {
            return Err(DefineThemeError(format!(
                "defineTheme(\"{theme_name}\"): inherited local-token owner metadata names undeclared token \"{name}\"."
            )));
        }
    }
    Ok(())
}

/// A theme's local-token contract, resolved and checked (upstream
/// `resolveLocalTokenContract`). `None` for a theme that declares no
/// `localTokens` and extends no enrolled base: such a theme keeps the legacy
/// behavior and none of these checks.
///
/// The base's local tokens and owners come first; each of this theme's own
/// declarations replaces an inherited value (its owner stays) or declares a
/// new token this theme owns, skipping [`TokenInput::Unset`](super::define::TokenInput::Unset)
/// values. Fails, with upstream's message,
/// on an invalid custom-property name, a local token also in `tokens` (the
/// theme's resolved tokens) or `portable_tokens` (core's token defaults), an
/// inherited token without an owner, inconsistent base metadata, or a
/// reference cycle among the local tokens.
pub fn resolve_local_token_contract(
    name: &str,
    declarations: Option<&TokenInputs>,
    base: Option<&DefinedTheme>,
    tokens: &CssMap,
    portable_tokens: &[(&str, &str)],
) -> Result<Option<ResolvedLocalTokenContract>, DefineThemeError> {
    let directly_enrolled = declarations.is_some();
    let inherited = base.is_some_and(|base| base.local_token_lineage.is_some());
    if !directly_enrolled && !inherited {
        return Ok(None);
    }
    if inherited && let Some(base) = base {
        assert_inherited_contract(name, base)?;
    }

    let mut local_tokens = base
        .and_then(|base| base.local_tokens.clone())
        .unwrap_or_default();
    let mut owners = base
        .and_then(|base| base.local_token_owners.clone())
        .unwrap_or_default();

    for (token, value) in declarations.into_iter().flat_map(TokenInputs::iter) {
        let Some(value) = value.value() else {
            continue;
        };
        let inherited_owner = owners.get(token).is_some_and(|owner| !owner.is_empty());
        if !is_valid_css_custom_property_name(token) {
            return Err(DefineThemeError(format!(
                "defineTheme(\"{name}\"): local token \"{token}\" must be a valid CSS custom-property name."
            )));
        }
        if !inherited_owner {
            owners.insert(token, name.to_string());
        }
        local_tokens.insert(token, resolve_token_value(value));
    }

    for token in local_tokens.keys() {
        if !owners.get(token).is_some_and(|owner| !owner.is_empty()) {
            return Err(DefineThemeError(format!(
                "defineTheme(\"{name}\"): inherited local token \"{token}\" has no owner metadata."
            )));
        }
        if tokens.contains_key(token) || portable_tokens.iter().any(|(key, _)| *key == token) {
            return Err(DefineThemeError(format!(
                "defineTheme(\"{name}\"): token \"{token}\" cannot be declared in both tokens and localTokens."
            )));
        }
    }

    assert_no_token_cycles(
        &local_tokens,
        Some(&format!("defineTheme(\"{name}\").localTokens")),
        None,
    )?;

    let mut lineage = base
        .and_then(|base| base.local_token_lineage.clone())
        .unwrap_or_default();
    lineage.push(name.to_string());
    Ok(Some(ResolvedLocalTokenContract {
        local_tokens,
        owners,
        lineage,
    }))
}

/// The local-token values one adaptation rule writes (upstream
/// `resolveAdaptationLocalTokens`): each must replace a name the root theme's
/// lineage already declares (`root_local_tokens`); a rule never enrolls one of
/// its own. `None` when the rule writes none. Cycles are judged once every
/// rule that can match together has cascaded.
pub fn resolve_adaptation_local_tokens(
    theme_name: &str,
    rule_index: usize,
    declarations: Option<&TokenInputs>,
    root_local_tokens: Option<&CssMap>,
) -> Result<Option<CssMap>, DefineThemeError> {
    let path =
        format!("defineTheme(\"{theme_name}\").adaptations.rules[{rule_index}].value.localTokens");
    let Some(declarations) = declarations else {
        return Ok(None);
    };
    let mut resolved = CssMap::new();
    for (name, value) in declarations.iter() {
        let Some(value) = value.value() else {
            continue;
        };
        if !root_local_tokens.is_some_and(|root| root.contains_key(name)) {
            return Err(DefineThemeError(format!(
                "{path}[\"{name}\"] cannot enroll a theme-local token. Declare it in the root theme or an exact enrolled base first."
            )));
        }
        resolved.insert(name, resolve_token_value(value));
    }
    Ok((!resolved.is_empty()).then_some(resolved))
}

#[cfg(test)]
mod tests {
    use super::super::define::TokenInput;
    use super::*;

    fn css_map<const N: usize>(entries: [(&str, &str); N]) -> CssMap {
        entries
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect()
    }

    fn cycle(tokens: CssMap) -> Option<String> {
        assert_no_token_cycles(&tokens, None, None)
            .err()
            .map(|e| e.0)
    }

    #[test]
    fn valid_custom_property_names_start_with_two_hyphens_and_hold_name_characters() {
        for name in [
            "--",
            "--a",
            "--ac-selection-ink",
            "--_internal",
            "--1",
            "--品牌",
            r"--\66 oo",
            r"--x\ y",
            r"--x\\y",
            r"--a\31 23",
        ] {
            assert!(is_valid_css_custom_property_name(name), "{name}");
        }
        for name in [
            "selection-ink",
            "-a",
            "--a b",
            "--a\0b",
            "--a\\\nb",
            "--a;",
            "--a\\",
        ] {
            assert!(!is_valid_css_custom_property_name(name), "{name:?}");
        }
    }

    #[test]
    fn tokens_that_read_each_other_are_a_cycle_named_by_its_path() {
        assert_eq!(
            cycle(css_map([("--a", "var(--b)"), ("--b", "VAR( --a )")])).as_deref(),
            Some("Theme token cycle detected: --b -> --a -> --b.")
        );
        assert_eq!(
            cycle(css_map([("--a", "var(--a, red)")])).as_deref(),
            Some("Theme token cycle detected: --a -> --a.")
        );
        // A fallback's reference counts as a dependency too.
        assert!(
            cycle(css_map([
                ("--a", "var(--x, var(--b))"),
                ("--b", "var(--a)")
            ]))
            .is_some()
        );
    }

    #[test]
    fn references_outside_the_map_or_without_a_closing_reference_are_no_cycle() {
        assert_eq!(
            cycle(css_map([("--a", "var(--b)"), ("--b", "var(--c)")])),
            None
        );
        // `var(--b x)` references nothing: the name must end at a comma or `)`.
        assert_eq!(
            cycle(css_map([("--a", "var(--b x)"), ("--b", "var(--a)")])),
            None
        );
        // Case-only differences are different names.
        assert_eq!(cycle(css_map([("--A", "var(--a)"), ("--a", "red")])), None);
    }

    #[test]
    fn with_relevant_names_only_a_cycle_through_one_of_them_fails() {
        let tokens = css_map([("--a", "var(--b)"), ("--b", "var(--a)"), ("--c", "red")]);
        let relevant: HashSet<String> = ["--c".to_string()].into();
        assert!(assert_no_token_cycles(&tokens, None, Some(&relevant)).is_ok());
        let relevant: HashSet<String> = ["--a".to_string()].into();
        assert_eq!(
            assert_no_token_cycles(&tokens, Some("rule [0]"), Some(&relevant))
                .unwrap_err()
                .0,
            "rule [0]: Theme token cycle detected: --a -> --b -> --a."
        );
    }

    fn base(name: &str) -> DefinedTheme {
        DefinedTheme {
            name: name.into(),
            built: true,
            local_tokens: Some(css_map([("--brand", "red")])),
            local_token_owners: Some(css_map([("--brand", name)])),
            local_token_lineage: Some(vec![name.to_string()]),
            ..DefinedTheme::default()
        }
    }

    fn contract(
        base: &DefinedTheme,
        declarations: Option<&TokenInputs>,
    ) -> Result<Option<ResolvedLocalTokenContract>, DefineThemeError> {
        resolve_local_token_contract("child", declarations, Some(base), &CssMap::new(), &[])
    }

    #[test]
    fn refuses_inconsistent_inherited_metadata() {
        let message = |base: DefinedTheme| contract(&base, None).unwrap_err().0;
        let mut wrong_lineage = base("base");
        wrong_lineage.local_token_lineage = Some(vec!["other".into()]);
        assert_eq!(
            message(wrong_lineage),
            "defineTheme(\"child\"): the base theme has invalid theme-local token lineage metadata."
        );
        let mut no_owners = base("base");
        no_owners.local_token_owners = None;
        assert_eq!(
            message(no_owners),
            "defineTheme(\"child\"): the base theme has incomplete theme-local token metadata."
        );
        let mut foreign_owner = base("base");
        foreign_owner.local_token_owners = Some(css_map([("--brand", "stranger")]));
        assert_eq!(
            message(foreign_owner),
            "defineTheme(\"child\"): inherited local token \"--brand\" does not match its exact lineage metadata."
        );
        let mut stray_owner = base("base");
        stray_owner.local_token_owners = Some(css_map([("--brand", "base"), ("--gone", "base")]));
        assert_eq!(
            message(stray_owner),
            "defineTheme(\"child\"): inherited local-token owner metadata names undeclared token \"--gone\"."
        );
    }

    #[test]
    fn refuses_inherited_local_tokens_without_owners() {
        // A base with local tokens but no lineage is not enrolled, so a child
        // that declares its own inherits the tokens without their owners.
        let mut unenrolled = base("base");
        unenrolled.local_token_lineage = None;
        unenrolled.local_token_owners = None;
        let own: TokenInputs = [("--mine", TokenInput::Css("blue".into()))].into();
        assert_eq!(
            contract(&unenrolled, Some(&own)).unwrap_err().0,
            "defineTheme(\"child\"): inherited local token \"--brand\" has no owner metadata."
        );
    }

    #[test]
    fn a_child_replaces_an_inherited_value_and_keeps_its_owner() {
        let own: TokenInputs = [
            ("--brand", TokenInput::Pair("#000".into(), "#fff".into())),
            ("--mine", TokenInput::Css("var(--brand)".into())),
        ]
        .into();
        let resolved = contract(&base("base"), Some(&own)).unwrap().unwrap();
        assert_eq!(
            resolved.local_tokens,
            css_map([
                ("--brand", "light-dark(#000, #fff)"),
                ("--mine", "var(--brand)")
            ])
        );
        assert_eq!(
            resolved.owners,
            css_map([("--brand", "base"), ("--mine", "child")])
        );
        assert_eq!(resolved.lineage, ["base", "child"]);
    }

    #[test]
    fn a_theme_that_never_enrolls_has_no_contract() {
        assert_eq!(
            resolve_local_token_contract("legacy", None, None, &CssMap::new(), &[]),
            Ok(None)
        );
    }

    #[test]
    fn an_adaptation_rule_may_only_replace_enrolled_local_tokens() {
        let root = css_map([("--brand", "red")]);
        let writes: TokenInputs = [
            ("--brand", TokenInput::Css("blue".into())),
            ("--unset", TokenInput::Unset),
        ]
        .into();
        assert_eq!(
            resolve_adaptation_local_tokens("lost-local", 0, Some(&writes), Some(&root)),
            Ok(Some(css_map([("--brand", "blue")])))
        );
        let enrolling: TokenInputs = [("--new", TokenInput::Css("blue".into()))].into();
        assert_eq!(
            resolve_adaptation_local_tokens("lost-local", 2, Some(&enrolling), Some(&root))
                .unwrap_err()
                .0,
            "defineTheme(\"lost-local\").adaptations.rules[2].value.localTokens[\"--new\"] cannot enroll a theme-local token. Declare it in the root theme or an exact enrolled base first."
        );
        assert_eq!(
            resolve_adaptation_local_tokens("t", 0, None, Some(&root)),
            Ok(None)
        );
        let only_unset: TokenInputs = [("--brand", TokenInput::Unset)].into();
        assert_eq!(
            resolve_adaptation_local_tokens("t", 0, Some(&only_unset), Some(&root)),
            Ok(None)
        );
    }
}
