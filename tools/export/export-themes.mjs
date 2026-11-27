// Writes spec/themes/<name>.json for core's defaults and every published
// theme package: typed tokens per mode, component override rules,
// adaptations, inverted-surface overrides, icon mappings and font faces.
// Also writes spec/theme-definitions/<name>.json: each theme's own
// `defineTheme` input (its JSON-able part), which the Rust port's
// `define_theme` is checked against, and spec/palettes/<name>.json: each
// generated palette's request (from its receipt) and output, which the Rust
// palette generator is checked against (M12).
//
// Values come only from core's resolver (`resolveThemeTokens`), as in the
// upstream Swift generator (packages/native/specs/swift/library/theme-generator.md G1).

import fs from 'node:fs';
import path from 'node:path';
import {importFile, importTs} from './lib/astryx.mjs';
import {
  loadConverters,
  propertyKind,
  stableStringify,
  substituteVars,
  tokenKind,
  typed,
} from './lib/values.mjs';

const MODES = ['light', 'dark'];
let css;

export async function exportThemes(root, specDir, warnings) {
  css = await loadConverters(root);
  const swift = await importFile(root, 'packages/native/scripts/swift-ios-theme.mjs');
  const {resolveThemeTokens} = await importFile(root, 'packages/core/dist/theme/tokens.js');

  const sources = [{theme: null, name: 'default', package: '@astryxdesign/core', dir: null}];
  globalThis.__astryxDefinitions = new Map();
  for (const p of swift.publishedThemePackages()) {
    const module = await importTs(root, p.sourceFile, [recordDefinitions()]);
    const theme = module[p.exportName];
    if (!theme) throw new Error(`${p.packageName} does not export ${p.exportName}`);
    sources.push({theme, name: theme.name.replace(/^astryx-/, ''), package: p.packageName, dir: p.dir});
  }
  writeDefinitions(specDir, sources);
  await writePalettes(root, specDir, sources);
  await writeTokenDefaults(root, specDir);

  const outDir = path.join(specDir, 'themes');
  fs.mkdirSync(outDir, {recursive: true});
  const iconUses = {};
  for (const source of sources) {
    const json = exportTheme(source, resolveThemeTokens, warnings);
    for (const icon of Object.values(json.icons)) {
      if (icon.lucide) iconUses[icon.lucide] = true;
    }
    fs.writeFileSync(path.join(outDir, `${source.name}.json`), stableStringify(json));
    console.log(`wrote spec/themes/${source.name}.json`);
  }
  return {themes: sources.map(s => s.name), lucideComponents: Object.keys(iconUses).sort()};
}

function exportTheme(source, resolveThemeTokens, warnings) {
  const {theme, name} = source;
  const raw = Object.fromEntries(
    MODES.map(mode => [mode, resolveThemeTokens(theme, {mode})]),
  );
  const tokens = Object.fromEntries(
    MODES.map(mode => [mode, convertTokens(raw[mode], raw[mode], mode, `${name} ${mode}`, warnings)]),
  );
  for (const token of Object.keys(tokens.light)) {
    if (!(token in tokens.dark)) warnings.push(`${name}: ${token} is missing in dark`);
  }
  return {
    name,
    package: source.package,
    tokens,
    components: theme ? exportComponents(theme, raw, name, warnings) : [],
    adaptations: (theme?.__adaptationRules ?? []).map(rule => ({
      when: rule.when,
      query: rule.query,
      tokens: Object.fromEntries(
        MODES.map(mode => [
          mode,
          convertTokens(rule.tokens, {...raw[mode], ...rule.tokens}, mode, `${name} adaptation ${mode}`, warnings),
        ]),
      ),
    })),
    widthBreakpoints: theme?.__adaptations?.widthBreakpoints ?? null,
    onDark: theme?.__onDark ? surface(theme.__onDark, raw.dark, 'dark', name, warnings) : null,
    onLight: theme?.__onLight ? surface(theme.__onLight, raw.light, 'light', name, warnings) : null,
    icons: exportIcons(theme, name, warnings),
    fonts: source.dir ? fontFaces(source.dir) : [],
  };
}

/**
 * Converts every `--*` entry of `overrides` in one mode. `scope` is the
 * token map that var() references resolve against.
 */
