// Writes spec/manifest.json: every upstream component and hook doc
// (`*.doc.mjs` in core, lab, charts and richtext), the full `docs` export
// and the agent-oriented `docsDense` export where one exists. The `astryx`
// CLI embeds it, so `astryx component <Name>` shows upstream's props and
// guidance next to the Rust port's API.

import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {stableStringify} from './lib/values.mjs';

const PACKAGES = ['core', 'lab', 'charts', 'richtext'];

function docFiles(dir, out = []) {
  for (const entry of fs.readdirSync(dir, {withFileTypes: true})) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'node_modules' && entry.name !== '__tests__') docFiles(full, out);
    } else if (entry.name.endsWith('.doc.mjs')) {
      out.push(full);
    }
  }
  return out;
}

/** A doc as plain data: functions and React elements dropped. */
function plain(value) {
  return JSON.parse(
    JSON.stringify(value, (key, v) => {
      if (typeof v === 'function' || typeof v === 'symbol') return undefined;
      if (v && typeof v === 'object' && '$$typeof' in v) return '[element]';
      return v;
    }),
  );
}

export async function exportManifest(root, specDir, warnings) {
  const entries = [];
  for (const pkg of PACKAGES) {
    const src = path.join(root, 'packages', pkg, 'src');
    if (!fs.existsSync(src)) continue;
    for (const file of docFiles(src).sort()) {
      let module;
      try {
        module = await import(pathToFileURL(file).href);
      } catch (error) {
        warnings.push(`manifest: ${path.relative(root, file)}: ${error.message}; left out`);
        continue;
      }
      const docs = [module.docs].flat().filter(Boolean);
      const dense = [module.docsDense].flat().filter(Boolean);
      // A dense doc is usually a nameless overlay on the file's one doc.
      const denseFor = doc =>
        dense.find(d => d.name === doc.name) ??
        (docs.length === 1 && dense.length === 1 ? dense[0] : null);
      for (const doc of docs) {
        entries.push({
          package: pkg,
          source: path.relative(root, file).split(path.sep).join('/'),
          doc: plain(doc),
          dense: plain(denseFor(doc)),
        });
      }
    }
  }
  entries.sort((a, b) =>
    `${a.package}/${a.doc.name}`.localeCompare(`${b.package}/${b.doc.name}`),
  );
  fs.writeFileSync(path.join(specDir, 'manifest.json'), stableStringify({entries}));
  console.log(`wrote spec/manifest.json (${entries.length} docs)`);
}
