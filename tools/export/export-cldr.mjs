// Exports the CLDR data the date components need that ICU4X does not carry:
// upstream's generated weekday-header and month-capital tables, and the
// calendar and week-field display names that upstream reads from
// `Intl.DisplayNames` at run time, read here from Node's `Intl`.
//
//   node tools/export/export-cldr.mjs [path/to/Astryx]
//
// Writes spec/cldr.json. export-all.mjs runs it too.

import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {astryxRoot} from './lib/astryx.mjs';
import {stableStringify} from './lib/values.mjs';

const CALENDARS = [
  'gregory', 'iso8601', 'buddhist', 'roc', 'japanese', 'persian', 'indian',
  'hebrew', 'chinese', 'dangi', 'coptic', 'ethiopic', 'ethioaa',
  'islamic-civil', 'islamic-tbla', 'islamic-umalqura',
];

// Languages whose people often use a calendar other than the Gregorian one,
// beyond the shipped catalogs, so a field names its calendar in them too.
const EXTRA_LOCALES = [
  'am', 'ar', 'bn', 'fa', 'gu', 'he', 'hi', 'id', 'ja', 'km', 'kn', 'ko', 'lo',
  'ml', 'mr', 'ms', 'my', 'ne', 'pa', 'ps', 'sw', 'ta', 'te', 'th', 'ti', 'ur',
  'zh', 'zh-Hant',
];

async function importTs(file) {
  return import(pathToFileURL(file).href);
}

export async function exportCldr(root, specDir, warnings) {
  const core = path.join(root, 'packages', 'core', 'src');
  const {standaloneMonthCapitalLanguages} = await importTs(
    path.join(core, 'utils', 'standaloneMonthCapitals.generated.ts'),
  );
  const {weekdayHeaderNamesByLocale} = await importTs(
    path.join(core, 'Calendar', 'weekdayHeaderNames.generated.ts'),
  );

  const shipped = fs
    .readdirSync(path.join(root, 'packages', 'core', 'locales'))
    .filter(f => f.endsWith('.json') && f !== 'pseudo.json')
    .map(f => f.slice(0, -'.json'.length));
  const tags = new Set(['en']);
  for (const tag of [...shipped, ...Object.keys(weekdayHeaderNamesByLocale), ...EXTRA_LOCALES]) {
    tags.add(tag);
    tags.add(new Intl.Locale(tag).language);
  }

  const calendarNames = {};
  const weekOfYear = {};
  for (const tag of [...tags].sort()) {
    const calendars = new Intl.DisplayNames(tag, {type: 'calendar'});
    const resolved = new Intl.Locale(calendars.resolvedOptions().locale).language;
    if (resolved !== new Intl.Locale(tag).language) {
      warnings.push(`Intl has no display names for ${tag}`);
      continue;
    }
    calendarNames[tag] = Object.fromEntries(CALENDARS.map(id => [id, calendars.of(id) ?? id]));
    weekOfYear[tag] = new Intl.DisplayNames(tag, {type: 'dateTimeField'}).of('weekOfYear') ?? 'week';
  }

  const data = {
    calendarNames,
    intl: `node ${process.version}, ICU ${process.versions.icu}, CLDR ${process.versions.cldr}`,
    standaloneMonthCapitalLanguages: [...standaloneMonthCapitalLanguages].sort(),
    weekdayHeaderNames: weekdayHeaderNamesByLocale,
    weekOfYear,
  };
  fs.writeFileSync(path.join(specDir, 'cldr.json'), stableStringify(data));
  console.log(`wrote cldr.json (${Object.keys(calendarNames).length} locales)`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
  const arg = process.argv.slice(2).find(a => !a.startsWith('--'));
  const root = astryxRoot(arg ?? path.join(repoRoot, '..', 'Astryx'));
  const warnings = [];
  await exportCldr(root, path.join(repoRoot, 'spec'), warnings);
  for (const warning of warnings) console.warn(`warning: ${warning}`);
}
