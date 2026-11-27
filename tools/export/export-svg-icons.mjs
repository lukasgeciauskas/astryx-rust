// Writes spec/svg-icons.json: the starter icons of lab's SVGIcon and WeightedSVGIcon, read
// from upstream's packages/lab/src/SVGIcon/icons.ts (SVGIconDef objects: each shape's
// element type, attributes and role) and weightedIcons.ts (generated WeightedIconDef
// objects: one filled drawing per weight range). Attributes keep their order, as pairs.
// `cargo xtask codegen` turns it into crates/astryx-lab/src/svg_icon/generated/icons.rs.
//
//   node tools/export/export-svg-icons.mjs [path/to/Astryx]   (also run by export-all.mjs)

import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {astryxRoot, importTs} from './lib/astryx.mjs';
import {stableStringify} from './lib/values.mjs';

const ICONS = 'packages/lab/src/SVGIcon/icons.ts';
const WEIGHTED = 'packages/lab/src/SVGIcon/weightedIcons.ts';

/** Each value of `list`, with the name it is exported under. */
function named(module, list) {
  return list.map(value => {
    const name = Object.keys(module).find(key => module[key] === value);
    if (name == null) {
      throw new Error(`an icon in the list is not exported on its own: ${value.name}`);
    }
    return [name, value];
  });
}

function shape(s) {
  return {
    type: s.type,
    attrs: Object.entries(s.attrs),
    ...(s.role != null ? {role: s.role} : {}),
  };
}

export async function exportSvgIcons(root, specDir, warnings) {
  const icons = await importTs(root, path.join(root, ICONS));
  const weighted = await importTs(root, path.join(root, WEIGHTED));
  if (!Array.isArray(icons.starterIcons) || !Array.isArray(weighted.weightedStarterIcons)) {
    warnings.push(`${ICONS}: no starterIcons or weightedStarterIcons`);
    return;
  }
  const starter = named(icons, icons.starterIcons).map(([exportName, icon]) => ({
    export: exportName,
    name: icon.name,
    ...(icon.viewBox != null ? {viewBox: icon.viewBox} : {}),
    primary: icon.primary.map(shape),
    ...(icon.secondary != null ? {secondary: icon.secondary.map(shape)} : {}),
  }));
  const drawings = named(weighted, weighted.weightedStarterIcons).map(([exportName, icon]) => ({
    export: exportName,
    name: icon.name,
    ...(icon.viewBox != null ? {viewBox: icon.viewBox} : {}),
    layers: icon.layers.map(layer => ({
      from: layer.from,
      to: layer.to,
      paths: layer.paths.map(p => (p.evenOdd === true ? {d: p.d, evenOdd: true} : {d: p.d})),
    })),
  }));
  fs.writeFileSync(path.join(specDir, 'svg-icons.json'), stableStringify({starter, weighted: drawings}));
  console.log(`wrote spec/svg-icons.json (${starter.length} starter icons, ${drawings.length} weighted icons)`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
  const root = astryxRoot(process.argv[2] ?? path.join(repoRoot, '..', 'Astryx'));
  const warnings = [];
  await exportSvgIcons(root, path.join(repoRoot, 'spec'), warnings);
  for (const warning of warnings) console.warn(`warning: ${warning}`);
}
