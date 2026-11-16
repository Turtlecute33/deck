// Writes the parity fixtures in fixtures/: for a few spots of every water
// type, the Open-Meteo replies the widget reads, and what the real
// dovepescare.com engine (assets/js/engine.js) computes from them.
//
//   node widgets/fishing-spot/tools/parity.mjs <path to a dove-pesco checkout> [--refresh]
//
// Without --refresh it keeps the saved replies and only computes the
// expected values again, so a change to engine.js can be checked offline.
// With --refresh it asks Open-Meteo for new replies first.

process.env.TZ = 'Europe/Rome';   // the site's visitors are in Italy

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadSite, flowCell, DAILY, PAST_DAYS, FORECAST_DAYS } from './site.mjs';

const DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), '../fixtures');

// One spot per kind of water and rule, none of them special.
export const SPOTS = [
  'mo-panaro-vignola',        // river with discharge, province rules (MO)
  'ra-senio-casola',          // torrent with discharge, rare species
  'pc-nure-bettola',          // trout torrent: winter closure
  'mo-casse-secchia',         // lake
  're-cavo-fiuma',            // canal
  'rn-laghi-santarcangelo',   // quarry lakes, Rimini cyprinid rule
  'rn-portocanale-rimini',    // sea: surface temperature and waves
  'pr-taro-san-secondo',      // temporary ban on the spot
];

const MODS = {
  'Portata in piena': 'flood',
  'Magra severa': 'low-water',
  'Pioggia forte prevista': 'heavy-rain',
  'Temporali': 'thunderstorms',
  'Mare molto mosso': 'rough-sea',
  'Vento forte': 'strong-wind',
  'Mare mosso: spigole attive': 'choppy-sea',
  'Acqua fresca in giornata torrida': 'cool-water',
  'Stagione consigliata per questo spot': 'top-season',
  'Nessuna specie trattenibile oggi': 'nothing-keepable',
};
const WINDOWS = { Alba: 'Dawn', Tramonto: 'Dusk', Notte: 'Night', 'Tutto il giorno': 'AllDay' };
const MOON = {
  'Luna nuova': 'New moon', 'Luna crescente': 'Waxing crescent', 'Primo quarto': 'First quarter',
  'Gibbosa crescente': 'Waxing gibbous', 'Luna piena': 'Full moon',
  'Gibbosa calante': 'Waning gibbous', 'Ultimo quarto': 'Last quarter',
  'Luna calante': 'Waning crescent',
};
const SEASON = { inverno: 'Winter', primavera: 'Spring', estate: 'Summer', autunno: 'Autumn' };

const args = process.argv.slice(2);
const refresh = args.includes('--refresh');
const site = loadSite(args.find((a) => !a.startsWith('--')));
const { ENGINE, REGOLE } = site;

// The same URLs as src/url.rs.
const urls = (s) => ({
  forecast: `https://api.open-meteo.com/v1/forecast?latitude=${s.lat}&longitude=${s.lon}`
    + `&daily=${DAILY}&current=temperature_2m&past_days=${PAST_DAYS}&forecast_days=${FORECAST_DAYS}`
    + '&timezone=Europe%2FRome',
  flow: ['fiume', 'torrente'].includes(s.tipo)
    ? `https://flood-api.open-meteo.com/v1/flood?latitude=${flowCell(site, s)[0]}`
      + `&longitude=${flowCell(site, s)[1]}&daily=river_discharge&past_days=45&forecast_days=7`
    : null,
  sea: s.tipo === 'mare'
    ? `https://marine-api.open-meteo.com/v1/marine?latitude=${s.lat}&longitude=${s.lon}`
      + `&hourly=sea_surface_temperature&daily=wave_height_max&past_days=${PAST_DAYS}`
      + `&forecast_days=${FORECAST_DAYS}&timezone=Europe%2FRome&cell_selection=sea`
    : null,
});

