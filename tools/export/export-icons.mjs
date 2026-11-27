// Writes the icon SVGs that rustryx-icons embeds, plus spec/icons.json:
// - core's fallback icons (packages/core/src/Icon/defaultIcons.tsx), rendered
//   to static SVG by react-dom/server;
// - every lucide icon a theme maps a semantic name to, built from lucide's
//   own icon data (`__iconData` in lucide-react's ESM files).
//
// Width and height are dropped so the SVG sizes from its viewBox; GPUI scales
// it to the element and tints it with the text color.

import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
import {importFile} from './lib/astryx.mjs';
import {stableStringify} from './lib/values.mjs';

export async function exportIcons(root, repoRoot, specDir, lucideComponents, warnings) {
  const assets = path.join(repoRoot, 'crates', 'rustryx-icons', 'assets');
  const core = await exportCoreIcons(root, path.join(assets, 'core'));
  const lucide = await exportLucideIcons(root, path.join(assets, 'lucide'), lucideComponents, warnings);
  fs.writeFileSync(path.join(specDir, 'icons.json'), stableStringify({core, lucide}));
  console.log(`wrote ${core.length} core and ${Object.keys(lucide).length} lucide icons`);
}

function resetDir(dir) {
  fs.rmSync(dir, {recursive: true, force: true});
  fs.mkdirSync(dir, {recursive: true});
}

/** File stem for an icon name; namespaced names (`richtext:bold`) use `__`. */
export const iconFileStem = name => name.replace(/:/g, '__');

async function exportCoreIcons(root, outDir) {
  resetDir(outDir);
  const {defaultIcons} = await importFile(root, 'packages/core/dist/Icon/defaultIcons.js');
  const require = createRequire(path.join(root, 'packages', 'core', 'package.json'));
  const {renderToStaticMarkup} = require('react-dom/server');
  const names = Object.keys(defaultIcons).sort();
  for (const name of names) {
    const svg = renderToStaticMarkup(defaultIcons[name])
      .replace(/\s(width|height|aria-hidden)="[^"]*"/g, '')
      .replace(/<svg/, '<svg xmlns="http://www.w3.org/2000/svg"')
      .replace(/(xmlns="[^"]*")\s+xmlns="[^"]*"/, '$1');
    fs.writeFileSync(path.join(outDir, `${iconFileStem(name)}.svg`), svg + '\n');
  }
  return names;
}

const LUCIDE_ATTRS =
  'xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"';

const kebab = name => name.replace(/[A-Z]/g, (c, i) => (i ? '-' : '') + c.toLowerCase());

async function exportLucideIcons(root, outDir, components, warnings) {
  resetDir(outDir);
  const pkg = fs.realpathSync(path.join(root, 'packages', 'themes', 'neutral', 'node_modules', 'lucide-react'));
  const iconsDir = path.join(pkg, 'dist', 'esm', 'icons');
  const index = new Map();
  for (const file of fs.readdirSync(iconsDir).filter(f => f.endsWith('.mjs'))) {
    const text = fs.readFileSync(path.join(iconsDir, file), 'utf8');
    const m = /const (\w+) = createLucideIcon/.exec(text);
    if (m) index.set(m[1], file);
  }
  fs.copyFileSync(path.join(pkg, 'LICENSE'), path.join(outDir, 'LICENSE'));
  const out = {};
  for (const component of components) {
    const file = index.get(component);
    if (!file) {
      warnings.push(`lucide component ${component} not found; left out`);
      continue;
    }
    const {__iconData: data} = await import(pathToFileURL(path.join(iconsDir, file)).href);
    const children = data.node
      .map(([tag, attrs]) => {
        const list = Object.entries(attrs)
          .filter(([k]) => k !== 'key')
          .map(([k, v]) => `${kebab(k)}="${v}"`)
          .join(' ');
        return `<${tag} ${list}/>`;
      })
      .join('');
    fs.writeFileSync(path.join(outDir, `${data.name}.svg`), `<svg ${LUCIDE_ATTRS}>${children}</svg>\n`);
    out[component] = data.name;
  }
  return out;
}