function convertTokens(overrides, scope, mode, where, warnings) {
  const out = {};
  const entries = Object.entries(overrides)
    .filter(([token, value]) => token.startsWith('--') && typeof value === 'string')
    // A length line height divides by its style's size, so sizes go first.
    .sort(([a], [b]) => Number(a.endsWith('-leading')) - Number(b.endsWith('-leading')));
  for (const [token, value] of entries) {
    const notes = [];
    try {
      const resolved = css.valueForMode(substituteVars(value, scope), mode);
      const size = out[token.replace(/-leading$/, '-size')]?.length;
      out[token] = typed(tokenKind(token), resolved, notes, {fontSize: size});
    } catch (error) {
      warnings.push(`${where}: ${token} = ${value}: ${error.message}; left out`);
      continue;
    }
    for (const note of notes) warnings.push(`${where}: ${token} = ${value}: ${note}`);
  }
  return out;
}

function surface(spec, scope, mode, name, warnings) {
  const {['color-scheme']: colorScheme, ...rest} = spec.tokens ?? {};
  return {
    colorScheme: colorScheme ?? mode,
    tokens: convertTokens(rest, {...scope, ...rest}, mode, `${name} on-${mode}`, warnings),
  };
}

/**
 * Component overrides as flat rules. A selector key `variant:blue+size:sm`
 * becomes conditions [["variant","blue"],["size","sm"]]; a bare key such as
 * `selected` is the condition ["selected","true"]; `base` has none. A nested
 * selector object becomes its own rule with `selector` set. `state` names a
 * plain interaction pseudo-class GPUI can style; any other selector is left
 * for hand-porting.
 */
function exportComponents(theme, raw, name, warnings) {
  const rules = [];
  for (const [target, byKey] of Object.entries(theme.components ?? {})) {
    for (const [key, decls] of Object.entries(byKey)) {
      const when =
        key === 'base'
          ? []
          : key.split('+').map(part => {
              const i = part.indexOf(':');
              return i === -1 ? [part, 'true'] : [part.slice(0, i), part.slice(i + 1)];
            });
      collectRule(rules, {target, when, selector: null}, decls, raw, `${name} ${target}[${key}]`, warnings);
    }
  }
  return rules;
}

const STATES = {':hover': 'hover', ':active': 'active', ':focus-visible': 'focus-visible'};

function collectRule(rules, rule, decls, raw, where, warnings) {
  const props = {};
  const unsupported = [];
  for (const [property, value] of Object.entries(decls)) {
    if (value && typeof value === 'object') {
      collectRule(
        rules,
        {...rule, selector: (rule.selector ?? '') + property},
        value,
        raw,
        `${where}${property}`,
        warnings,
      );
      continue;
    }
    const kind = propertyKind(property);
    try {
      props[property] = Object.fromEntries(
        MODES.map(mode => {
          const notes = [];
          const read = v => css.valueForMode(substituteVars(String(v), raw[mode]), mode);
          const fontSize =
            property === 'lineHeight' && decls.fontSize
              ? typed('length', read(decls.fontSize), []).length
              : undefined;
          const out = typed(kind, read(value), notes, {property, fontSize});
          for (const note of notes) warnings.push(`${where} ${mode}: ${property}: ${note}`);
          return [mode, out];
        }),
      );
    } catch (error) {
      unsupported.push({property, value: String(value), reason: error.message});
    }
  }
  const state = rule.selector === null ? null : (STATES[rule.selector] ?? null);
  rules.push({...rule, state, props, unsupported});
}

/**
 * The theme's icon registry: each semantic name maps to the lucide component
 * the theme renders, or to core's fallback when the theme has none.
 */
function exportIcons(theme, name, warnings) {
  const out = {};
  for (const [icon, element] of Object.entries(theme?.icons ?? {})) {
    const component = element?.type?.render?.name ?? element?.type?.displayName;
    if (!component) {
      warnings.push(`${name}: icon ${icon} is not a lucide component; left out`);
      continue;
    }
    const props = {...(element.props ?? {})};
    delete props.size;
    delete props['aria-hidden'];
    out[icon] = {lucide: component, ...(Object.keys(props).length ? {props} : {})};
  }
  return out;
}

