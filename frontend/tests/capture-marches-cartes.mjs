// Capture des CARTES de l'écran Marchés et du nouvel en-tête de détail,
// dans le VRAI Chrome.
//
// ⚠️ jsdom ne calcule pas la mise en page : c'est ici qu'on vérifie qu'aucune
// carte n'est écrasée, que la grille tient sans débordement, et que les tuiles
// de l'en-tête sont réellement dessinées.
//
// Usage : node capture-marches-cartes.mjs [id-marche] [port]
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const idMarche = process.argv[2] || '';
const port = process.argv[3] || '1704';

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
  args: ['--no-sandbox', '--window-size=1700,1000'],
});
const page = await navigateur.newPage();
await page.setViewport({ width: 1700, height: 1000 });

const erreurs = [];
page.on('pageerror', e => erreurs.push(String(e)));
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });

await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});

// --- Écran Marchés (cartes) -------------------------------------------------
await page.goto(`http://localhost:${port}/marches.html`, { waitUntil: 'networkidle2' });
await page.waitForSelector('#grille');
await new Promise(r => setTimeout(r, 500));

const mListe = await page.evaluate(() => {
  const cartes = [...document.querySelectorAll('.proj-carte')];
  const doc = document.documentElement;
  const grille = document.getElementById('grille');
  const rects = cartes.map(c => c.getBoundingClientRect());
  // Le bug n° 1 du projet : un enfant de `.content` écrasé à 0 px.
  const ecrasees = rects.filter(r => r.height < 80).length;
  // Combien de cartes par rangée : la grille doit vraiment se répartir.
  const parRangee = new Set(rects.map(r => Math.round(r.top))).size;
  return {
    nb: cartes.length,
    ecrasees,
    // ⚠️ Pas de `0` dans le Math.min : il tirerait le minimum à zéro et
    // ferait croire à une carte écrasée alors qu'il n'y en a pas.
    hauteurMin: rects.length ? Math.round(Math.min(...rects.map(r => r.height))) : 0,
    hauteurMax: rects.length ? Math.round(Math.max(...rects.map(r => r.height))) : 0,
    largeurCarte: rects.length ? Math.round(rects[0].width) : 0,
    rangees: parRangee,
    colonnes: getComputedStyle(grille).gridTemplateColumns.split(' ').length,
    // La case à cocher doit être visible et posée sur la carte.
    coches: document.querySelectorAll('.proj-coche').length,
    cocheVisible: (() => {
      const c = document.querySelector('.proj-coche');
      return c ? c.getBoundingClientRect().width > 6 : false;
    })(),
    pastillesRetard: document.querySelectorAll('.proj-carte .retard-info').length,
    jauges: document.querySelectorAll('.proj-carte .barre').length,
    debordePage: doc.scrollWidth > doc.clientWidth + 1,
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marches-cartes.png', fullPage: true });

// Sélection : la carte cochée doit se distinguer VISUELLEMENT.
let mSelection = { present: false };
if (mListe.nb) {
  await page.click('.proj-carte .proj-coche');
  await new Promise(r => setTimeout(r, 300));
  mSelection = await page.evaluate(() => {
    const c = document.querySelector('.proj-carte.choisie');
    const barre = document.getElementById('barre-lot');
    return {
      present: !!c,
      bordure: c ? getComputedStyle(c).borderColor : '',
      barreVisible: barre ? getComputedStyle(barre).display !== 'none' : false,
      compte: (document.getElementById('lot-compte') || {}).textContent || '',
    };
  });
  await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marches-cartes-selection.png' });
}

// --- Détail d'un marché (nouvel en-tête) ------------------------------------
const cible = idMarche || await page.evaluate(() => {
  const c = document.querySelector('.proj-carte');
  return c ? c.dataset.id : '';
});
let mDetail = { present: false };
if (cible) {
  await page.goto(`http://localhost:${port}/marche-detail.html?id=${cible}`,
                  { waitUntil: 'networkidle2' });
  await page.waitForSelector('#h-synthese .pe-bloc');
  await new Promise(r => setTimeout(r, 400));
  mDetail = await page.evaluate(() => {
    const tuiles = [...document.querySelectorAll('#h-synthese .pe-bloc')];
    const jauges = [...document.querySelectorAll('#h-jauges .barre')];
    const hero = document.querySelector('.projet-hero');
    const bandeau = document.querySelector('.hero-bandeau');
    return {
      present: true,
      nbTuiles: tuiles.length,
      tuilesEcrasees: tuiles.filter(t => t.getBoundingClientRect().height < 40).length,
      // La pastille d'icône fait tout le caractère de la tuile : si elle est
      // à 0, c'est que le style partagé n'a pas été chargé.
      pastillesOk: tuiles.every(t => {
        const i = t.querySelector('.pe-ic');
        return i && i.getBoundingClientRect().width > 20;
      }),
      libelles: tuiles.map(t => t.querySelector('.pe-lbl').textContent.trim()),
      nbJauges: jauges.length,
      jaugesRemplies: jauges.filter(j => j.firstElementChild
        && j.firstElementChild.getBoundingClientRect().width > 0).length,
      heroHauteur: hero ? Math.round(hero.getBoundingClientRect().height) : 0,
      // Le bandeau doit être en dégradé vert, pas blanc.
      bandeauFond: bandeau ? getComputedStyle(bandeau).backgroundImage.slice(0, 40) : '',
      selectStatut: (document.getElementById('h-statut') || {}).value || '',
      debordePage: document.documentElement.scrollWidth
                 > document.documentElement.clientWidth + 1,
    };
  });
  await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-entete.png' });
}

await navigateur.close();

const l = (k, v) => console.log('  ' + k.padEnd(28) + ': ' + v);
console.log('\n=== Écran Marchés (cartes) ===');
l('cartes', `${mListe.nb} (écrasées : ${mListe.ecrasees})`);
l('hauteur des cartes', `${mListe.hauteurMin} → ${mListe.hauteurMax} px`);
l('largeur d\'une carte', mListe.largeurCarte + ' px');
l('colonnes / rangées', `${mListe.colonnes} / ${mListe.rangees}`);
l('cases à cocher', `${mListe.coches} (visible : ${mListe.cocheVisible ? 'oui' : 'NON'})`);
l('pastilles de retard', mListe.pastillesRetard);
l('jauges d\'avancement', mListe.jauges);
l('débordement de la page', mListe.debordePage ? 'OUI (défaut)' : 'non');

console.log('\n=== Sélection ===');
if (!mSelection.present) console.log('  aucune carte marquée « choisie »');
else {
  l('carte marquée', 'oui (bordure ' + mSelection.bordure + ')');
  l('barre de lot', mSelection.barreVisible ? 'visible' : 'MASQUÉE');
  l('compte', mSelection.compte);
}

console.log('\n=== Détail marché (en-tête) ===');
if (!mDetail.present) console.log('  aucun marché à ouvrir');
else {
  l('tuiles', `${mDetail.nbTuiles} (écrasées : ${mDetail.tuilesEcrasees})`);
  l('pastilles d\'icône', mDetail.pastillesOk ? 'toutes dessinées' : 'MANQUANTES');
  l('libellés', mDetail.libelles.join(' | '));
  l('jauges', `${mDetail.nbJauges} (remplies : ${mDetail.jaugesRemplies})`);
  l('hauteur de l\'en-tête', mDetail.heroHauteur + ' px');
  l('fond du bandeau', mDetail.bandeauFond || '(uni — dégradé absent ?)');
  l('sélecteur de statut', mDetail.selectStatut);
  l('débordement de la page', mDetail.debordePage ? 'OUI (défaut)' : 'non');
}

console.log('\nerreurs JS : ' + (erreurs.length ? erreurs.join(' | ') : 'aucune'));
console.log('\nCaptures : captures_ecran/marches-cartes.png,'
          + '\n           marches-cartes-selection.png, marche-entete.png');
