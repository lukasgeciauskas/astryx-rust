// Writes spec/code/languages.json and spec/code/syntax-themes.json: CodeBlock's grammars
// (the built-in languages and the importable language modules, each rule's regex source
// and flags) and the syntax theme presets. `cargo xtask codegen` turns them into Rust.
//
//   node tools/export/export-code.mjs [path/to/Astryx]   (also run by export-all.mjs)

import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {astryxRoot, importTs} from './lib/astryx.mjs';
import {stableStringify} from './lib/values.mjs';

const TOKENIZER = 'packages/core/src/CodeBlock/tokenizer.ts';

/** One ASCII sample with a little of every language's syntax. */
const SAMPLE = [
  '// line comment',
  '# hash comment',
  '/* block',
  '   comment */',
  '<!DOCTYPE html>',
  '<div class="box" id=\'main\'>Hello &amp; welcome</div>',
  '@decorator',
  'const total = price * 1.5e3 + 0x1F; // trailing',
  'let s = `tmpl ${x}`;',
  'def greet(name: str) -> None:',
  '    """doc string"""',
  '    return f"hi {name}"',
  'fn main() -> Result<(), Box<dyn Error>> { let r = r#"raw"#; Ok(()) }',
  'SELECT id, name FROM users WHERE age >= 21 AND active = true;',
  'key: value',
  '- item: [1, 2, 3]',
  '#include <stdio.h>',
  'int main(void) { printf("%d\\n", 42); return 0; }',
  'echo "$HOME" && ls -la | grep foo',
  '$variable = new Thing(); $obj->method();',
  '.selector:hover { color: #fff; margin: 0 auto !important; }',
  '## Heading',
  '```js',
  'query Q($id: ID!) { node(id: $id) { name } }',
  'FROM node:20 AS build',
  'local t = { a = 1 } -- lua comment',
  'Get-ChildItem -Path . | Where-Object { $_.Length -gt 0 }',
  'struct Point { 1: i32 x }',
  '[section]',
  'name = "toml"',
  'puts "ruby" if x.nil?',
  '@interface Foo : NSObject',
  'public static void main(String[] args) { System.out.println("java"); }',
].join('\n');

/** Exposes the tokenizer's internal built-in grammar builder at import time. */
const exposeBuiltins = {
  name: 'astryx-export-expose-grammars',
  transform(code, id) {
    if (id.replace(/\\/g, '/').endsWith('CodeBlock/tokenizer.ts')) {
      return code + '\nexport {buildLanguagePatterns as __astryxBuildLanguagePatterns};\n';
    }
    return null;
  },
};

function serializeGrammar(grammar) {
  return {
    defaultType: grammar.defaultType,
    patterns: grammar.patterns.map(p => ({
      type: p.type,
      source: p.regex.source,
      flags: p.regex.flags.replace(/[gy]/g, ''),
    })),
  };
}

export async function exportCode(root, specDir, warnings) {
  const outDir = path.join(specDir, 'code');
  fs.mkdirSync(outDir, {recursive: true});

  // Built-in languages: every identifier the builder's switch names, grouped by grammar.
  const tokenizerFile = path.join(root, TOKENIZER);
  const tokenizer = await importTs(root, tokenizerFile, [exposeBuiltins]);
  const build = tokenizer.__astryxBuildLanguagePatterns;
  if (typeof build !== 'function') throw new Error('could not expose buildLanguagePatterns');
  const source = fs.readFileSync(tokenizerFile, 'utf8');
  const body = source.slice(source.indexOf('function buildLanguagePatterns'));
  const identifiers = [...body.matchAll(/case '([^']+)':/g)].map(m => m[1]);
  const groups = new Map();
  for (const id of identifiers) {
    const grammar = build(id);
    if (!grammar) {
      warnings.push(`code: built-in ${id} has no grammar`);
      continue;
    }
    const serialized = serializeGrammar(grammar);
    const key = JSON.stringify(serialized);
    if (!groups.has(key)) groups.set(key, {identifiers: [], ...serialized});
    groups.get(key).identifiers.push(id);
  }
  const builtins = [...groups.values()];

  // Language modules: each exported language value under CodeBlock/languages.
  const grammarModule = await importTs(root, path.join(root, 'packages/core/src/CodeBlock/grammar.ts'));
  const langDir = path.join(root, 'packages/core/src/CodeBlock/languages');
  const modules = [];
  for (const file of fs.readdirSync(langDir).filter(f => f.endsWith('.ts') && !/test|spec/.test(f)).sort()) {
    const module = await importTs(root, path.join(langDir, file));
    for (const [exportName, value] of Object.entries(module)) {
      if (!grammarModule.isCodeLanguage(value)) continue;
      const painter = grammarModule.codeLanguagePainter(value);
      if (painter?.kind !== 'grammar') {
        warnings.push(`code: ${file} ${exportName} is not a grammar`);
        continue;
      }
      modules.push({
        module: file.replace(/\.ts$/, ''),
        export: exportName,
        name: value.name,
        identifiers: [...value.identifiers],
        fileExtensions: [...value.fileExtensions],
        fileNames: [...value.fileNames],
        ...serializeGrammar(painter.grammar),
      });
    }
  }
  fs.writeFileSync(path.join(outDir, 'languages.json'), stableStringify({builtins, modules}));

  // Fixtures: upstream's tokenizer over one ASCII sample in every language, so the port's
  // tokens can be checked against it rule for rule (offsets agree on ASCII).
  const results = {};
  for (const group of builtins) results[group.identifiers[0]] = tokenizer.tokenize(SAMPLE, group.identifiers[0]);
  for (const m of modules) {
    const module = await importTs(root, path.join(langDir, `${m.module}.ts`));
    results[`module:${m.name}`] = tokenizer.tokenize(SAMPLE, module[m.export]);
  }
  fs.writeFileSync(path.join(outDir, 'fixtures.json'), stableStringify({sample: SAMPLE, results}));
  console.log(`wrote ${builtins.length} built-in grammars (${identifiers.length} identifiers) and ${modules.length} language modules`);

  // Syntax theme presets, in upstream order.
  const presets = await importTs(root, path.join(root, 'packages/core/src/theme/syntax/presets.ts'));
  const themes = presets.allSyntaxPresets.map(theme => ({
    name: theme.name,
    export: Object.entries(presets).find(([, v]) => v === theme)?.[0],
    tokens: theme.__inputTokens,
  }));
  const dark = presets.darkSyntaxPresets.map(t => t.name);
  const light = presets.lightSyntaxPresets.map(t => t.name);
  fs.writeFileSync(path.join(outDir, 'syntax-themes.json'), stableStringify({themes, dark, light}));
  console.log(`wrote ${themes.length} syntax theme presets`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
  const root = astryxRoot(process.argv[2] ?? path.join(repoRoot, '..', 'Astryx'));
  const warnings = [];
  await exportCode(root, path.join(repoRoot, 'spec'), warnings);
  for (const w of warnings) console.warn(`warning: ${w}`);
}
