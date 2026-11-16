// Writes src/catalogue.rs: every spot and species of dovepescare.com, with
// only the fields the scoring engine and the widget faces need.
//
//   node widgets/fishing-spot/tools/catalogue.mjs <path to a dove-pesco checkout>
//
// The province rules (REGOLE.province) and the spot closures (REGOLE.avvisi,
// the winter closure of category D waters) are resolved here, per spot and
// species, with the same logic as regolaLocale() and chiusura() in engine.js.
// Only the date checks are left for the widget.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { loadSite, spotSlugs, flowCell } from './site.mjs';

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), '../src/catalogue.rs');

// English names shown on the Deck. A species missing here stops the script.
const ENGLISH = {
  cavedano: 'Chub', barbo: 'Barbel', barboCanino: 'Brook barbel', lasca: 'Nase',
  vairone: 'Riffle dace', rovella: 'Rovella', gobione: 'Gudgeon', alborella: 'Bleak',
  bosega: 'Thicklip mullet', triotto: 'Italian roach', savetta: 'Savetta', pigo: 'Pigo',
  sanguinerola: 'Minnow', trotaFario: 'Brown trout', trotaIridea: 'Rainbow trout',
  trotaLacustre: 'Lake trout', salmerino: 'Arctic char', temolo: 'Grayling', carpa: 'Carp',
  carpaErbivora: 'Grass carp', carassio: 'Crucian carp', tinca: 'Tench', scardola: 'Rudd',
  breme: 'Bream', luccio: 'Pike', persicoReale: 'Perch', persicoTrota: 'Black bass',
  lucioperca: 'Zander', aspio: 'Asp', siluro: 'Wels catfish', pesceGatto: 'Bullhead',
  anguilla: 'Eel', cheppia: 'Twaite shad', cefalo: 'Mullet', storione: 'Sturgeon',
  spigola: 'Sea bass', orata: 'Gilthead bream', mormora: 'Striped bream',
  pescePelagico: 'Mackerel', paganello: 'Rock goby', calamaro: 'Squid', seppia: 'Cuttlefish',
};

const KIND = {
  torrente: 'Torrent', fiume: 'River', canale: 'Canal', lago: 'Lake',
  bacino: 'Reservoir', cava: 'Quarry', mare: 'Sea',
};
const CATEGORY = { A: 'A', B: 'B', C: 'C', D: 'D', mare: 'Sea' };
const LIGHT = { alba: 'Dawn', crepuscolo: 'Dusk', notte: 'Night', giorno: 'Day', qualsiasi: 'Any' };
const SEASON_BIT = { inverno: 1, primavera: 2, estate: 4, autunno: 8 };

const site = loadSite(process.argv[2]);
const fail = (msg) => { console.error(msg); process.exit(1); };

const str = (s) => JSON.stringify(String(s));
const f64 = (x) => {
  if (typeof x !== 'number' || !isFinite(x)) fail('not a number: ' + x);
  return Number.isInteger(x) ? x.toFixed(1) : String(x);
};
const opt = (x) => (x === null || x === undefined ? 'None' : `Some(${x})`);
const window4 = (w) => `[${w.join(', ')}]`;

// ---------------------------------------------------------------- species
const speciesKeys = Object.keys(site.SPECIE);
const speciesIndex = new Map(speciesKeys.map((k, i) => [k, i]));
const speciesRust = speciesKeys.map((k) => {
  const s = site.SPECIE[k];
  if (!ENGLISH[k]) fail(`no English name for species ${k}`);
  if (!LIGHT[s.luce]) fail(`unknown light ${s.luce} for ${k}`);
  if (!Array.isArray(s.mesi) || s.mesi.length !== 12) fail(`bad months for ${k}`);
  return `    Species {
        key: ${str(k)},
        name: ${str(ENGLISH[k])},
        t_opt: (${f64(s.tOpt[0])}, ${f64(s.tOpt[1])}),
        t_live: (${f64(s.tLive[0])}, ${f64(s.tLive[1])}),
        months: [${s.mesi.map(f64).join(', ')}],
        light: Light::${LIGHT[s.luce]},
        turbid: ${f64(s.torbida || 0)},
        flow: ${f64(s.portata || 0)},
        pressure: ${f64(s.press || 0)},
        closed: &[${(s.divieto || []).map(window4).join(', ')}],
        salmonid_closure: ${!!s.chiusuraSalmonidi},
    },`;
});

// ---------------------------------------------------------------- spots
const P = site.REGOLE.province || {};
const notices = site.REGOLE.avvisi || [];
const toCheck = site.REGOLE.categoriaDaVerificare || [];
const mayOpening = site.REGOLE.apertureDiMaggio || [];

