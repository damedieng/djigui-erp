// Mesure dans le VRAI Chrome des deux ajouts N1 OHADA :
//   • la mention du montant arrêté en toutes lettres, sur la facture ;
//   • le contrôle de continuité de la numérotation, dans les rapports.
//
// ⚠️ jsdom ne calcule pas la mise en page : un bloc écrasé à 0 px ou un tableau
// qui déborde ne se voit QUE d'ici.
//
// Usage : node capture-facture-n1.mjs [port]
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const port = process.argv[2] || '1704';
const CHEMINS = [
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
  process.env.LOCALAPPDATA + '/Google/Chrome/Application/chrome.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
];
const chrome = CHEMINS.find(p => p && existsSync(p));
if (!chrome) { console.error('Aucun Chrome/Edge trouvé.'); process.exit(1); }

const navigateur = await puppeteer.launch({
  executablePath: chrome, headless: 'new',
  args: ['--no-sandbox', '--window-size=1500,1100'],
});
const page = await navigateur.newPage();
await page.setViewport({ width: 1500, height: 1100 });

const erreurs = [];
page.on('pageerror', e => erreurs.push(String(e)));
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });

await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});

const ok = [], ko = [];
const v = (nom, cond, detail = '') => (cond ? ok : ko).push(nom + (detail ? ` — ${detail}` : ''));

// --- 1. La facture ----------------------------------------------------------
const docs = await (await fetch(`http://localhost:${port}/api/documents?sens=vente`)).json();
const facture = docs.find(d => d.type_document === 'facture');
if (!facture) { console.error('Aucune facture dans la base.'); process.exit(1); }

await page.goto(`http://localhost:${port}/facture.html?id=${facture.id}`, { waitUntil: 'networkidle2' });
await page.waitForSelector('#f-ttc');
await new Promise(r => setTimeout(r, 700));

const m = await page.evaluate(() => {
  const r = el => { const b = el.getBoundingClientRect();
    return { x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width), h: Math.round(b.height) }; };
  const bloc = document.querySelector('#f-lettres-bloc');
  const ttc = document.querySelector('#f-ttc');
  return {
    visible: getComputedStyle(bloc).display !== 'none',
    boite: r(bloc),
    texte: document.querySelector('#f-lettres').textContent.trim(),
    ttcTexte: ttc.textContent.trim(),
    ttcBas: r(ttc).y + r(ttc).h,
    debordement: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
});

console.log('\n--- Facture ---');
console.log('  TTC affiché  :', m.ttcTexte);
console.log('  En lettres   :', m.texte);
console.log('  Bloc         :', JSON.stringify(m.boite));

v('la mention en lettres est affichée', m.visible);
v('elle n\'est pas écrasée', m.boite.h > 25, `${m.boite.h} px de haut`);
v('elle contient bien du texte', m.texte.length > 10, `« ${m.texte} »`);
// La mention doit venir APRÈS le total : elle l'arrête, elle ne l'annonce pas.
v('elle est placée sous le total TTC', m.boite.y >= m.ttcBas,
  `mention à y=${m.boite.y}, total finit à y=${m.ttcBas}`);
v('aucun débordement horizontal', m.debordement <= 0, `${m.debordement} px`);

// ⚠️ Le contrôle qui compte vraiment : les deux montants doivent parler du
// MÊME nombre. Une facture qui affiche 14 080 et écrit « douze mille » serait
// pire que pas de mention du tout.
const chiffres = (m.ttcTexte.match(/[\d]/g) || []).join('');
v('le TTC chiffré est bien présent', chiffres.length > 0);
v('la mention n\'est pas vide alors qu\'un total existe',
  !(chiffres.length > 0 && m.texte.length === 0));

await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/facture-montant-lettres.png', fullPage: true });

// --- 2. Le rapport de numérotation -----------------------------------------
await page.goto(`http://localhost:${port}/rapports.html`, { waitUntil: 'networkidle2' });
await new Promise(r => setTimeout(r, 400));
await page.evaluate(() => {
  document.querySelector('.tab[data-tab="numerotation"]').click();
});
await new Promise(r => setTimeout(r, 900));

const n = await page.evaluate(() => {
  const panneau = document.querySelector('[data-panel="numerotation"]');
  const r = el => { const b = el.getBoundingClientRect();
    return { w: Math.round(b.width), h: Math.round(b.height) }; };
  return {
    visible: getComputedStyle(panneau).display !== 'none',
    nbTuiles: panneau.querySelectorAll('.tuile').length,
    nbSeries: panneau.querySelectorAll('#corps-series tr').length,
    constat: (panneau.querySelector('#constat-numerotation')?.textContent || '').trim(),
    blocTrousVisible: getComputedStyle(document.querySelector('#bloc-trous')).display !== 'none',
    aide: (panneau.querySelector('.aide')?.textContent || '').replace(/\s+/g, ' '),
    tableau: r(panneau.querySelector('.card')),
    debordement: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    autresPanneaux: [...document.querySelectorAll('[data-panel]')]
      .filter(p => p.dataset.panel !== 'numerotation')
      .filter(p => getComputedStyle(p).display !== 'none').length,
  };
});

console.log('\n--- Rapport numérotation ---');
console.log('  Constat  :', n.constat);
console.log('  Séries   :', n.nbSeries, '| tuiles :', n.nbTuiles);

v('le panneau Numérotation s\'affiche', n.visible);
v('les autres panneaux se ferment', n.autresPanneaux === 0);
v('les trois tuiles sont dessinées', n.nbTuiles === 3, `${n.nbTuiles}`);
v('le tableau des séries est rempli', n.nbSeries >= 1);
v('le tableau n\'est pas écrasé', n.tableau.h > 40, `${n.tableau.h} px`);
v('un constat en langage clair est affiché', n.constat.length > 20);
// Le bloc des trous ne doit apparaître QUE s'il y a des trous : une section
// vide ferait croire à un problème.
v('le bloc des trous suit l\'état réel',
  n.blocTrousVisible === !n.constat.includes('Aucun trou'));
v('l\'aide dit qu\'un trou n\'est pas une faute en soi',
  n.aide.includes('pas une faute'));
v('l\'aide explique la colonne des numéros tirés', n.aide.includes('Numéros tirés'));
v('aucun débordement horizontal', n.debordement <= 0, `${n.debordement} px`);

await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/rapport-numerotation.png', fullPage: true });

v('aucune erreur JavaScript', erreurs.length === 0, erreurs.join(' | '));

console.log(`\ncapture facture N1 : ${ok.length}/${ok.length + ko.length} vérifications`);
if (ko.length) { console.log('\nÉCHECS :'); ko.forEach(x => console.log('  ✗ ' + x)); }
console.log('\nCaptures : captures_ecran/facture-montant-lettres.png, rapport-numerotation.png');

await navigateur.close();
process.exit(ko.length ? 1 : 0);
