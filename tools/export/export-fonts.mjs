// Downloads the static TTF of every font face the themes use, as Google
// Fonts serves it, into crates/astryx-themes/fonts/<family>/. GPUI needs raw
// TTF/OTF; the themes ship woff2 subsets. Every Astryx theme font is a Google
// Font. Existing files are kept, so a re-run is offline unless a face is new.
//
// The request carries only the family, weight and style.

import fs from 'node:fs';
import path from 'node:path';

const familySlug = family => family.toLowerCase().replace(/\s+/g, '-');
export const fontFileName = (family, weight, style) =>
  `${family.replace(/\s+/g, '')}-${weight}${style === 'italic' ? 'Italic' : ''}.ttf`;

export async function exportFonts(root, repoRoot, specDir, warnings) {
  const fontsDir = path.join(repoRoot, 'crates', 'astryx-themes', 'fonts');
  const faces = new Map();
  for (const file of fs.readdirSync(path.join(specDir, 'themes'))) {
    const theme = JSON.parse(fs.readFileSync(path.join(specDir, 'themes', file), 'utf8'));
    for (const face of theme.fonts) {
      faces.set(`${face.family}|${face.weight}|${face.style}`, {...face, theme: theme.name});
    }
  }
  let downloaded = 0;
  for (const {family, weight, style, theme} of faces.values()) {
    const dir = path.join(fontsDir, familySlug(family));
    const file = path.join(dir, fontFileName(family, weight, style));
    fs.mkdirSync(dir, {recursive: true});
    copyLicense(root, theme, family, dir, warnings);
    if (fs.existsSync(file)) continue;
    const axis = style === 'italic' ? `ital,wght@1,${weight}` : `wght@${weight}`;
    const query = `${encodeURIComponent(family).replace(/%20/g, '+')}:${axis}`;
    const cssText = await (await fetch(`https://fonts.googleapis.com/css2?family=${query}`)).text();
    const url = /url\((https:[^)]+\.ttf)\)/.exec(cssText)?.[1];
    if (!url) {
      warnings.push(`font ${family} ${weight} ${style}: Google Fonts has no TTF; left out`);
      continue;
    }
    const response = await fetch(url);
    if (!response.ok) {
      warnings.push(`font ${family} ${weight} ${style}: download failed (${response.status})`);
      continue;
    }
    fs.writeFileSync(file, Buffer.from(await response.arrayBuffer()));
    downloaded++;
  }
  console.log(`fonts: ${faces.size} faces, ${downloaded} downloaded`);
}

function copyLicense(root, theme, family, dir, warnings) {
  const target = path.join(dir, 'OFL.txt');
  if (fs.existsSync(target)) return;
  const fontsDir = path.join(root, 'packages', 'themes', theme, 'fonts');
  const slug = familySlug(family);
  const license = fs
    .readdirSync(fontsDir)
    .find(f => f.endsWith('.LICENSE.txt') && f.toLowerCase().startsWith(slug));
  if (license) fs.copyFileSync(path.join(fontsDir, license), target);
  else warnings.push(`font ${family}: no license file in ${fontsDir}`);
}
