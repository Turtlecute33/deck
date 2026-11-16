// Loads the dovepescare.com data and scoring engine from a checkout of the
// site (github.com/Turtlecute33/dove-pesco), the same way the site's own
// tools/genera-pagine.py does: the JS files run together in one node VM.

import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

const FILES = [
  'data-species.js', 'data-spots-emilia.js', 'data-spots-romagna.js',
  'data-spots-extra.js', 'data-spots-centro.js', 'data-index.js',
  'data-rules.js', 'engine.js',
];

export function loadSite(root) {
  if (!root) {
    console.error('usage: node <script> <path to a dove-pesco checkout>');
    process.exit(2);
  }
  const dir = path.join(root, 'assets/js');
  const src = FILES.map((f) => fs.readFileSync(path.join(dir, f), 'utf8')).join('\n');
  const ctx = vm.createContext({});
  const site = vm.runInContext(
    src + '\n;({ SPOT, SPECIE, PROVINCE, RARITA, REGOLE, CELLE_PORTATA, ENGINE })',
    ctx,
  );
  site.files = FILES.map((f) => 'assets/js/' + f);
  site.root = root;
  return site;
}

// The page address of each spot, as tools/genera-pagine.py assigns it:
// slug(nome), or slug(nome-comune) when taken, or the id, then -2, -3...
export function slug(text) {
  return String(text)
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

export function spotSlugs(spots) {
  const taken = new Set();
  const unique = (s, fallback) => {
    if (s && !taken.has(s)) { taken.add(s); return s; }
    let out = fallback;
    for (let n = 2; taken.has(out); n++) out = fallback + '-' + n;
    taken.add(out);
    return out;
  };
  return spots.map((s) => unique(slug(s.nome), slug(s.nome + '-' + s.comune) || s.id));
}

// The request parameters the site sends to Open-Meteo (assets/js/api.js).
export const PAST_DAYS = 14;
export const FORECAST_DAYS = 7;
export const DAILY = [
  'weather_code', 'temperature_2m_max', 'temperature_2m_min', 'temperature_2m_mean',
  'precipitation_sum', 'wind_speed_10m_max', 'wind_direction_10m_dominant',
  'pressure_msl_mean', 'cloud_cover_mean', 'sunrise', 'sunset',
].join(',');

export function flowCell(site, spot) {
  return site.CELLE_PORTATA[spot.id] || [spot.lat, spot.lon];
}