async function reply(file, url) {
  if (!url) return null;
  const p = path.join(DIR, file);
  if (refresh || !fs.existsSync(p)) {
    const r = await fetch(url);
    if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
    fs.writeFileSync(p, await r.text());
  }
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

// api.js, mare(): the day's mean of the hourly sea temperature, and the
// day's highest wave, put into the forecast days.
function mergeSea(meteo, r) {
  const sum = {}, count = {}, tsm = {}, wave = {};
  ((r.hourly && r.hourly.time) || []).forEach((t, i) => {
    const v = r.hourly.sea_surface_temperature[i];
    if (v === null || v === undefined) return;
    const d = t.slice(0, 10);
    sum[d] = (sum[d] || 0) + v; count[d] = (count[d] || 0) + 1;
  });
  Object.keys(sum).forEach((d) => { tsm[d] = Math.round(sum[d] / count[d] * 10) / 10; });
  ((r.daily && r.daily.time) || []).forEach((t, i) => { wave[t] = r.daily.wave_height_max[i]; });
  const o = (v) => (v === undefined ? null : v);
  meteo.daily.sea_surface_temperature = meteo.daily.time.map((t) => o(tsm[t]));
  meteo.daily.wave_height_max = meteo.daily.time.map((t) => o(wave[t]));
}

const isoLocal = (d) => [d.getFullYear(), d.getMonth() + 1, d.getDate()]
  .map((n, i) => String(n).padStart(i ? 2 : 4, '0')).join('-');

function modKey(spot, date, m, chiuso) {
  if (chiuso && m.t === chiuso.t) {
    if (!chiuso.stagionale) return 'closed-notice';
    const may = (REGOLE.apertureDiMaggio || []).includes(spot.id);
    return 'closed-until-' + isoLocal(ENGINE.stagioneChiusa(new Date(date + 'T12:00:00'), may));
  }
  if (!MODS[m.t]) throw new Error(`no key for modifier "${m.t}"`);
  return MODS[m.t];
}

// The same replies moved to other dates, to check the seasons, the closed
// seasons, the trout opening and summer time against the source.
const SHIFTS = { 'mo-panaro-vignola': [138, 210], 'pc-nure-bettola': [138, 320],
  'mo-casse-secchia': [138, 210], 'rn-portocanale-rimini': [138, 210] };

const shiftDate = (s, n) => {
  const d = new Date(s.slice(0, 10) + 'T00:00:00Z');
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10) + s.slice(10);
};
function shiftReply(r, n) {
  if (!r) return null;
  const out = JSON.parse(JSON.stringify(r));
  for (const block of ['daily', 'hourly', 'current']) {
    const b = out[block];
    if (!b) continue;
    for (const k of ['time', 'sunrise', 'sunset']) {
      if (Array.isArray(b[k])) b[k] = b[k].map((v) => (typeof v === 'string' ? shiftDate(v, n) : v));
      else if (typeof b[k] === 'string') b[k] = shiftDate(b[k], n);
    }
  }
  return out;
}

function expect(spot, name, meteo, flow, sea) {
  if (sea) mergeSea(meteo, sea);
  const days = meteo.daily.time.map((date) => {
    const v = ENGINE.valuta(spot, meteo, flow, date);
    if (!v) throw new Error(`${name} ${date}: no evaluation`);
    return {
      date,
      score: v.punteggio,
      water: v.acqua.temp,
      measured: v.acqua.misurata,
      wave: v.acqua.onda,
      turbidity: v.acqua.torbidita,
      flow: v.acqua.flow,
      ratio: v.acqua.flowRatio,
      rain72: v.meteo.p72,
      pressureDelta: v.meteo.dPress,
      moon: v.luna.illum,
      moonName: MOON[v.luna.nome],
      season: SEASON[v.stagione],
      species: v.specie.map((s) => [s.id, s.score, s.soloRilascio]),
      mods: v.mod.map((m) => modKey(spot, date, m, v.chiuso)),
      windows: v.finestre.map((f) => [
        WINDOWS[f.q],
        f.o === 'cielo coperto' ? 'overcast' : f.o.replaceAll('–', '-'),
      ]),
    };
  });
  fs.writeFileSync(path.join(DIR, `${name}.expected.json`),
    JSON.stringify({ spot: spot.id, engine: 'assets/js/engine.js', days }) + '\n');
  console.log(`${name}: ${days.length} days, scores ${days.map((d) => d.score).join(' ')}`);
}

const save = (name, kind, r) => {
  if (r) fs.writeFileSync(path.join(DIR, `${name}.${kind}.json`), JSON.stringify(r));
};

fs.mkdirSync(DIR, { recursive: true });
for (const id of SPOTS) {
  const spot = site.SPOT.find((s) => s.id === id);
  if (!spot) throw new Error(`no spot ${id}`);
  const u = urls(spot);
  const meteo = await reply(`${id}.forecast.json`, u.forecast);
  const flow = await reply(`${id}.flow.json`, u.flow);
  const sea = await reply(`${id}.sea.json`, u.sea);
  expect(spot, id, structuredClone(meteo), flow, sea);
  for (const n of SHIFTS[id] || []) {
    const name = `${id}+${n}`;
    const [m, f, s] = [shiftReply(meteo, n), shiftReply(flow, n), shiftReply(sea, n)];
    save(name, 'forecast', m); save(name, 'flow', f); save(name, 'sea', s);
    expect(spot, name, structuredClone(m), f, s);
  }
}
