// Writes spec/markdown/: the HTML named character references Markdown decodes
// (character-references.json), and fixtures of upstream's own parser
// (parser-fixtures.json): its canonical AST for every input in a corpus (the string
// arguments of its parser tests, and the samples below), under the default options,
// math, GFM autolinks and citation sources. Positions are converted to UTF-8 byte
// offsets and columns, the units the port works in.
//
//   node tools/export/export-markdown.mjs [path/to/Astryx]   (also run by export-all.mjs)

import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {astryxRoot, importTs} from './lib/astryx.mjs';
import {stableStringify} from './lib/values.mjs';

const exposeNamedReferences = {
  name: 'astryx-export-expose-references',
  transform(code, id) {
    if (id.replace(/\\/g, '/').endsWith('Markdown/characterReferences.ts')) {
      return code + '\nexport {NAMED_REFERENCES as __astryxNamedReferences};\n';
    }
    return null;
  },
};

/** Samples beyond the parser tests' inputs: CommonMark, GFM and Astryx's own syntax. */
const SAMPLES = [
  '# Heading 1\n\n## Heading 2\n\n###### Heading 6\n\n####### not a heading',
  'Setext 1\n========\n\nSetext 2\n--------',
  'Plain paragraph\nwith a soft break.\n\nSecond paragraph.',
  'Hard break with spaces  \nnext line\\\nafter backslash',
  '*em* _em_ **strong** __strong__ ***both*** ~~strike~~ ~one~',
  'intra_word_underscores and intra*word*stars',
  '**unclosed bold and *mixed _nesting_ here***',
  '`code` and ``double ` tick`` and ` padded `',
  '[link](https://example.com "title") and [rel](/path) and <https://auto.link>',
  '[ref link][Ref] and [collapsed][] and [shortcut]\n\n[ref]: https://example.com/ref\n[collapsed]: /c\n[shortcut]: <https://s.example>',
  '![alt text](https://img.example/a.png) and ![ref img][pic]\n\n[pic]: /p.png',
  '[unsafe](javascript:alert(1)) and [data](data:text/html,x) and [mail](mailto:a@b.c)',
  '> quote\n> continues\n>\n> > nested quote',
  '> lazy\ncontinuation line',
  '- one\n- two\n  - nested\n  - nested 2\n- three',
  '1. first\n2. second\n10. tenth',
  '3. starts at three\n4. four',
  '- [ ] todo\n- [x] done\n- plain',
  '* star\n+ plus\n- dash',
  '- loose\n\n- list\n\n  with paragraph',
  '1) paren\n2) list',
  '```ts\nconst a = 1;\n```\n\n~~~\ntilde fence\n~~~',
  '```\nunclosed fence\nstill code',
  '    indented code\n    block\n\nafter',
  '---\n***\n___\n- - -',
  '| a | b |\n|---|:-:|\n| 1 | 2 |\n| 3 | 4 |',
  '| left | center | right |\n|:--|:--:|--:|\n| x | y | z |',
  'Table without leading pipes\na | b\n--|--\n1 | 2',
  'Escapes: \\* \\_ \\# \\[ \\] \\\\ \\`',
  'Entities: &amp; &lt; &gt; &quot; &copy; &#65; &#x42; &nbsp;x &bogus;',
  'Citation [1] and [src] and 【2】 and [unknown]',
  'Math $x^2$ inline and $$\nE = mc^2\n$$ display',
  'Price $5 and $10 is not math',
  '$$\na + b\n$$',
  'Bare https://example.com/path?q=1. and www.example.org, and mail@example.com!',
  '<mail@example.com> and <ftp://not.auto>',
  'Line one\n\n\n\nLine after blank lines',
  '<div>html block</div>\n\n<span>inline</span> text',
  'Trailing spaces at end   ',
  '#NoSpace heading and # Space heading',
  '  ## indented heading',
  '- item with `code` and **bold**\n- item with [link](/x)',
  '> - list in quote\n> - second',
  '1. ordered\n   - nested unordered\n     1. deep ordered',
  'Text with emoji 😀 and accents café — dash',
  '# Héading with ünïcode\n\nParagraph ✓',
  'A *very* long paragraph that goes on and on with **emphasis** and _more_ text and a [link](https://example.com) at the end.',
  '| a |\n|---|\n| `code \\| pipe` |',
  '[link with **bold** inside](/x) and [![img](/i.png)](/wrap)',
  'auto <https://a.b/c?d=e&f=g> link',
  '***\nthematic then text',
  '- a\n\n\n- b',
  '> # heading in quote\n> text',
  'Footnote-like [^1] text\n\n[^1]: not supported',
  '\\$escaped dollar\\$ and \\[1\\]',
  '**bold**text and text**bold**',
  'snake_case_word and __init__',
  '1. one\n\n   para in item\n\n2. two',
  '- ```\n  code in list\n  ```',
  '|no table',
  'Heading\n=\n\ntext',
  '[a]\n\n[a]: /first\n[a]: /second',
  '[link](</with spaces> "t")',
  '[link](url(with)parens)',
  'image ![](/empty-alt.png)',
];

