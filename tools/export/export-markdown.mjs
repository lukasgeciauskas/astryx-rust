// Writes spec/markdown/: the HTML named character references Markdown decodes
// (character-references.json), fixtures of upstream's own parser
// (parser-fixtures.json): its canonical AST for every input in a corpus (the string
// arguments of its parser tests, and the samples below), under the default options,
// math, GFM autolinks and citation sources; and fixtures of its plugin system
// (plugin-fixtures.json): parseMarkdownAst, parseInlineAst, parseOutlineFromMarkdown and
// the heading projection under each first-party plugin and the test plugins below, with
// the plugin failures upstream reported (plugin and phase) and semantic-fence proposals.
// Positions are converted to UTF-8 byte offsets and columns, the units the port works in.
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

/**
 * Walks a node tree, converting UTF-16 positions in `source` to UTF-8 bytes: canonical
 * `position` points and the released projection's `range`.
 */
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
    // fromEntries, not assignment: a `__proto__` key (frontmatter can author one) stays a key.
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) =>
        k === 'position' && v && typeof v === 'object'
          ? [k, {start: point(v.start), end: point(v.end)}]
          : k === 'range' && v && typeof v.start === 'number' && typeof v.end === 'number'
            ? [k, {start: bytes(v.start), end: bytes(v.end)}]
            : [k, walk(v)],
      ),
    );
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

/**
 * Exposes, from one module graph, what the plugin fixtures need: the plugin factories,
 * the fence proposal reader, the heading projection, Outline's parser and the Remark
 * adapter (appended to parser.ts as exports), and a record of every plugin failure
 * upstream reports (a hook at the top of reportMarkdownPluginFailure).
 */
