// Capture des onglets Avenants et Réception dans le VRAI Chrome.
//
// ⚠️ Raison d'être : jsdom ne calcule pas la mise en page. Les tests de fumée
// étaient verts pendant que des modales étaient cassées (classes CSS
// inexistantes). Pour tout ce qui est visuel, on passe par Chrome et on MESURE.
//
// Usage : le serveur doit tourner. Le marché visé est passé en argument :
//   node capture-avenants.mjs <id-du-marche> [port]
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const idMarche = process.argv[2];
const port = process.argv[3] || '1704';
if (!idMarche) { console.error('Usage : node capture-avenants.mjs <id-du-marche> [port]'); process.exit(1); }

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
  args: ['--no-sandbox', '--window-size=1600,1000'],
});
const page = await navigateur.newPage();
await page.setViewport({ width: 1600, height: 1000 });

const erreurs = [];
page.on('pageerror', e => erreurs.push(String(e)));
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });

await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});

await page.goto(`http://localhost:${port}/marche-detail.html?id=${idMarche}`,
                { waitUntil: 'networkidle2' });
await page.waitForSelector('#corps-avenants');

const clicOnglet = async nom => {
  await page.evaluate(n => {
    [...document.querySelectorAll('#onglets .tab')].find(t => t.dataset.tab === n).click();
  }, nom);
  await new Promise(r => setTimeout(r, 300));
};

// --- Onglet Avenants --------------------------------------------------------
await clicOnglet('avenants');
const mAv = await page.evaluate(() => {
  const tuiles = [...document.querySelectorAll('#tuiles .tuile')];
  const lignes = [...document.querySelectorAll('#corps-avenants tr')];
  const doc = document.documentElement;
  return {
    // Le bug n° 1 du projet : un enfant de .content écrasé à 0 px.
    tuilesEcrasees: tuiles.filter(t => t.getBoundingClientRect().height < 30).length,
    nbTuiles: tuiles.length,
    libellesTuiles: tuiles.map(t => t.querySelector('.tuile-lib').textContent.trim()),
    nbLignes: lignes.length,
    lignesEcrasees: lignes.filter(l => l.getBoundingClientRect().height < 20).length,
    piedRempli: (document.getElementById('pied-avenants').textContent || '').trim().length > 0,
    aideVisible: !!document.querySelector('[data-panel="avenants"] .aide'),
    // Le corps de la page ne doit JAMAIS défiler horizontalement.
    deborde: doc.scrollWidth > doc.clientWidth + 1,
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-avenants.png', fullPage: true });

// --- Modale avenant ---------------------------------------------------------
await page.click('#btn-nouvel-avenant');
await page.waitForSelector('#modal-avenant:not([hidden])');
await new Promise(r => setTimeout(r, 250));
const mModale = await page.evaluate(() => {
  const m = document.querySelector('#modal-avenant .modal');
  const labels = [...document.querySelectorAll('#modal-avenant .form-grid > label')];
  const r = m.getBoundingClientRect();
  return {
    largeur: Math.round(r.width), hauteur: Math.round(r.height),
    nbChamps: labels.length,
    // Un libellé collé à son champ = le style n'est pas appliqué.
    champsEcrases: labels.filter(l => l.getBoundingClientRect().height < 40).length,
    croix: !!document.querySelector('#modal-avenant .ti-x'),
    boutons: [...document.querySelectorAll('#modal-avenant .modal-actions .btn')]
      .map(b => b.textContent.trim()),
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-avenant-modale.png' });
await page.evaluate(() => { document.getElementById('modal-avenant').hidden = true; });

// --- Onglet Réception -------------------------------------------------------
await clicOnglet('reception');
const mRec = await page.evaluate(() => {
  const lignes = [...document.querySelectorAll('#corps-receptions tr')];
  return {
    nbLignes: lignes.length,
    lignesEcrasees: lignes.filter(l => l.getBoundingClientRect().height < 20).length,
    aideVisible: !!document.querySelector('[data-panel="reception"] .aide'),
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-reception.png', fullPage: true });

// --- Modale réception : le champ « réserves » apparaît au bon moment --------
await page.click('#btn-nouvelle-reception');
await page.waitForSelector('#modal-reception:not([hidden])');
await new Promise(r => setTimeout(r, 250));
const reservesAvant = await page.evaluate(() =>
  document.getElementById('bloc-reserves').getBoundingClientRect().height);
await page.select('#r-resultat', 'avec_reserves');
await new Promise(r => setTimeout(r, 250));
const mRecModale = await page.evaluate(() => {
  const m = document.querySelector('#modal-reception .modal');
  const labels = [...document.querySelectorAll('#modal-reception .form-grid > label')];
  const r = m.getBoundingClientRect();
  return {
    largeur: Math.round(r.width), hauteur: Math.round(r.height),
    nbChamps: labels.length,
    champsEcrases: labels.filter(l => l.getBoundingClientRect().height < 40).length,
    reservesApres: document.getElementById('bloc-reserves').getBoundingClientRect().height,
    debordeModale: m.scrollHeight > window.innerHeight,
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-reception-modale.png' });

await navigateur.close();

const l = (k, v) => console.log('  ' + k.padEnd(26) + ': ' + v);
console.log('\n=== Onglet Avenants ===');
l('tuiles', `${mAv.nbTuiles} (écrasées : ${mAv.tuilesEcrasees})`);
l('libellés', mAv.libellesTuiles.join(' | '));
l('lignes d\'avenants', `${mAv.nbLignes} (écrasées : ${mAv.lignesEcrasees})`);
l('pied de table', mAv.piedRempli ? 'rempli' : 'VIDE');
l('section d\'aide', mAv.aideVisible ? 'présente' : 'ABSENTE');
l('débordement horizontal', mAv.deborde ? 'OUI (défaut)' : 'non');

console.log('\n=== Modale avenant ===');
l('taille', `${mModale.largeur} × ${mModale.hauteur} px`);
l('champs', `${mModale.nbChamps} (écrasés : ${mModale.champsEcrases})`);
l('croix de fermeture', mModale.croix ? 'présente' : 'ABSENTE');
l('boutons', mModale.boutons.join(' | '));

console.log('\n=== Onglet Réception ===');
l('lignes', `${mRec.nbLignes} (écrasées : ${mRec.lignesEcrasees})`);
l('section d\'aide', mRec.aideVisible ? 'présente' : 'ABSENTE');

console.log('\n=== Modale réception ===');
l('taille', `${mRecModale.largeur} × ${mRecModale.hauteur} px`);
l('champs', `${mRecModale.nbChamps} (écrasés : ${mRecModale.champsEcrases})`);
l('bloc réserves', `masqué ${reservesAvant} px → affiché ${Math.round(mRecModale.reservesApres)} px`);
l('modale plus haute que l\'écran', mRecModale.debordeModale ? 'OUI' : 'non');

console.log('\nerreurs JS : ' + (erreurs.length ? erreurs.join(' | ') : 'aucune'));
console.log('\nCaptures : captures_ecran/marche-avenants.png, marche-avenant-modale.png,'
          + '\n           marche-reception.png, marche-reception-modale.png');
