#!/usr/bin/env node
// Re-exports everything the Rust port reads from an upstream Astryx checkout.
//
//   node tools/export/export-all.mjs [path/to/Astryx] [--strict] [--no-fonts]
//
// Writes spec/ (themes, icons, manifest, locales, cldr.json, code/, markdown/, emoji.json, upstream.json), the icon SVGs in
// crates/astryx-icons/assets, and the theme fonts in crates/astryx-themes/fonts.
// Then run `cargo xtask codegen`. See UPSTREAM.md.

import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {astryxRoot, upstreamCommit} from './lib/astryx.mjs';
import {stableStringify} from './lib/values.mjs';
import {exportThemes} from './export-themes.mjs';
import {exportIcons} from './export-icons.mjs';
import {exportFonts} from './export-fonts.mjs';
import {exportManifest} from './export-manifest.mjs';
import {exportCldr} from './export-cldr.mjs';
import {exportCode} from './export-code.mjs';
import {exportMarkdown} from './export-markdown.mjs';
import {exportEmoji} from './export-emoji.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const args = process.argv.slice(2);
const strict = args.includes('--strict');
const root = astryxRoot(args.find(a => !a.startsWith('--')) ?? path.join(repoRoot, '..', 'Astryx'));
const specDir = path.join(repoRoot, 'spec');
fs.mkdirSync(specDir, {recursive: true});

const warnings = [];
const {themes, lucideComponents} = await exportThemes(root, specDir, warnings);
await exportIcons(root, repoRoot, specDir, lucideComponents, warnings);
await exportManifest(root, specDir, warnings);
await exportCldr(root, specDir, warnings);
await exportCode(root, specDir, warnings);
await exportMarkdown(root, specDir, warnings);
await exportEmoji(root, specDir, warnings);
if (!args.includes('--no-fonts')) await exportFonts(root, repoRoot, specDir, warnings);

const localesDir = path.join(specDir, 'locales');
fs.rmSync(localesDir, {recursive: true, force: true});
fs.mkdirSync(localesDir, {recursive: true});
const locales = fs.readdirSync(path.join(root, 'packages', 'core', 'locales')).filter(f => f.endsWith('.json'));
for (const file of locales) {
  fs.copyFileSync(path.join(root, 'packages', 'core', 'locales', file), path.join(localesDir, file));
}
console.log(`copied ${locales.length} locale catalogs`);

fs.writeFileSync(
  path.join(specDir, 'upstream.json'),
  stableStringify({commit: upstreamCommit(root), themes}),
);

for (const warning of warnings) console.warn(`warning: ${warning}`);
console.log(`${warnings.length} warning(s)`);
if (strict && warnings.length) process.exitCode = 1;