/** Walks a node tree, converting UTF-16 positions in `source` to UTF-8 bytes. */
function toUtf8Positions(node, source) {
  const bytes = offset => Buffer.byteLength(source.slice(0, offset), 'utf8');
  const point = p => {
    if (!p || typeof p !== 'object') return p;
    const out = {...p};
    if (typeof p.offset === 'number') {
      out.offset = bytes(p.offset);
      if (typeof p.column === 'number') {
        const lineStart = source.lastIndexOf('\n', p.offset - 1) + 1;
        out.column = Buffer.byteLength(source.slice(lineStart, p.offset), 'utf8') + 1;
      }
    }
    return out;
  };
  const walk = value => {
    if (Array.isArray(value)) return value.map(walk);
    if (!value || typeof value !== 'object') return value;
    const out = {};
    for (const [k, v] of Object.entries(value)) {
      if (k === 'position' && v && typeof v === 'object') {
        out[k] = {start: point(v.start), end: point(v.end)};
      } else {
        out[k] = walk(v);
      }
    }
    return out;
  };
  return walk(node);
}

/** The string-literal first arguments of `fn(` calls in a test file (no interpolation). */
function literalArguments(source, fns) {
  const BACKTICK = String.fromCharCode(96);
  const quotes = ['"', "'", BACKTICK];
  const out = [];
  for (const fn of fns) {
    const needle = fn + '(';
    let i = 0;
    while ((i = source.indexOf(needle, i)) !== -1) {
      const before = source[i - 1];
      i += needle.length;
      if (before && /[A-Za-z0-9_$]/.test(before)) continue;
      let j = i;
      while (/\s/.test(source[j] ?? '')) j++;
      const quote = source[j];
      if (!quotes.includes(quote)) continue;
      let k = j + 1;
      let ok = true;
      while (k < source.length && source[k] !== quote) {
        if (source[k] === '\\') k++;
        else if (quote === BACKTICK && source[k] === '$' && source[k + 1] === '{') { ok = false; break; }
        else if (quote !== BACKTICK && source[k] === '\n') { ok = false; break; }
        k++;
      }
      if (!ok || k >= source.length) continue;
      try {
        const value = new Function('return ' + source.slice(j, k + 1) + ';')();
        if (typeof value === 'string') out.push(value);
      } catch {
        // A literal this reader cannot evaluate is skipped.
      }
    }
  }
  return out;
}

export async function exportMarkdown(root, specDir, warnings) {
  const outDir = path.join(specDir, 'markdown');
  fs.mkdirSync(outDir, {recursive: true});
  const core = path.join(root, 'packages/core/src/Markdown');

  const refs = await importTs(root, path.join(core, 'characterReferences.ts'), [exposeNamedReferences]);
  const table = refs.__astryxNamedReferences;
  if (typeof table !== 'string') throw new Error('could not expose NAMED_REFERENCES');
  const entries = table.trim().split(/\s+/).map(entry => {
    const [name, hex] = entry.split('=');
    return [name, hex.split(',').map(h => parseInt(h, 16))];
  });
  fs.writeFileSync(path.join(outDir, 'character-references.json'), stableStringify({references: entries}));

  const parser = await importTs(root, path.join(core, 'parser.ts'));
  const tests = fs.readFileSync(path.join(core, 'parser.test.ts'), 'utf8');
  const blockInputs = [...new Set([...literalArguments(tests, ['parseMarkdown', 'parseMarkdownAst']), ...SAMPLES])];
  const inlineInputs = [...new Set(literalArguments(tests, ['parseInline', 'parseInlineAst']))];
  const variants = {
    default: {},
    math: {math: true},
    autolink: {autolink: 'gfm'},
    citations: {sourceIds: new Set(['1', '2', 'src', 'a'])},
  };
  const block = [];
  for (const input of blockInputs) {
    const results = {};
    for (const [name, options] of Object.entries(variants)) {
      try {
        results[name] = toUtf8Positions(parser.parseMarkdownAst(input, options), input);
      } catch (error) {
        warnings.push(`markdown: parseMarkdownAst failed on ${JSON.stringify(input).slice(0, 60)}: ${error.message}`);
      }
    }
    block.push({input, results});
  }
  const inline = [];
  for (const input of inlineInputs) {
    const results = {};
    for (const [name, options] of Object.entries(variants)) {
      try {
        results[name] = toUtf8Positions(parser.parseInlineAst(input, options), input);
      } catch (error) {
        warnings.push(`markdown: parseInlineAst failed on ${JSON.stringify(input).slice(0, 60)}: ${error.message}`);
      }
    }
    inline.push({input, results});
  }
  const slugs = [...new Set(blockInputs.flatMap(i => [...i.matchAll(/^#{1,6}\s+(.*)$/gm)].map(m => m[1])))]
    .map(text => [text, parser.slugify(text)]);
  fs.writeFileSync(path.join(outDir, 'parser-fixtures.json'), stableStringify({block, inline, slugs}));
  console.log(`wrote ${entries.length} character references; ${block.length} block and ${inline.length} inline parser fixtures`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
  const root = astryxRoot(process.argv[2] ?? path.join(repoRoot, '..', 'Astryx'));
  const warnings = [];
  await exportMarkdown(root, path.join(repoRoot, 'spec'), warnings);
  for (const w of warnings) console.warn(`warning: ${w}`);
}
