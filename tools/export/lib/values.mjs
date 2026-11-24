// Converts resolved CSS values to the typed JSON that `cargo xtask codegen`
// reads. Colors, lengths, weights and line heights go through the upstream
// Swift generator's converters (packages/native/scripts/css-values.mjs), so
// both native ports read CSS the same way. Shadows, durations, easings and
// edges are read here, because GPUI draws what SwiftUI cannot (shadow
// spread, for one).
//
// Every typed value is a one-key object, so the Rust reader can match on the
// key: {color}, {length, rem}, {weight}, {leading}, {duration}, {ease},
// {families}, {shadow}, {edges}, {keyword}, {number}.

import {importFile} from './astryx.mjs';

let css;
export async function loadConverters(root) {
  css = await importFile(root, 'packages/native/scripts/css-values.mjs');
  return css;
}

/** The kind of value a token holds, from its name. */
export function tokenKind(name) {
  if (/^--shadow-/.test(name)) return 'shadow';
  if (/duration/.test(name)) return 'duration';
  if (/^--ease-/.test(name)) return 'ease';
  if (/^--font-family-/.test(name)) return 'families';
  if (/^--font-weight-|^--text-.*-weight$/.test(name)) return 'weight';
  if (/^--text-.*-leading$/.test(name)) return 'leading';
  if (/-style$/.test(name)) return 'keyword';
  if (/^--color-|-color$/.test(name)) return 'color';
  return 'length';
}

/** The kind of value a CSS property in a component override holds. */
export function propertyKind(property) {
  if (property.startsWith('--')) return tokenKind(property);
  switch (property) {
    case 'backgroundColor':
    case 'color':
    case 'borderColor':
    case 'outlineColor':
      return 'color';
    case 'fontFamily':
      return 'families';
    case 'fontWeight':
      return 'weight';
    case 'lineHeight':
      return 'leading';
    case 'boxShadow':
      return 'shadow';
    case 'borderStyle':
      return 'keyword';
    case 'padding':
    case 'paddingInline':
    case 'paddingBlock':
    case 'margin':
      return 'edges';
    case 'opacity':
      return 'number';
    default:
      return 'length';
  }
}

/** Splits `text` at top-level occurrences of `separator` (outside parens). */
export function splitTopLevel(text, separator = ',') {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '(') depth++;
    else if (c === ')') depth--;
    else if (depth === 0 && (separator === ' ' ? /\s/.test(c) : c === separator)) {
      parts.push(text.slice(start, i).trim());
      start = i + 1;
    }
  }
  parts.push(text.slice(start).trim());
  return parts.filter(p => p !== '');
}

/**
 * Replaces every `var(--x[, fallback])` with the value of `--x` in `tokens`,
 * recursively, so a value can be read on its own.
 */
export function substituteVars(value, tokens, depth = 0) {
  if (depth > 20) throw new Error(`var() nesting too deep in "${value}"`);
  const start = value.indexOf('var(');
  if (start === -1) return value;
  let close = start + 4;
  for (let d = 1; close < value.length && d > 0; close++) {
    if (value[close] === '(') d++;
    else if (value[close] === ')') d--;
  }
  const inner = value.slice(start + 4, close - 1);
  const [name, ...fallback] = splitTopLevel(inner);
  let replacement = tokens[name.trim()];
  if (replacement === undefined) {
    if (!fallback.length) throw new Error(`var(${name}) is not defined`);
    replacement = fallback.join(', ');
  }
  const expanded = substituteVars(String(replacement), tokens, depth + 1);
  return substituteVars(value.slice(0, start) + expanded + value.slice(close), tokens, depth + 1);
}

function color(value, notes) {
  const out = css.toColor(value, notes);
  if (out.startsWith('display-p3')) {
    throw new Error(`"${value}" is outside sRGB, which GPUI cannot draw`);
  }
  return out;
}

function length(value, notes) {
  const trimmed = value.trim();
  return {length: css.toPoints(trimmed, notes), rem: /^-?[\d.]+rem$/.test(trimmed)};
}

