// Mesure de l'écran Sauvegarde dans le VRAI Chrome.
//
// ⚠️ jsdom ne calcule pas la mise en page : c'est ici qu'on vérifie ce que
// l'utilisateur a vu sur sa capture — une case à cocher étirée sur toute la
// largeur du formulaire, son dessin centré au milieu et le texte rejeté à
// droite. `.field input { width: 100% }` s'appliquait aux cases à cocher.
//
// Usage : node capture-sauvegarde.mjs [port]
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
  args: ['--no-sandbox', '--window-size=1500,1000'],
});
const page = await navigateur.newPage();
await page.setViewport({ width: 1500, height: 1000 });

const erreurs = [];
page.on('pageerror', e => erreurs.push(String(e)));
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });

await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});

await page.goto(`http://localhost:${port}/sauvegarde.html`, { waitUntil: 'networkidle2' });
await page.waitForSelector('#tuiles');
await new Promise(r => setTimeout(r, 600));

const ok = [], ko = [];
const v = (nom, cond, detail = '') => (cond ? ok : ko).push(nom + (detail ? ` — ${detail}` : ''));

// --- Onglet Réglages : le défaut signalé par l'utilisateur ------------------
await page.click('.tab[data-vue="reglages"]');
await new Promise(r => setTimeout(r, 300));

const mesures = await page.evaluate(() => {
  const r = el => { const b = el.getBoundingClientRect();
    return { x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width), h: Math.round(b.height) }; };
  const cases = [...document.querySelectorAll('#p-activee, #p-fermeture, #p-serveur')]
    .map(c => {
      const lbl = c.closest('label');
      const texte = lbl.querySelector('span');
      return { id: c.id, boite: r(c), libelle: r(lbl), texte: r(texte),
               mot: texte.textContent.trim() };
    });
  const panneau = document.querySelector('[data-panneau="reglages"]');
  return {
    cases,
    panneau: r(panneau),
    // Débordement horizontal : le corps ne doit JAMAIS défiler latéralement.
    debordement: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    copies: r(document.querySelector('#p-copies')),
    blocs: [...document.querySelectorAll('.reglage-bloc')].map(r),
  };
});

console.log('\n--- Cases à cocher du panneau Réglages ---');
for (const c of mesures.cases) {
  console.log(`  ${c.id.padEnd(12)} case ${c.boite.w}x${c.boite.h} px à x=${c.boite.x}` +
              ` | texte « ${c.mot} » à x=${c.texte.x}`);
}

for (const c of mesures.cases) {
  // C'ÉTAIT LE BUG : une case de 700 px de large au lieu de 18.
  v(`${c.id} : la case fait bien 18 px de large`, c.boite.w <= 24, `${c.boite.w} px`);
  v(`${c.id} : la case fait bien 18 px de haut`, c.boite.h <= 24, `${c.boite.h} px`);
  // Le texte doit suivre la case IMMÉDIATEMENT, pas être rejeté à l'autre bout.
  const ecart = c.texte.x - (c.boite.x + c.boite.w);
  v(`${c.id} : le texte suit la case`, ecart >= 0 && ecart <= 20, `${ecart} px d'écart`);
  // La case doit être à GAUCHE du bloc, pas centrée au milieu du formulaire.
  v(`${c.id} : la case est à gauche du libellé`,
    c.boite.x - c.libelle.x <= 4, `décalée de ${c.boite.x - c.libelle.x} px`);
}

v('le champ « copies » garde une largeur de champ, pas de case',
  mesures.copies.w > 100, `${mesures.copies.w} px`);
v('les trois blocs de réglage sont dessinés', mesures.blocs.length === 3);
v('aucun bloc écrasé à 0 px', mesures.blocs.every(b => b.h > 30));
v('aucun débordement horizontal de la page', mesures.debordement <= 0,
  `${mesures.debordement} px`);

// --- Les autres onglets doivent réellement disparaître ----------------------
const panneaux = await page.evaluate(() => {
  const o = {};
  document.querySelectorAll('[data-panneau]').forEach(p => {
    o[p.dataset.panneau] = getComputedStyle(p).display;
  });
  return o;
});
v('seul le panneau Réglages est affiché',
  panneaux.reglages !== 'none' && panneaux.dest === 'none'
  && panneaux.protection === 'none' && panneaux.journal === 'none',
  JSON.stringify(panneaux));

await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/sauvegarde-reglages.png', fullPage: true });

// --- Modale d'ajout de dossier ---------------------------------------------
await page.click('.tab[data-vue="dest"]');
await new Promise(r => setTimeout(r, 200));
await page.click('#btn-add-dest');
await new Promise(r => setTimeout(r, 500));

const modale = await page.evaluate(() => {
  const r = el => { const b = el.getBoundingClientRect();
    return { w: Math.round(b.width), h: Math.round(b.height) }; };
  const boite = document.querySelector('#modal-dest .modal');
  return {
    ouverte: getComputedStyle(document.querySelector('#modal-dest')).display !== 'none',
    boite: r(boite),
    parcourirVisible: getComputedStyle(document.querySelector('#btn-parcourir')).display !== 'none',
    exploVisible: getComputedStyle(document.querySelector('#zone-explo')).display !== 'none',
    // Un champ écrasé à 0 px de haut est le symptôme d'un flex mal réglé.
    // ⚠️ On ne compte QUE les champs affichés : un panneau volontairement
    // masqué mesure 0 px, ce qui n'est pas un défaut de mise en page.
    champsPlats: [...boite.querySelectorAll('.field')]
      .filter(f => getComputedStyle(f).display !== 'none')
      .filter(f => f.getBoundingClientRect().height < 20).length,
    debordement: boite.scrollWidth - boite.clientWidth,
  };
});
console.log('\n--- Modale « Ajouter un dossier » ---');
console.log('  ', JSON.stringify(modale));

v('la modale s\'ouvre', modale.ouverte);
v('le bouton « Parcourir » est proposé', modale.parcourirVisible);
v('l\'explorateur de repli reste masqué tant que le sélecteur marche',
  !modale.exploVisible);
v('aucun champ écrasé dans la modale', modale.champsPlats === 0,
  `${modale.champsPlats} champ(s) à moins de 20 px`);
v('la modale ne déborde pas horizontalement', modale.debordement <= 0);

await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/sauvegarde-ajout-dossier.png' });

v('aucune erreur JavaScript', erreurs.length === 0, erreurs.join(' | '));

console.log(`\ncapture sauvegarde : ${ok.length}/${ok.length + ko.length} vérifications`);
if (ko.length) { console.log('\nÉCHECS :'); ko.forEach(n => console.log('  ✗ ' + n)); }
console.log('\nCaptures : captures_ecran/sauvegarde-reglages.png, sauvegarde-ajout-dossier.png');

await navigateur.close();
process.exit(ko.length ? 1 : 0);