/** The theme's font faces from its fonts.css: unique family/weight/style. */
function fontFaces(dir) {
  const file = path.join(dir, 'fonts.css');
  if (!fs.existsSync(file)) return [];
  const faces = new Map();
  for (const block of fs.readFileSync(file, 'utf8').split('@font-face').slice(1)) {
    const family = /font-family:\s*['"]?([^;'"]+)['"]?;/.exec(block)?.[1];
    const weight = /font-weight:\s*([^;]+);/.exec(block)?.[1]?.trim() ?? '400';
    const style = /font-style:\s*([^;]+);/.exec(block)?.[1]?.trim() ?? 'normal';
    if (!family) continue;
    // A variable face (`font-weight: 100 900`) becomes static faces at each
    // hundred in its range, since GPUI selects static faces by weight.
    const [low, high = low] = weight.split(/\s+/).map(Number);
    for (let w = Math.ceil(low / 100) * 100; w <= high; w += 100) {
      faces.set(`${family}|${w}|${style}`, {family, weight: String(w), style});
    }
  }
  return [...faces.values()].sort((a, b) =>
    `${a.family}${a.weight}${a.style}`.localeCompare(`${b.family}${b.weight}${b.style}`),
  );
}

/**
 * A Vite plugin that makes each `defineTheme(...)` call in a theme package's
 * sources record its input by theme name. (Vite runs `@astryxdesign/core`
 * outside its pipeline, so the call sites are rewritten rather than the import.)
 */
function recordDefinitions() {
  globalThis.__astryxRecordDefinition = (define, input) => {
    const theme = define(input);
    globalThis.__astryxDefinitions.set(theme.name, input);
    return theme;
  };
  return {
    name: 'astryx-record-definitions',
    enforce: 'pre',
    transform(code, id) {
      if (!/[\\/]packages[\\/]themes[\\/]/.test(id) || !code.includes('defineTheme(')) return null;
      return code.replace(/\bdefineTheme\(/g, 'globalThis.__astryxRecordDefinition(defineTheme, ');
    },
  };
}

/**
 * A definition as JSON: functions and React nodes (icons, indicators) drop
 * out, and a base theme (`extends`) is named rather than copied.
 */
function definitionJson(input) {
  return JSON.parse(
    JSON.stringify(input, (key, value) => {
      if (typeof value === 'function') return undefined;
      if (value && typeof value === 'object' && '$$typeof' in value) return undefined;
      if (key === 'extends' && value && typeof value === 'object') return {name: value.name};
      return value;
    }),
  );
}

function writeDefinitions(specDir, sources) {
  const dir = path.join(specDir, 'theme-definitions');
  fs.rmSync(dir, {recursive: true, force: true});
  fs.mkdirSync(dir, {recursive: true});
  for (const source of sources) {
    if (!source.theme) continue;
    const input = globalThis.__astryxDefinitions.get(source.theme.name);
    if (!input) throw new Error(`${source.name}: its defineTheme input was not recorded`);
    // Upstream's key order, not sorted: a theme's later component rules win.
    fs.writeFileSync(
      path.join(dir, `${source.name}.json`),
      JSON.stringify(definitionJson(input), null, 2) + '\n',
    );
    console.log(`wrote spec/theme-definitions/${source.name}.json`);
  }
}

/**
 * Each theme's generated palette: the generator request its receipt records
 * and the palette the generator wrote (`<theme>Palettes.generated.ts`).
 */
async function writePalettes(root, specDir, sources) {
  const dir = path.join(specDir, 'palettes');
  fs.rmSync(dir, {recursive: true, force: true});
  fs.mkdirSync(dir, {recursive: true});
  for (const source of sources) {
    if (!source.dir) continue;
    const src = path.join(source.dir, 'src');
    const receipt = fs.readdirSync(src).find(f => f.endsWith('Palettes.generated.receipt.json'));
    if (!receipt) continue;
    const generated = path.join(src, receipt.replace('.receipt.json', '.ts'));
    const module = await importTs(root, generated);
    const {request, recipe, generatorVersion} = JSON.parse(fs.readFileSync(path.join(src, receipt), 'utf8'));
    const out = {recipe, generatorVersion, request, black: module.black, white: module.white, palette: module.palette};
    fs.writeFileSync(path.join(dir, `${source.name}.json`), stableStringify(out));
    console.log(`wrote spec/palettes/${source.name}.json`);
  }
}

/**
 * Every token's default as upstream's `tokenDefaults` writes it (a CSS value,
 * `light-dark(...)` for a pair), in its order: the flat map `defineTheme` and
 * the token resolvers start from.
 */
async function writeTokenDefaults(root, specDir) {
  const {tokenDefaults} = await importFile(root, 'packages/core/dist/theme/tokenDefaults.js');
  fs.writeFileSync(
    path.join(specDir, 'token-defaults.json'),
    JSON.stringify(Object.entries(tokenDefaults), null, 1) + '\n',
  );
  console.log('wrote spec/token-defaults.json');
}