function duration(value) {
  const m = /^(-?[\d.]+)(ms|s)$/.exec(value.trim());
  if (!m) throw new Error(`"${value}" is not a duration`);
  return Number(m[1]) * (m[2] === 's' ? 1000 : 1);
}

const EASE_KEYWORDS = {
  linear: [0, 0, 1, 1],
  ease: [0.25, 0.1, 0.25, 1],
  'ease-in': [0.42, 0, 1, 1],
  'ease-out': [0, 0, 0.58, 1],
  'ease-in-out': [0.42, 0, 0.58, 1],
};

function ease(value) {
  const v = value.trim();
  if (EASE_KEYWORDS[v]) return EASE_KEYWORDS[v];
  const m = /^cubic-bezier\(([^)]*)\)$/.exec(v);
  if (!m) throw new Error(`"${value}" is not a cubic-bezier easing`);
  const points = m[1].split(',').map(Number);
  if (points.length !== 4 || points.some(Number.isNaN)) {
    throw new Error(`"${value}" needs four numbers`);
  }
  return points;
}

function families(value) {
  return splitTopLevel(value).map(f => f.replace(/^["']|["']$/g, '').trim());
}

/** A `box-shadow` list as layers, keeping spread, which GPUI draws. */
function shadow(value, notes) {
  if (value.trim() === 'none') return [];
  return splitTopLevel(value).map(layer => {
    const lengths = [];
    let layerColor;
    let inset = false;
    for (const part of splitTopLevel(layer, ' ')) {
      if (part === 'inset') inset = true;
      else if (/^-?[\d.]/.test(part) || /^calc\(/.test(part)) lengths.push(css.toPoints(part, notes));
      else if (layerColor) throw new Error(`shadow layer "${layer}" has two colors`);
      else layerColor = color(part, notes);
    }
    if (lengths.length < 2 || lengths.length > 4) {
      throw new Error(`shadow layer "${layer}" needs two to four lengths`);
    }
    if (!layerColor) throw new Error(`shadow layer "${layer}" has no color`);
    const [x, y, blur = 0, spread = 0] = lengths;
    return {x, y, blur, spread, inset, color: layerColor};
  });
}

/** CSS edge shorthand (`padding: 4px 8px`) as [top, right, bottom, left]. */
function edges(value, notes, property) {
  const parts = splitTopLevel(value, ' ').map(p => length(p, notes));
  if (property === 'paddingInline') {
    const [start, end = start] = parts;
    return {inline: [start, end]};
  }
  if (property === 'paddingBlock') {
    const [start, end = start] = parts;
    return {block: [start, end]};
  }
  const [t, r = t, b = t, l = r] = parts;
  return {all: [t, r, b, l]};
}

/**
 * Converts one value, already split to one mode and with every var()
 * substituted, to its typed form. `context.fontSize` (px) reads a length
 * line height. Throws on a value with no static form.
 */
export function typed(kind, value, notes, context = {}) {
  const v = String(value).trim();
  switch (kind) {
    case 'color':
      return {color: color(v, notes)};
    case 'length':
      return length(v, notes);
    case 'weight':
      return {weight: css.toWeight(v, notes)};
    case 'leading':
      return {leading: css.toLeading(v, context.fontSize, notes)};
    case 'duration':
      return {duration: duration(v)};
    case 'ease':
      return {ease: ease(v)};
    case 'families':
      return {families: families(v)};
    case 'shadow':
      return {shadow: shadow(v, notes)};
    case 'edges':
      return {edges: edges(v, notes, context.property)};
    case 'keyword':
      return {keyword: v};
    case 'number': {
      const n = Number(v);
      if (Number.isNaN(n)) throw new Error(`"${v}" is not a number`);
      return {number: n};
    }
    default:
      throw new Error(`unknown kind ${kind}`);
  }
}

/** JSON with object keys sorted, so the same input gives the same bytes. */
export function stableStringify(value) {
  return JSON.stringify(sortKeys(value), null, 2) + '\n';
}

function sortKeys(value) {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map(k => [k, sortKeys(value[k])]),
    );
  }
  return value;
}
