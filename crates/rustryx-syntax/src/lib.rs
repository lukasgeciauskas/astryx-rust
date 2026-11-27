//! CodeBlock's syntax engine, upstream `CodeBlock/grammar.ts`, `CodeBlock/tokenizer.ts`,
//! `CodeBlock/languages/*` and `theme/syntax/`: grammars of regex rules tried in order at
//! each position, the built-in languages and the importable modules, custom languages,
//! the fence and diff helpers, and the syntax theme definitions with their presets.
//!
//! Offsets are byte offsets into the UTF-8 source (upstream's are UTF-16 units).

mod grammar;
mod theme;
mod tokenizer;

#[rustfmt::skip]
mod generated {
    pub mod languages;
    pub mod presets;
}

pub use generated::languages::languages;
pub use generated::presets::*;
pub use grammar::{
    BuiltinGrammar, CodeLanguage, CodeLanguageInput, FlatToken, GrammarDef, ModuleLanguage,
    TokenizeFn, builtin_identifiers, create_code_language, language_key, language_label,
    listed_language, listed_language_for_file,
};
pub use theme::{SyntaxColor, SyntaxThemeDefinition, SyntaxThemeTokens, define_syntax_theme};
pub use tokenizer::{
    DiffLineKind, SYNC_TOKENIZE_THRESHOLD, SyntaxToken, TokenLine, diff_line_kind,
    file_header_lines, flat_tokens_to_lines, parse_fence_language, split_diff_lines, tokenize,
};