const exposePluginModules = {
  name: 'astryx-export-expose-plugins',
  // On the TypeScript source, before it is compiled.
  enforce: 'pre',
  transform(code, id) {
    const file = id.replace(/\\/g, '/');
    if (file.endsWith('Markdown/parser.ts')) {
      return (
        code +
        [
          "export * as __astryxPlugins from './plugins/index';",
          "export {getMarkdownFenceProposal as __astryxFenceProposal} from './plugins/semanticFence';",
          "export {prepareMarkdownPlugins as __astryxPrepare} from './plugins/protocol';",
          "export {projectMarkdownHeadings as __astryxProjectHeadings} from './headingProjection';",
          "export {parseOutlineFromMarkdown as __astryxOutline} from '../Outline/parseOutlineFromMarkdown';",
          "export {createMarkdownRemarkTransform as __astryxRemark} from './remark';",
        ].join('\n') +
        '\n'
      );
    }
    if (file.endsWith('Markdown/plugins/protocol.ts')) {
      const hooked = code.replace(
        /(export function reportMarkdownPluginFailure\([\s\S]*?\): void \{)/,
        '$1\n  (globalThis as any).__astryxMarkdownFailures?.push([pluginName, phase]);',
      );
      if (hooked === code) throw new Error('could not hook reportMarkdownPluginFailure');
      return hooked;
    }
    return null;
  },
};

/** The string literals a test file assigns to source-like variables. */
function literalAssignments(source) {
  const BACKTICK = String.fromCharCode(96);
  const out = [];
  const re = /(?:const|let)\s+(?:source|markdown|input|text|document|body|doc)\w*\s*=\s*(['"`])/g;
  let match;
  while ((match = re.exec(source)) !== null) {
    const quote = match[1];
    const j = match.index + match[0].length - 1;
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
  return out;
}

/**
 * The test plugins: each named set is an ordered plugin list. The Rust test
 * (crates/astryx-markdown/tests/plugin_fixtures.rs) defines the same plugins.
 */
function pluginSets(api) {
  const {createMarkdownPlugin} = api.plugins;
  const mentions = createMarkdownPlugin({
    name: 'mentions',
    apiVersion: 1,
    parseKey: 'v1',
    syntax: {
      inline: [{
        startsWith: ['@{'],
        maxSpan: 80,
        tokenize({source, offset, end, isFinal}) {
          const close = source.indexOf('}', offset + 2);
          if (close < 0 || close >= end) return isFinal ? {status: 'no-match'} : {status: 'defer'};
          return {
            status: 'match',
            end: close + 1,
            node: {type: 'extension', plugin: 'mentions', name: 'mention', display: 'inline', data: {label: source.slice(offset + 2, close)}},
          };
        },
      }],
    },
    renderers: {mention: {render: () => null, toText: node => '@' + node.data.label}},
  });
  const handles = createMarkdownPlugin({
    name: 'handles',
    apiVersion: 1,
    parseKey: 'v1',
    syntax: {
      inline: [{
        startsWith: ['@'],
        maxSpan: 32,
        tokenize({source, offset, end}) {
          const match = /^@([A-Za-z0-9_]+)/.exec(source.slice(offset, end));
          if (match == null) return {status: 'no-match'};
          return {
            status: 'match',
            end: offset + match[0].length,
            node: {type: 'extension', plugin: 'handles', name: 'handle', display: 'inline', data: {user: match[1]}},
          };
        },
      }],
    },
    renderers: {handle: {render: () => null, toText: node => '@' + node.data.user}},
  });
  const callouts = createMarkdownPlugin({
    name: 'callouts',
    apiVersion: 1,
    parseKey: 'v1',
    syntax: {
      block: [{
        startsWith: [':::note'],
        maxSpan: 500,
        tokenize({source, offset, end, isFinal}) {
          const close = source.indexOf('\n:::', offset + 7);
          if (close < 0 || close + 4 > end) return isFinal ? {status: 'no-match'} : {status: 'defer'};
          return {
            status: 'match',
            end: close + 4,
            node: {type: 'extension', plugin: 'callouts', name: 'callout', display: 'block', data: {body: source.slice(offset + 7, close).trim()}},
          };
        },
      }],
    },
    renderers: {callout: {render: () => null, toText: node => node.data.body}},
  });
  const brokenSyntax = createMarkdownPlugin({
    name: 'broken-syntax',
    apiVersion: 1,
    parseKey: 'v1',
    syntax: {
      inline: [{startsWith: ['%%'], maxSpan: 16, tokenize() { throw new Error('broken tokenizer'); }}],
      block: [{
        startsWith: ['%%%'],
        maxSpan: 16,
        tokenize({offset}) {
          return {status: 'match', end: offset + 2, node: {type: 'extension', plugin: 'broken-syntax', name: 'bad', display: 'block', data: null}};
        },
      }],
    },
    renderers: {bad: {render: () => null, toText: () => ''}},
  });
  const badges = createMarkdownPlugin({
    name: 'badges',
    apiVersion: 1,
    transform(root) {
      return {
        ...root,
        children: root.children.map(block =>
          block.type === 'paragraph'
            ? {...block, children: [...block.children, {type: 'extension', plugin: 'badges', name: 'badge', display: 'inline', data: {label: 'New'}}]}
            : block,
        ),
      };
    },
    renderers: {badge: {render: () => null, toText: node => node.data.label}},
  });
  const replaceDraft = createMarkdownPlugin({
    name: 'replace-draft',
    apiVersion: 1,
    transform(root) {
      return {
        ...root,
        children: root.children.map(block =>
          block.type === 'paragraph' || block.type === 'heading'
            ? {...block, children: block.children.map(node => node.type === 'text' ? {...node, value: node.value.replaceAll('Draft', 'Final')} : node)}
            : block,
        ),
      };
    },
  });
  const finality = createMarkdownPlugin({
    name: 'finality',
    apiVersion: 1,
    transform(root, context) {
      return {
        ...root,
        children: root.children.map(block =>
          block.type === 'heading' ? {...block, children: [{type: 'text', value: context.isFinal ? 'Final' : 'Draft'}]} : block,
        ),
      };
    },
  });
  const releveled = createMarkdownPlugin({
    name: 'relevel',
    apiVersion: 1,
    transform(root) {
      return {...root, children: root.children.map(block => block.type === 'heading' ? {...block, depth: 6} : block)};
    },
  });
  const addHeading = createMarkdownPlugin({
    name: 'add-heading',
    apiVersion: 1,
    transform(root) {
      return {...root, children: [...root.children, {type: 'heading', depth: 2, children: [{type: 'text', value: 'Generated'}]}]};
    },
  });
  const removeHeadings = createMarkdownPlugin({
    name: 'remove-headings',
    apiVersion: 1,
    transform(root) {
      return {...root, children: root.children.filter(block => block.type !== 'heading')};
    },
  });
  const deleteForeign = createMarkdownPlugin({
    name: 'delete-foreign',
    apiVersion: 1,
    transform(root) {
      return {
        ...root,
        children: root.children.map(block =>
          block.type === 'paragraph' ? {...block, children: block.children.filter(node => node.type !== 'extension')} : block,
        ),
      };
    },
  });
  const links = url => createMarkdownPlugin({
    name: 'set-link',
    apiVersion: 1,
    transform(root) {
      return {
        ...root,
        children: root.children.map(block =>
          block.type === 'paragraph' ? {...block, children: [...block.children, {type: 'link', url, children: [{type: 'text', value: 'go'}]}]} : block,
        ),
      };
    },
  });
  const failing = createMarkdownPlugin({
    name: 'failing',
    apiVersion: 1,
    transform() { throw new Error('private failure text'); },
  });
  const textTransform = createMarkdownPlugin({
    name: 'shortcodes',
    apiVersion: 1,
    transform: api.plugins.createMarkdownTextTransform({
      pattern: /:([a-z]+):/g,
      requiredSubstrings: [':'],
      replace: match => ({type: 'strong', children: [{type: 'text', value: match[1].toUpperCase()}]}),
    }),
  });
  const todoLinks = createMarkdownPlugin({
    name: 'todo-links',
    apiVersion: 1,
    transform: api.plugins.createMarkdownTextTransform({
      pattern: /\bTODO\b/g,
      replace: () => [{type: 'link', url: '  /todo  ', children: [{type: 'text', value: 'todo'}]}],
    }),
  });
  const drafts = createMarkdownPlugin({
    name: 'drafts',
    apiVersion: 1,
    transform: api.plugins.createMarkdownTextTransform({
      pattern: /draft/gi,
      requiredSubstrings: ['raft'],
      replace: match => ({type: 'emphasis', children: [{type: 'text', value: match[0]}]}),
    }),
  });
  const fence = createMarkdownPlugin({
    name: 'charts',
    apiVersion: 1,
    transform: api.plugins.createMarkdownFenceTransform({
      languages: ['chart'],
      createNode: ({code, meta}) =>
        code.trim() === '' ? null : {type: 'extension', plugin: 'charts', name: 'chart', display: 'block', data: {code, meta: meta ?? null}},
    }),
    renderers: {chart: {render: () => null, toText: node => node.data.code}},
  });
  const decoration = createMarkdownPlugin({
    name: 'review',
    apiVersion: 1,
    transform: api.plugins.createMarkdownSourceDecoration({
      name: 'review-hit',
      ranges: [{start: 12, end: 30}, {start: 0, end: 3, data: {reviewer: 'ada'}}, {start: 0, end: 3, data: {reviewer: 'ada'}}],
    }),
  });
  const frontmatter = api.plugins.createMarkdownFrontmatter({name: 'meta', parse: fields => fields});
  const headingLinks = api.plugins.createMarkdownHeadingLinks({headingIdPrefix: 'doc', permalinkBaseUrl: 'https://example.com/page#old'});
  const plainHeadingLinks = api.plugins.createMarkdownHeadingLinks();
  const shout = createMarkdownPlugin({
    name: 'shout',
    apiVersion: 1,
    transform: api.remark(() => tree => {
      const visit = node => {
        if (node.type === 'heading') {
          for (const child of node.children) if (child.type === 'text') child.value = child.value.toUpperCase();
        }
        for (const child of node.children ?? []) visit(child);
      };
      visit(tree);
    }),
  });
  const remarkHtml = createMarkdownPlugin({
    name: 'remark-html',
    apiVersion: 1,
    transform: api.remark(() => tree => {
      const paragraph = tree.children.find(node => node.type === 'paragraph');
      if (paragraph) paragraph.children.push({type: 'html', value: '<b>x</b>'});
    }),
  });
  const remarkData = createMarkdownPlugin({
    name: 'remark-data',
    apiVersion: 1,
    transform: api.remark(() => tree => {
      for (const node of tree.children) node.data = {seen: true};
      tree.children.push({type: 'paragraph', children: [{type: 'link', url: ' https://example.com/x ', title: null, children: [{type: 'text', value: 'added'}]}]});
    }),
  });
  return {
    mentions: [mentions],
    handles: [handles],
    callouts: [callouts],
    'mentions+callouts': [mentions, callouts],
    'broken-syntax': [brokenSyntax],
    badges: [badges],
    'replace-draft': [replaceDraft],
    finality: [finality],
    relevel: [releveled],
    'add-heading': [addHeading],
    'remove-headings': [removeHeadings],
    'mentions+delete-foreign': [mentions, deleteForeign],
    'unsafe-link': [links('javascript:alert(1)')],
    'padded-link': [links('  /docs  ')],
    failing: [failing],
    'soft-breaks': [api.plugins.markdownSoftBreaksPlugin],
    'source-lines': [api.plugins.markdownSourceLinesPlugin],
    'source-lines+callouts': [api.plugins.markdownSourceLinesPlugin, callouts],
    shortcodes: [textTransform],
    'todo-links': [todoLinks],
    drafts: [drafts],
    charts: [fence],
    'charts+source-lines': [fence, api.plugins.markdownSourceLinesPlugin],
    review: [decoration],
    frontmatter: [frontmatter.plugin],
    'frontmatter+source-lines': [frontmatter.plugin, api.plugins.markdownSourceLinesPlugin],
    'heading-links': [headingLinks],
    'heading-links-plain': [plainHeadingLinks],
    'heading-links+mentions': [mentions, headingLinks],
    shout: [shout],
    'remark-html': [remarkHtml],
    'remark-data': [remarkData],
    'badges+replace-draft+shout': [badges, replaceDraft, shout],
  };
}

/** Plugin inputs beyond the corpus: each set's own syntax, streamed and malformed. */
const PLUGIN_SAMPLES = [
  'Hello @{Ada}.',
  '**@{Ada}** [@{Grace}](/people) `@{Linus}` ![@{Img}](/i.png)',
  'Pending @{Ad',
  'Pending @',
  '@{a} and @{b} and @{',
  '~~@{struck}~~ and _@{em}_',
  'Ping @ada_l and @grace, not a@b or @ alone.',
  '**@ada** and [@link](/x)',
  ':::note\nRead this\n:::',
  ':::note\nRead this\n:::\n\nAfter the callout.',
  'Before\n:::note\nInterrupts\n:::',
  '> :::note\n> quoted\n> :::\n\n- :::note\n  listed\n  :::',
  ':::note\nunfinished',
  ':::note',
  ':::',
  '   :::note\nindented\n:::',
  '    :::note\ntoo deep\n:::',
  '> quote\n:::note\nlazy\n:::',
  '- item\n:::note\nlazy\n:::',
  'Hello @{Ada}.\n\n:::note\nRead this\n:::',
  'a %% b',
  '%%%\nline',
  '# Draft heading\n\nA Draft paragraph.',
  '# Pending',
  '# One\n\n## Two\n\n### Three',
  '# Removed\n\nSurviving body.',
  'Soft\nbreak\r\nand\rcr',
  '# Head\nnot\n\n> quoted\n> soft\n\n| a |\n|---|\n| b |\n\n`code\nspan`',
  '**strong\nsoft** [link\ntext](/x)',
  'Use :smile: and :tada: here, not `:code:` or [:link:](/x).',
  'TODO: fix TODOs and TODO.',
  '# A draft\n\nDRAFT and Draft and drafted',
  '```chart line\nseries a\n```\n\n```chart\n\n```\n\n```charts\nnot mine\n```',
  '> ```chart\n> quoted\n> ```\n\n- ```chart\n  listed\n  ```',
  '~~~chart\ntilde\n~~~',
  '# Title\n\nFirst paragraph is long enough.\n\nSecond paragraph.\n\n- list',
  '---\ntitle: Hello\nauthor: Ada\n---\n# Body\n\nText',
  '---\r\ntitle: CRLF\r\n---\r\nBody',
  '---\ntitle: Pending',
  '---\ntitle: a\ntitle: b\n---\nDuplicate keys',
  '---\nnot a field\n---\nBody',
  'Intro\n\n---\ntitle: late\n---',
  '---\n---\nEmpty block',
  '# Setup\n\n## Setup\n\n> # Quoted\n\n- # Listed\n\n# Setup-1\n\n# 🎉\n\n# Café Ünïcode ﬁ',
  '# Hello @{Ada}',
  '- one\n- two\n\n  para\n\n> quote\n> more\n\n[ref]: /x\n\nlast',
  '1. a\n   - b\n     c\nlazy',
  '| h1 | h2 |\n|---|---|\n| 1 | 2 |',
  '$$\nmath\n$$',
];

async function exportPluginFixtures(root, core, outDir, warnings, blockInputs) {
  const parser = await importTs(root, path.join(core, 'parser.ts'), [exposePluginModules]);
  const api = {plugins: parser.__astryxPlugins, remark: parser.__astryxRemark};
  const sets = pluginSets(api);
  const testSources = fs
    .readdirSync(path.join(core, 'plugins'))
    .filter(name => /\.test\.tsx?$/.test(name))
    .map(name => path.join(core, 'plugins', name))
    .concat(['ast.test.ts', 'remark.test.tsx', 'parser.public.test.ts'].map(name => path.join(core, name)))
    .map(file => fs.readFileSync(file, 'utf8'));
  const fromTests = testSources.flatMap(source => [
    ...literalArguments(source, ['parseMarkdown', 'parseMarkdownAst', 'parseInline', 'parseInlineAst', 'parseOutlineFromMarkdown']),
    ...literalAssignments(source),
  ]);
  const common = blockInputs.filter((_, index) => index % 6 === 0);
  // Sets whose result can depend on finality (syntax that defers, transforms that read it).
  const streaming = new Set(['mentions', 'callouts', 'mentions+callouts', 'finality', 'frontmatter', 'frontmatter+source-lines', 'review', 'source-lines']);
  const inputs = [...new Set([...PLUGIN_SAMPLES, ...fromTests, ...common])];
  const ascii = input => /^[\x00-\x7f]*$/.test(input);
  const SEPARATOR = String.fromCharCode(0);

  const record = parse => {
    globalThis.__astryxMarkdownFailures = [];
    const warn = console.warn;
    const error = console.error;
    console.warn = () => {};
    console.error = () => {};
    try {
      const result = parse();
      const failures = [...new Set(globalThis.__astryxMarkdownFailures.map(pair => pair.join(SEPARATOR)))]
        .sort()
        .map(pair => pair.split(SEPARATOR));
      return {result, failures};
    } finally {
      console.warn = warn;
      console.error = error;
      globalThis.__astryxMarkdownFailures = undefined;
    }
  };
  const plain = value => JSON.parse(JSON.stringify(value));
  const proposalsOf = rootNode => {
    const out = [];
    const visit = node => {
      if (node.type === 'code') {
        const proposal = parser.__astryxFenceProposal(node);
        out.push(proposal == null ? null : plain(proposal.node));
      }
      for (const child of node.children ?? []) visit(child);
    };
    visit(rootNode);
    return out;
  };

  const cases = [];
  for (const [set, plugins] of Object.entries(sets)) {
    for (const input of inputs) {
      if (set === 'review' && !ascii(input)) continue;
      for (const isFinal of streaming.has(set) ? [true, false] : [true]) {
        const block = record(() => parser.parseMarkdownAstInternal(input, {plugins}, isFinal));
        const entry = {
          set,
          input,
          isFinal,
          block: toUtf8Positions(plain(block.result), input),
          failures: block.failures,
        };
        const proposals = proposalsOf(block.result);
        if (proposals.some(proposal => proposal != null)) entry.proposals = toUtf8Positions(proposals, input);
        if (isFinal) {
          const inline = record(() => parser.parseInlineAst(input, {plugins}));
          entry.inline = toUtf8Positions(plain(inline.result), input);
          entry.inlineFailures = inline.failures;
        }
        if (input.includes('#')) {
          const outline = record(() => parser.__astryxOutline(input, {plugins, isFinal}));
          entry.outline = outline.result;
          entry.outlineFailures = outline.failures;
        }
        if (/^heading-links/.test(set) || set === 'mentions') {
          const projection = parser.__astryxProjectHeadings(block.result, parser.__astryxPrepare(plugins));
          const headings = [];
          const visit = node => {
            if (node.type === 'heading') {
              const id = projection.ids.get(node);
              headings.push(id == null ? null : {id, label: projection.labels.get(node), permalink: projection.permalinkUrls.get(node) ?? null});
            }
            for (const child of node.children ?? []) visit(child);
          };
          visit(block.result);
          entry.headings = headings;
        }
        cases.push(entry);
      }
    }
  }
  // One case a line, keys sorted: compact, and a regeneration diffs by case.
  const sorted = value =>
    Array.isArray(value)
      ? value.map(sorted)
      : value && typeof value === 'object'
        ? Object.fromEntries(Object.keys(value).sort().map(key => [key, sorted(value[key])]))
        : value;
  const lines = cases.map(entry => JSON.stringify(sorted(entry)));
  fs.writeFileSync(path.join(outDir, 'plugin-fixtures.json'), '{"cases": [\n' + lines.join(',\n') + '\n]}\n');
  console.log(`wrote ${cases.length} plugin fixtures over ${Object.keys(sets).length} plugin sets and ${inputs.length} inputs`);
  return {parser, sets};
}

/**
 * The released projection (legacy-fixtures.json): parseMarkdown and parseInline, the
 * BlockNode/InlineNode shapes, for the parser-fixture inputs under the four option
 * variants, and parseMarkdown with sourceRanges.
 */
function exportLegacyFixtures(parser, outDir, blockInputs, inlineInputs, variants) {
  const plain = value => JSON.parse(JSON.stringify(value));
  const blockVariants = {...variants, ranges: {sourceRanges: true}, 'math+ranges': {math: true, sourceRanges: true}};
  const block = blockInputs.map(input => ({
    input,
    results: Object.fromEntries(
      Object.entries(blockVariants).map(([name, options]) => [name, toUtf8Positions(plain(parser.parseMarkdown(input, options)), input)]),
    ),
  }));
  const inline = inlineInputs.map(input => ({
    input,
    results: Object.fromEntries(Object.entries(variants).map(([name, options]) => [name, plain(parser.parseInline(input, options))])),
  }));
  fs.writeFileSync(path.join(outDir, 'legacy-fixtures.json'), stableStringify({block, inline}));
  console.log('wrote ' + block.length + ' block and ' + inline.length + ' inline legacy fixtures');
}

/** Every string literal of a test file (no interpolated templates), for the streaming corpus. */
function stringLiterals(source) {
  const BACKTICK = String.fromCharCode(96);
  const out = [];
  for (let i = 0; i < source.length; i++) {
    const quote = source[i];
    if (quote === '/' && source[i + 1] === '/') {
      i = source.indexOf('\n', i);
      if (i === -1) break;
      continue;
    }
    if (quote !== '"' && quote !== "'" && quote !== BACKTICK) continue;
    let k = i + 1;
    let ok = true;
    while (k < source.length && source[k] !== quote) {
      if (source[k] === '\\') k++;
      else if (quote === BACKTICK && source[k] === '$' && source[k + 1] === '{') { ok = false; break; }
      else if (quote !== BACKTICK && source[k] === '\n') { ok = false; break; }
      k++;
    }
    if (ok && k < source.length) {
      try {
        const value = new Function('return ' + source.slice(i, k + 1) + ';')();
        if (typeof value === 'string') out.push(value);
      } catch {
        // A literal this reader cannot evaluate is skipped.
      }
    }
    i = k;
  }
  return out;
}

/** Inputs of upstream's streaming tests built in code rather than written as literals. */
const TICK = String.fromCharCode(96);
const FENCE = TICK.repeat(3);
const EMOJI = String.fromCodePoint(0x1f600);
const STREAMING_SAMPLES = [
  ['# Overview', '', 'This is an introduction paragraph.', '', FENCE + 'typescript', 'function hello() {', '  return "world";', '}', FENCE, '', '| Feature | Status |', '| --- | --- |', '| Auth | Done |', '', '> A blockquote with important info.', '', '1. First step', '2. Second step', '', '---', '', 'Visit [our site](https://example.com) for more.'].join('\n'),
  ['## Heading', '', 'A paragraph with **bold**, *italic*, and ' + TICK + 'code' + TICK + '.', '', '- List item **one**', '- List item [two](http://two.com)', '- List item ~~three~~', '', '| Col | Val |', '| --- | --- |', '| **a** | *b* |'].join('\n'),
  '# Stable\n\n' + FENCE + 'ts\nconst a = 1;\n\nconst b = 2;\n' + FENCE + '\n\nAfter',
  'Filler paragraph.\n\n'.repeat(6) + 'Costs 5 \\| 10 per unit',
  'Intro\n\nCosts 5 \\| 10 per unit',
  'See [d] here.\n\n[d]: /docs\n',
  'See [d].\n\n[d]: /first\n\nMiddle paragraph\n\n[d]: /second\n\nAfter',
  'Line one  \r\nline two\\\r\nline three\r\n\r\n- a\r\n- b\r\n',
  '- a\n\n* b\n\n- c\n\n+ d',
  ' 1. a\n\n2. b\n\n 3. c',
  '1. a\n\n3. b\n\n4. c\n\nText\n\n5) d',
  '- one\n  - two\n\n    para\n\n- three\n\n' + FENCE + '\n- not a list\n' + FENCE,
  'Text $x$ and $$y$$\n\n$$\na\n\nb\n$$\n\n> $$\n> q\n> $$\n\n- $$\n  l\n  $$\n\nEnd $5 and $',
  'Hi @{Ada} and @{Gr\n\n:::note\nbody\n\n:::\n\n:::note\nopen',
  '---\ntitle: Streamed\n---\n# Body @{x}\n\nText',
  '| a | b |\n| - | - |\n| 1 | 2 |\n\n| lone | header |',
  'Mixed caf' + String.fromCodePoint(0xe9) + ' ' + EMOJI + ' text **bold ' + EMOJI + '** and [link](/x) ' + TICK + 'code' + TICK + '\n\n- ' + String.fromCodePoint(0xfc) + '\n- ' + EMOJI + ' item\n\nEnd',
];

/** 64-bit FNV-1a of a string's UTF-8 bytes, as 16 hex digits (the Rust tests compute it too). */
function fnv1a(text) {
  let hash = 0xcbf29ce484222325n;
  for (const byte of Buffer.from(text, 'utf8')) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, '0');
}

/** JSON with every object's keys sorted, compact: the text a step's hash is taken of. */
function canonicalJson(value) {
  if (Array.isArray(value)) return '[' + value.map(canonicalJson).join(',') + ']';
  if (value && typeof value === 'object') {
    return '{' + Object.keys(value).sort().filter(key => value[key] !== undefined).map(key => JSON.stringify(key) + ':' + canonicalJson(value[key])).join(',') + '}';
  }
  return JSON.stringify(value);
}

/**
 * The chunk boundaries a case streams through, as UTF-16 offsets that split no surrogate
 * pair: every one for short inputs; for long ones, every line start and end and an even
 * spread between.
 */
function chunkEnds(input) {
  const valid = end => end > 0 && end < input.length && !(input.charCodeAt(end - 1) >= 0xd800 && input.charCodeAt(end - 1) <= 0xdbff);
  if (input.length <= 64) {
    return Array.from({length: input.length}, (_, i) => i).filter(valid);
  }
  const ends = new Set();
  for (let i = 0; i < input.length; i++) {
    if (input[i] === '\n') {
      ends.add(i);
      ends.add(i + 1);
    }
  }
  const step = Math.max(1, Math.floor(input.length / 32));
  for (let i = step; i < input.length; i += step) ends.add(i);
  for (const tail of [1, 2, 3]) ends.add(input.length - tail);
  return [...ends].filter(valid).sort((a, b) => a - b);
}

/**
 * Streaming fixtures (streaming-fixtures.json): upstream's parseMarkdownAstIncremental (or,
 * for the legacy cases, parseMarkdownIncremental's released shape with sourceRanges) fed
 * each chunk boundary of an input with isFinal false, then the whole input with isFinal
 * true, all in one state; a trimmed case feeds trimStreamingArtifacts' output instead, as
 * Markdown does. Each step records the hash of its canonical JSON; the last streamed step
 * and the final result are recorded whole. The corpus is the streaming tests' literals,
 * the samples above and the parser-fixture inputs; plugin sets stream the plugin samples.
 * Also trimStreamingArtifacts at every boundary, with and without math.
 */
function exportStreamingFixtures(parser, sets, core, outDir, blockInputs) {
  const testSources = ['incremental.test.ts', 'streamingListShapes.test.tsx', 'incrementalIndentation.test.tsx']
    .map(name => fs.readFileSync(path.join(core, name), 'utf8'));
  const significant = new RegExp('[\\n#*_\\[\\]|>$~!:@' + TICK + '-]');
  const fromTests = testSources.flatMap(stringLiterals).filter(value => value.length >= 2 && significant.test(value));
  const streamingInputs = [...new Set([...fromTests, ...STREAMING_SAMPLES])];
  const corpus = [...new Set([...streamingInputs, ...blockInputs])].filter(input => input.length > 0);
  const citationLike = new RegExp('\\[|' + String.fromCodePoint(0x3010));
  const plain = value => JSON.parse(JSON.stringify(value));
  const bytes = (text, end) => Buffer.byteLength(text.slice(0, end), 'utf8');
  const quiet = run => {
    const warn = console.warn;
    const error = console.error;
    console.warn = () => {};
    console.error = () => {};
    try {
      return run();
    } finally {
      console.warn = warn;
      console.error = error;
    }
  };

  const cases = [];
  const stream = (input, variant, options, {legacy = false, trimmed = false} = {}) => {
    const ends = chunkEnds(input);
    const state = parser.createIncrementalState();
    const parse = (text, isFinal) => {
      const opts = {...options, isFinal};
      const result = legacy
        ? parser.parseMarkdownIncremental(text, state, opts)
        : parser.parseMarkdownAstIncremental(text, state, opts);
      return toUtf8Positions(plain(result), text);
    };
    const prepare = text => (trimmed ? parser.trimStreamingArtifacts(text, options) : text);
    quiet(() => {
      const hashes = [];
      let last = null;
      for (const end of ends) {
        last = parse(prepare(input.slice(0, end)), false);
        hashes.push(fnv1a(canonicalJson(last)));
      }
      const final = parse(prepare(input), true);
      const entry = {input, variant};
      if (legacy) entry.legacy = true;
      if (trimmed) entry.trimmed = true;
      Object.assign(entry, {ends: ends.map(end => bytes(input, end)), hashes, last, final, settledText: state.settledText});
      cases.push(entry);
    });
  };

  const variants = {
    default: {},
    math: {math: true},
    autolink: {autolink: 'gfm'},
    citations: {sourceIds: new Set(['1', '2', 'src', 'a'])},
  };
  corpus.forEach((input, index) => {
    const fromStreamingTests = index < streamingInputs.length;
    stream(input, 'default', variants.default);
    stream(input, 'math', variants.math);
    if (/https?:|www\.|@/.test(input)) stream(input, 'autolink', variants.autolink);
    if (citationLike.test(input)) stream(input, 'citations', variants.citations);
    if (fromStreamingTests || index % 3 === 0) {
      stream(input, 'default', {sourceRanges: true}, {legacy: true});
      stream(input, 'math', {math: true, sourceRanges: true}, {legacy: true});
    }
    if (fromStreamingTests || index % 2 === 0) {
      stream(input, 'default', variants.default, {trimmed: true});
      stream(input, 'math', variants.math, {trimmed: true});
    }
  });
  const pluginStreams = ['mentions', 'callouts', 'mentions+callouts', 'finality', 'source-lines', 'source-lines+callouts', 'frontmatter', 'frontmatter+source-lines', 'charts', 'soft-breaks', 'heading-links+mentions'];
  const pluginInputs = [...new Set([...PLUGIN_SAMPLES, ...STREAMING_SAMPLES, ...blockInputs.filter((_, index) => index % 9 === 0)])];
  for (const set of pluginStreams) {
    for (const input of pluginInputs) {
      stream(input, set, {plugins: sets[set]});
    }
  }

  const trims = corpus.map(input => {
    const ends = [...chunkEnds(input), input.length];
    // Each trimmed text as the bytes of the prefix it keeps and what follows them.
    const encode = (prefix, trimmed) => {
      let keep = 0;
      while (keep < prefix.length && keep < trimmed.length && prefix[keep] === trimmed[keep]) keep++;
      if (keep > 0 && prefix.charCodeAt(keep - 1) >= 0xd800 && prefix.charCodeAt(keep - 1) <= 0xdbff) keep--;
      return [Buffer.byteLength(prefix.slice(0, keep), 'utf8'), trimmed.slice(keep)];
    };
    const at = math => ends.map(end => encode(input.slice(0, end), parser.trimStreamingArtifacts(input.slice(0, end), {math})));
    return {input, ends: ends.map(end => bytes(input, end)), plain: at(false), math: at(true)};
  });

  fs.writeFileSync(
    path.join(outDir, 'streaming-fixtures.json'),
    '{"trims": [\n' + trims.map(entry => JSON.stringify(entry)).join(',\n') + '\n],\n"cases": [\n' + cases.map(entry => JSON.stringify(entry)).join(',\n') + '\n]}\n',
  );
  const steps = cases.reduce((sum, entry) => sum + entry.hashes.length + 1, 0);
  const trimCount = trims.reduce((sum, entry) => sum + 2 * entry.ends.length, 0);
  console.log('wrote ' + cases.length + ' streaming fixtures (' + steps + ' parses over ' + corpus.length + ' inputs) and ' + trimCount + ' trims');
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
  const {parser: pluginParser, sets} = await exportPluginFixtures(root, core, outDir, warnings, blockInputs);
  exportLegacyFixtures(parser, outDir, blockInputs, inlineInputs, variants);
  exportStreamingFixtures(pluginParser, sets, core, outDir, blockInputs);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
  const root = astryxRoot(process.argv[2] ?? path.join(repoRoot, '..', 'Astryx'));
  const warnings = [];
  await exportMarkdown(root, path.join(repoRoot, 'spec'), warnings);
  for (const w of warnings) console.warn(`warning: ${w}`);
}
