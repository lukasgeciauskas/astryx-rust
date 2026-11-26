// Loads modules from an upstream Astryx checkout without writing into it.
//
// The checkout must have `pnpm install` run and `@astryxdesign/core` built
// (`pnpm -F @astryxdesign/core build`): theme tokens are StyleX variables,
// which only the compiled dist can evaluate.

import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

/** Resolves and checks the Astryx checkout path given on the command line. */
export function astryxRoot(arg) {
  const root = path.resolve(arg ?? '../Astryx');
  const core = path.join(root, 'packages', 'core');
  if (!fs.existsSync(path.join(core, 'package.json'))) {
    throw new Error(`${root} is not an Astryx checkout (no packages/core)`);
  }
  if (!fs.existsSync(path.join(core, 'dist', 'theme', 'tokens.js'))) {
    throw new Error(
      `${core}/dist is missing: run \`pnpm -F @astryxdesign/core build\` in ${root}`,
    );
  }
  return root;
}

/** Imports a file inside the checkout; its own imports resolve from there. */
export function importFile(root, relative) {
  return import(pathToFileURL(path.join(root, relative)).href);
}

let vite;
/**
 * Imports a TypeScript source file through Vite, as upstream scripts do. `plugins` are
 * Vite plugins for this import (an exporter may expose an internal binding with one).
 */
export async function importTs(root, file, plugins = []) {
  if (!vite) {
    const pkg = path.join(root, 'node_modules', 'vite', 'package.json');
    const entry = JSON.parse(fs.readFileSync(pkg, 'utf8')).exports['.'];
    const main = typeof entry === 'string' ? entry : (entry.import ?? entry.default);
    vite = await import(pathToFileURL(path.join(path.dirname(pkg), main)).href);
  }
  const {module} = await vite.runnerImport(file, {
    root,
    configFile: false,
    logLevel: 'error',
    plugins,
  });
  return module;
}

/** The upstream commit of the checkout, for the snapshot header. */
export function upstreamCommit(root) {
  const head = fs.readFileSync(path.join(root, '.git', 'HEAD'), 'utf8').trim();
  if (!head.startsWith('ref: ')) return head;
  const ref = head.slice(5);
  const loose = path.join(root, '.git', ref);
  if (fs.existsSync(loose)) return fs.readFileSync(loose, 'utf8').trim();
  const packed = fs.readFileSync(path.join(root, '.git', 'packed-refs'), 'utf8');
  const line = packed.split('\n').find(l => l.endsWith(` ${ref}`));
  return line ? line.split(' ')[0] : 'unknown';
}
