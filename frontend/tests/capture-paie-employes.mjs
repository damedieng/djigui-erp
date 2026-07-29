// Mesure de l'écran « Salariés » dans le VRAI Chrome (mig 0045).
//
// ⚠️ jsdom ne calcule pas la mise en page : une grille de cartes, quatre modales
// et un traitement par lot, c'est exactement le terrain des cartes écrasées et
// des panneaux qui refusent de se fermer.
//
// Usage : node capture-paie-employes.mjs [port]
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const port = process.argv[2] || '1704';
const CH = ['C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
  process.env.LOCALAPPDATA + '/Google/Chrome/Application/chrome.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe'];
const chrome = CH.find(p => p && existsSync(p));
if (!chrome) { console.error('Aucun Chrome/Edge trouvé.'); process.exit(1); }

const nav = await puppeteer.launch({ executablePath: chrome, headless: 'new',
  args: ['--no-sandbox', '--window-size=1600,1100'] });
const page = await nav.newPage();
await page.setViewport({ width: 1600, height: 1100 });
const erreurs = [];
page.on('pageerror', e => erreurs.push(String(e)));
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });
await page.evaluateOnNewDocument(() => sessionStorage.setItem('djigui_user',
  JSON.stringify({ id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin' })));

const ok = [], ko = [];
const v = (n, c, d = '') => (c ? ok : ko).push(n + (d ? ` — ${d}` : ''));

await page.goto(`http://localhost:${port}/paie-employes.html`, { waitUntil: 'networkidle2' });
await page.waitForSelector('.proj-carte');
await new Promise(r => setTimeout(r, 600));

const m = await page.evaluate(() => {
  const cartes = [...document.querySelectorAll('.proj-carte')];
  return {
    nbCartes: cartes.length,
    nbTuiles: document.querySelectorAll('.pe-bloc').length,
    hauteurs: cartes.map(c => Math.round(c.getBoundingClientRect().height)),
    largeurs: cartes.map(c => Math.round(c.getBoundingClientRect().width)),
    texte: document.querySelector('#grille').textContent.replace(/\s+/g, ' '),
    barreLotVisible: getComputedStyle(document.querySelector('#barre-lot')).display !== 'none',
    deborde: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    tuiles: document.querySelector('#tuiles').textContent.replace(/\s+/g, ' '),
  };
});
console.log('\n--- Liste ---');
console.log('  Cartes :', m.nbCartes, '| hauteurs :', m.hauteurs.join(', '), 'px');
console.log('  Tuiles :', m.tuiles.slice(0, 130));

v('les deux salariés ont leur carte', m.nbCartes === 2, `${m.nbCartes}`);
v('les 4 tuiles de synthèse sont dessinées', m.nbTuiles === 4, `${m.nbTuiles}`);
v('aucune carte écrasée', m.hauteurs.every(h => h > 150), m.hauteurs.join(','));
v('les cartes ont une largeur réelle', m.largeurs.every(w => w > 200), m.largeurs.join(','));
v('la rémunération de base est affichée', m.texte.includes('525 000'));
v('le nombre de parts est affiché', /2[.,]5 part/.test(m.texte));
v('l’ancienneté est affichée', m.texte.includes('mois'));
// La carte ne montre QUE la première anomalie plus un compteur : afficher les
// trois d'affilée noierait la carte et on ne verrait plus le salaire.
// Le détail complet est sur la fiche — vérifié plus bas.
v('une fiche incomplète est signalée sur la carte',
  m.texte.includes('CDD') || m.texte.includes('IPRES') || m.texte.includes('compte'));
v('la carte annonce qu’il y a d’autres anomalies', /\+\d+ autre/.test(m.texte));
v('un cadre est marqué', m.texte.includes('cadre'));
// ⚠️ Piège n°2 : `.bulk-bar` doit être RÉELLEMENT masquée quand rien n'est coché.
v('la barre de lot est masquée au départ', !m.barreLotVisible);
v('aucun débordement horizontal', m.deborde <= 0, `${m.deborde} px`);

// --- Sélection par lot ------------------------------------------------------
await page.click('.proj-coche');
await new Promise(r => setTimeout(r, 250));
const lot = await page.evaluate(() => ({
  visible: getComputedStyle(document.querySelector('#barre-lot')).display !== 'none',
  compte: document.querySelector('#lot-compte').textContent.trim(),
  carteMarquee: document.querySelectorAll('.proj-carte.choisie').length,
}));
v('cocher une carte ouvre la barre de lot', lot.visible);
v('le compteur suit', lot.compte.startsWith('1'), lot.compte);
// ⚠️ La carte NE doit PAS être re-rendue : un nœud détaché ne remonterait plus
// les clics suivants (piège rencontré sur l'écran Marchés).
v('la carte cochée est marquée', lot.carteMarquee === 1);
await page.click('.proj-coche');
await new Promise(r => setTimeout(r, 250));
v('décocher referme la barre', !(await page.evaluate(() =>
  getComputedStyle(document.querySelector('#barre-lot')).display !== 'none')));

await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/paie-employes.png', fullPage: true });

// --- La fiche ---------------------------------------------------------------
await page.click('[data-fiche]');
await new Promise(r => setTimeout(r, 700));
const fiche = await page.evaluate(() => {
  const mo = document.querySelector('#modal-fiche');
  const boite = mo.querySelector('.modal');
  return {
    ouverte: getComputedStyle(mo).display !== 'none',
    hauteur: Math.round(boite.getBoundingClientRect().height),
    texte: mo.textContent.replace(/\s+/g, ' '),
    lignesContrat: mo.querySelectorAll('.ctr-tbl tbody tr').length,
    deborde: boite.scrollWidth - boite.clientWidth,
  };
});
console.log('\n--- Fiche ---');
console.log('  ', fiche.texte.slice(0, 160));
v('la fiche s’ouvre', fiche.ouverte);
v('la fiche n’est pas écrasée', fiche.hauteur > 250, `${fiche.hauteur} px`);
v('le contrat apparaît dans l’historique', fiche.lignesContrat >= 1);
v('le contrat en cours est marqué', fiche.texte.includes('en cours'));
v('les parts sont rappelées sur la fiche', fiche.texte.includes('Parts'));
v('la fiche ne déborde pas', fiche.deborde <= 0);
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/paie-fiche-salarie.png' });

// --- La modale contrat ------------------------------------------------------
await page.click('#btn-nouveau-contrat');
await new Promise(r => setTimeout(r, 500));
const ctr = await page.evaluate(() => {
  const mo = document.querySelector('#modal-ctr');
  return {
    ouverte: getComputedStyle(mo).display !== 'none',
    ficheFermee: getComputedStyle(document.querySelector('#modal-fiche')).display === 'none',
    champsPlats: [...mo.querySelectorAll('input, select')]
      .filter(c => getComputedStyle(c).display !== 'none' && c.type !== 'checkbox')
      .filter(c => c.getBoundingClientRect().height < 20).length,
    avertissement: mo.textContent.includes('se fermera automatiquement la veille'),
    nbAvantages: mo.querySelectorAll('[data-av]').length,
  };
});
v('la modale contrat s’ouvre', ctr.ouverte);
v('la fiche se ferme derrière', ctr.ficheFermee);
v('aucun champ écrasé', ctr.champsPlats === 0, `${ctr.champsPlats}`);
// C'est LA chose à dire : personne ne doit croire qu'il perd son ancien contrat.
v('elle prévient que le contrat en cours se fermera', ctr.avertissement);
v('les avantages viennent des paramètres de paie', ctr.nbAvantages >= 1, `${ctr.nbAvantages}`);
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/paie-nouveau-contrat.png' });

// --- L'aide -----------------------------------------------------------------
await page.evaluate(() => document.querySelector('[data-fermer="modal-ctr"]').click());
const aide = await page.evaluate(() =>
  document.querySelector('.aide').textContent.replace(/\s+/g, ' '));
v('l’aide explique le contrat unique', aide.includes('Un seul contrat est actif'));
v('l’aide dit de ne pas supprimer un salarié', aide.includes('ne le supprimez pas'));
v('l’aide dit que les alertes ne bloquent pas',
  aide.includes('empêchent jamais d'));

v('aucune erreur JavaScript', erreurs.length === 0, erreurs.join(' | '));

console.log(`\ncapture paie-employés : ${ok.length}/${ok.length + ko.length} vérifications`);
if (ko.length) { console.log('\nÉCHECS :'); ko.forEach(x => console.log('  ✗ ' + x)); }
await nav.close();
process.exit(ko.length ? 1 : 0);
