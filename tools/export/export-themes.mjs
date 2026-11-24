// Writes spec/themes/<name>.json for core's defaults and every published
// theme package: typed tokens per mode, component override rules,
// adaptations, inverted-surface overrides, icon mappings and font faces.
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
  for (const p of swift.publishedThemePackages()) {
    const module = await importTs(root, p.sourceFile);
    const theme = module[p.exportName];
    if (!theme) throw new Error(`${p.packageName} does not export ${p.exportName}`);
    sources.push({theme, name: theme.name.replace(/^astryx-/, ''), package: p.packageName, dir: p.dir});
  }

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