// regolaLocale() without the date check, plus the parts of the species
// record that do not change with the date.
function spotSpecies(spot, key) {
  const sp = site.SPECIE[key];
  const notKeepable = ((P.nonTrattenibili || {})[key] || []).includes(spot.prov);
  let period = null, size = null, limit = null;
  if (!notKeepable) {
    const per = (P.periodi || []).find((p) => p.specie.includes(key) && p.prov.includes(spot.prov));
    if (per) period = per.divieto;
    size = ((P.misure || {})[key] || {})[spot.prov] || null;
    limit = ((P.limiti || {})[key] || {})[spot.prov] || null;
  }
  const rare = !!(site.RARITA[spot.id] && site.RARITA[spot.id].includes(key));
  const alwaysRelease = !!sp.protetta || sp.limiteGiorno === 0 || notKeepable;
  const minSize = notKeepable ? null : (size || sp.misuraMin);
  const dailyLimit = notKeepable ? null : (limit || sp.limiteGiorno);
  const int = (v) => {
    if (v === null || v === undefined) return null;
    if (!Number.isInteger(v) || v < 0) fail(`bad rule value ${v} for ${key} at ${spot.id}`);
    return v;
  };
  return `SpotSpecies { species: ${speciesIndex.get(key)}, rare: ${rare}, always_release: ${alwaysRelease}, `
    + `local_closed: ${period ? `Some(${window4(period)})` : 'None'}, `
    + `min_size_cm: ${opt(int(minSize))}, daily_limit: ${opt(int(dailyLimit))} }`;
}

const slugs = spotSlugs(site.SPOT);
const spotsRust = site.SPOT.map((s, n) => {
  if (!KIND[s.tipo]) fail(`unknown water type ${s.tipo} at ${s.id}`);
  if (!CATEGORY[s.categoria]) fail(`unknown category ${s.categoria} at ${s.id}`);
  const known = (s.specie || []).filter((k) => speciesIndex.has(k));
  const runs = s.tipo === 'fiume' || s.tipo === 'torrente';
  const cell = runs ? flowCell(site, s) : null;
  const top = (s.stagioniTop || []).reduce((m, x) => m | (SEASON_BIT[x] || 0), 0);
  const own = notices.filter((a) => (a.spot || []).includes(s.id))
    .map((a) => `Notice { from: ${a.dal ? `Some(${str(a.dal)})` : 'None'}, to: ${a.al ? `Some(${str(a.al)})` : 'None'} }`);
  let winter = 'None';
  if (s.categoria === 'D' && !s.pagamento && !toCheck.includes(s.id)) {
    winter = mayOpening.includes(s.id) ? 'May' : 'March';
  }
  return `    Spot {
        id: ${str(s.id)},
        slug: ${str(slugs[n])},
        name: ${str(s.nome)},
        kind: WaterKind::${KIND[s.tipo]},
        category: Category::${CATEGORY[s.categoria]},
        lat: ${str(s.lat)},
        lon: ${str(s.lon)},
        flow_cell: ${cell ? `Some((${str(cell[0])}, ${str(cell[1])}))` : 'None'},
        top_seasons: ${top},
        species: &[${known.map((k) => '\n            ' + spotSpecies(s, k) + ',').join('')}
        ],
        notices: &[${own.join(', ')}],
        winter_closure: WinterClosure::${winter},
    },`;
});

let source = 'unknown';
try {
  source = execFileSync('git', ['-C', site.root, 'log', '-1', '--format=%h %cs'], { encoding: 'utf8' }).trim();
} catch { /* not a git checkout */ }

const out = `// @generated by tools/catalogue.mjs from dove-pesco ${source}.
// Do not edit. To regenerate, from the repo root:
//
//   node widgets/fishing-spot/tools/catalogue.mjs <path to a dove-pesco checkout>
//
// Sources: ${site.files.filter((f) => f !== 'assets/js/engine.js').join(', ')}.
// ${site.SPOT.length} spots, ${speciesKeys.length} species. Public data of dovepescare.com.

use crate::spot::{
    Category, Light, Notice, Species, Spot, SpotSpecies, WaterKind, WinterClosure,
};

pub static SPECIES: &[Species] = &[
${speciesRust.join('\n')}
];

pub static SPOTS: &[Spot] = &[
${spotsRust.join('\n')}
];
`;

fs.writeFileSync(OUT, out);
console.log(`${path.relative(process.cwd(), OUT)}: ${site.SPOT.length} spots, ${speciesKeys.length} species`);
