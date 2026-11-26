//! Grammars and language values, upstream `CodeBlock/grammar.ts`.

use std::borrow::Cow;
use std::sync::Arc;

/// One grammar: rules tried in order at each position; text that matches paints as its
/// type, and the default type (the element's own text color) is left out of the output.
#[derive(Debug)]
pub struct GrammarDef {
    pub default_type: &'static str,
    /// Each rule's type and its regex (the `fancy-regex` dialect).
    pub patterns: &'static [(&'static str, &'static str)],
}

/// A built-in grammar and the identifiers that select it.
#[derive(Debug)]
pub struct BuiltinGrammar {
    pub identifiers: &'static [&'static str],
    pub grammar: GrammarDef,
}

/// An importable language module, upstream `CodeBlock/languages/<name>`.
#[derive(Debug)]
pub struct ModuleLanguage {
    pub name: &'static str,
    pub identifiers: &'static [&'static str],
    pub file_extensions: &'static [&'static str],
    pub file_names: &'static [&'static str],
    pub grammar: GrammarDef,
}

/// A flat token from a custom tokenize function: byte offsets in the whole source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatToken {
    pub ty: Cow<'static, str>,
    pub start: usize,
    pub end: usize,
}

/// A custom language's tokenize function: `(code, language name)`.
pub type TokenizeFn = Arc<dyn Fn(&str, &str) -> Vec<FlatToken> + Send + Sync>;

#[derive(Clone)]
pub(crate) enum Painter {
    Grammar(&'static GrammarDef),
    Tokenize(TokenizeFn),
}

/// Upstream `CodeLanguage`: a language module's value, or a custom language from
/// [`create_code_language`].
#[derive(Clone)]
pub struct CodeLanguage {
    pub name: Cow<'static, str>,
    pub identifiers: Vec<Cow<'static, str>>,
    pub file_extensions: Vec<Cow<'static, str>>,
    pub file_names: Vec<Cow<'static, str>>,
    pub(crate) painter: Painter,
}

impl std::fmt::Debug for CodeLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeLanguage")
            .field("name", &self.name)
            .field("identifiers", &self.identifiers)
            .finish()
    }
}

impl PartialEq for CodeLanguage {
    /// Upstream `sameCodeLanguage`: the same module, or custom languages with the same
    /// tokenize function, name and identifiers.
    fn eq(&self, other: &Self) -> bool {
        let same_painter = match (&self.painter, &other.painter) {
            (Painter::Grammar(a), Painter::Grammar(b)) => std::ptr::eq(*a, *b),
            (Painter::Tokenize(a), Painter::Tokenize(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        same_painter && self.name == other.name && self.identifiers == other.identifiers
    }
}

fn owned(list: &'static [&'static str]) -> Vec<Cow<'static, str>> {
    list.iter().map(|s| Cow::Borrowed(*s)).collect()
}

impl CodeLanguage {
    /// A language module's value.
    pub fn module(module: &'static ModuleLanguage) -> Self {
        CodeLanguage {
            name: Cow::Borrowed(module.name),
            identifiers: owned(module.identifiers),
            file_extensions: owned(module.file_extensions),
            file_names: owned(module.file_names),
            painter: Painter::Grammar(&module.grammar),
        }
    }

    /// Whether it is a custom language (its own tokenize function).
    pub fn is_custom(&self) -> bool {
        matches!(self.painter, Painter::Tokenize(_))
    }
}

/// Upstream `createCodeLanguage`: a language from your own tokenize function, usable
/// everywhere a language module is.
pub fn create_code_language(
    name: impl Into<Cow<'static, str>>,
    identifiers: impl IntoIterator<Item = impl Into<Cow<'static, str>>>,
    file_extensions: impl IntoIterator<Item = impl Into<Cow<'static, str>>>,
    file_names: impl IntoIterator<Item = impl Into<Cow<'static, str>>>,
    tokenize: impl Fn(&str, &str) -> Vec<FlatToken> + Send + Sync + 'static,
) -> CodeLanguage {
    CodeLanguage {
        name: name.into(),
        identifiers: identifiers.into_iter().map(Into::into).collect(),
        file_extensions: file_extensions.into_iter().map(Into::into).collect(),
        file_names: file_names.into_iter().map(Into::into).collect(),
        painter: Painter::Tokenize(Arc::new(tokenize)),
    }
}

/// Upstream `CodeLanguageInput`: a built-in identifier or a language value.
#[derive(Clone, Debug, PartialEq)]
pub enum CodeLanguageInput {
    Name(Cow<'static, str>),
    Language(CodeLanguage),
}

impl Default for CodeLanguageInput {
    fn default() -> Self {
        CodeLanguageInput::Name(Cow::Borrowed("plaintext"))
    }
}

impl From<&'static str> for CodeLanguageInput {
    fn from(name: &'static str) -> Self {
        CodeLanguageInput::Name(Cow::Borrowed(name))
    }
}

impl From<String> for CodeLanguageInput {
    fn from(name: String) -> Self {
        CodeLanguageInput::Name(Cow::Owned(name))
    }
}

impl From<CodeLanguage> for CodeLanguageInput {
    fn from(language: CodeLanguage) -> Self {
        CodeLanguageInput::Language(language)
    }
}

/// Upstream `languageLabel`: the word a language labels, a string itself or a value's
/// name.
pub fn language_label(language: &CodeLanguageInput) -> &str {
    match language {
        CodeLanguageInput::Name(name) => name,
        CodeLanguageInput::Language(l) => &l.name,
    }
}

/// Upstream `languageKey`: the word a language reflects, its first identifier.
pub fn language_key(language: &CodeLanguageInput) -> &str {
    match language {
        CodeLanguageInput::Name(name) => name,
        CodeLanguageInput::Language(l) => l.identifiers.first().map_or(&l.name, |i| i),
    }
}

/// Upstream `listedLanguage`: the listed value that claims `word`; a later entry wins.
pub fn listed_language<'a>(languages: &'a [CodeLanguage], word: &str) -> Option<&'a CodeLanguage> {
    languages
        .iter()
        .rev()
        .find(|l| l.identifiers.iter().any(|i| i == word))
}

/// Upstream `listedLanguageForFile`: the listed value that claims a file by name or
/// extension; a later entry wins.
pub fn listed_language_for_file<'a>(
    languages: &'a [CodeLanguage],
    file_name: &str,
    extension: Option<&str>,
) -> Option<&'a CodeLanguage> {
    languages.iter().rev().find(|l| {
        l.file_names.iter().any(|n| n == file_name)
            || extension.is_some_and(|e| l.file_extensions.iter().any(|x| x == e))
    })
}

/// Every identifier a built-in grammar answers to.
pub fn builtin_identifiers() -> impl Iterator<Item = &'static str> {
    crate::generated::languages::BUILTINS
        .iter()
        .flat_map(|b| b.identifiers.iter().copied())
}

/// The built-in grammar an identifier selects.
pub(crate) fn builtin(identifier: &str) -> Option<&'static GrammarDef> {
    crate::generated::languages::BUILTINS
        .iter()
        .find(|b| b.identifiers.contains(&identifier))
        .map(|b| &b.grammar)
}
