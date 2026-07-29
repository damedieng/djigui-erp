// Capture du Gantt avec les retards, dans le VRAI Chrome.
//
// ⚠️ jsdom ne calcule pas la mise en page : c'est ici qu'on vérifie que le trait
// « aujourd'hui » tombe au bon endroit et que les hachures couvrent bien la part
// dépassée — deux choses qu'un test logique ne peut pas voir.
//
// Usage : node capture-gantt-retard.mjs <id-projet> [port]
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const idProjet = process.argv[2];
const port = process.argv[3] || '1704';
if (!idProjet) { console.error('Usage : node capture-gantt-retard.mjs <id-projet> [port]'); process.exit(1); }

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

await page.goto(`http://localhost:${port}/projet-detail.html?id=${idProjet}`,
                { waitUntil: 'networkidle2' });
await page.waitForSelector('#corps-taches tr');
await new Promise(r => setTimeout(r, 400));

// --- Onglet Liste -----------------------------------------------------------
const mListe = await page.evaluate(() => {
  const lignes = [...document.querySelectorAll('#corps-taches tr')];
  const retard = lignes.filter(l => l.classList.contains('en-retard'));
  const jaune = retard.filter(l => {
    const c = getComputedStyle(l.querySelector('td')).backgroundColor;
    return c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent';
  });
  const past = document.querySelectorAll('#corps-taches .retard-info');
  return {
    nbLignes: lignes.length, nbRetard: retard.length,
    // Le fond jaune doit être RÉELLEMENT peint : `tr` ne prend pas toujours un
    // background, c'est la règle sur `> td` qui fait le travail.
    nbJaunesPeintes: jaune.length,
    nbPastilles: past.length,
    pastilleVisible: past.length ? past[0].getBoundingClientRect().width > 8 : false,
    exempleBulle: past.length ? past[0].getAttribute('data-tip').split('\n')[0] : '',
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/gantt-retard-liste.png', fullPage: true });

// --- Onglet Gantt -----------------------------------------------------------
// ⚠️ Les onglets de cet écran s'identifient par `data-vue`, pas `data-tab`.
await page.evaluate(() => {
  const t = [...document.querySelectorAll('#tabs .tab')].find(x => x.dataset.vue === 'gantt');
  if (!t) throw new Error('onglet Gantt introuvable');
  t.click();
});
await page.waitForSelector('#gantt-zone .gantt', { visible: true });
await new Promise(r => setTimeout(r, 700));

const mGantt = await page.evaluate(() => {
  const zone = document.getElementById('gantt-zone');
  const traits = [...zone.querySelectorAll('.g-row .g-today')];
  const etiquette = zone.querySelector('.g-today-h');
  const barresRetard = [...zone.querySelectorAll('.g-bar.en-retard')];
  const hachures = [...zone.querySelectorAll('.g-bar-retard')];
  const droite = zone.querySelector('.gantt-right');
  // Le trait doit être un vrai trait : visible, et à la même abscisse partout.
  const xs = traits.map(t => Math.round(t.getBoundingClientRect().left));
  const uniques = [...new Set(xs)];
  // Les hachures doivent avoir une largeur réelle, sinon elles ne se voient pas.
  const largeursHachures = hachures.map(h => Math.round(h.getBoundingClientRect().width));
  return {
    nbTraits: traits.length,
    traitAligne: uniques.length <= 2,   // tolérance d'arrondi
    traitVisible: traits.length ? traits[0].getBoundingClientRect().height > 20 : false,
    etiquetteTexte: etiquette ? (zone.querySelector('.g-today-lbl') || {}).textContent : '(absente)',
    nbBarresRetard: barresRetard.length,
    nbHachures: hachures.length,
    hachuresVides: largeursHachures.filter(w => w < 2).length,
    largeurHachureMax: Math.max(0, ...largeursHachures),
    nbLignesGaucheRetard: zone.querySelectorAll('.g-left-row.en-retard').length,
    legende: (zone.querySelector('.g-legende') || {}).textContent.replace(/\s+/g, ' ').trim(),
    // Le corps de la page ne doit pas défiler horizontalement (le Gantt, si).
    debordePage: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
    ganttScrollable: droite ? droite.scrollWidth > droite.clientWidth : false,
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/gantt-retard.png' });

// Vue rapprochée sur la zone du planning.
const cadre = await page.$('#gantt-zone');
if (cadre) await cadre.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/gantt-retard-zoom.png' });

// --- L'info-bulle maison ----------------------------------------------------
// C'est le remplaçant du `title` du navigateur : on vérifie qu'elle apparaît,
// qu'elle est lisible et qu'elle reste DANS l'écran (une bulle coupée ne sert
// à rien, et c'est le défaut classique d'une bulle posée en bord de fenêtre).
await page.hover('#gantt-zone .g-left-row.en-retard .retard-info');
await new Promise(r => setTimeout(r, 450));
const mTip = await page.evaluate(() => {
  const t = document.querySelector('.dj-tip');
  if (!t) return { present: false };
  const r = t.getBoundingClientRect();
  const s = getComputedStyle(t);
  return {
    present: true,
    opacite: s.opacity,
    largeur: Math.round(r.width), hauteur: Math.round(r.height),
    dansEcran: r.left >= 0 && r.top >= 0
            && r.right <= window.innerWidth && r.bottom <= window.innerHeight,
    multiligne: t.textContent.includes('\n'),
    extrait: t.textContent.split('\n').slice(0, 2).join(' / '),
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/gantt-retard-bulle.png' });

await navigateur.close();

const l = (k, v) => console.log('  ' + k.padEnd(30) + ': ' + v);
console.log('\n=== Liste des activités ===');
l('lignes', mListe.nbLignes);
l('lignes en retard', mListe.nbRetard);
l('fonds jaunes réellement peints', mListe.nbJaunesPeintes);
l('pastilles « i »', `${mListe.nbPastilles} (visible : ${mListe.pastilleVisible ? 'oui' : 'NON'})`);
l('exemple d\'info-bulle', mListe.exempleBulle);

console.log('\n=== Gantt ===');
l('traits « aujourd\'hui »', `${mGantt.nbTraits} (visible : ${mGantt.traitVisible ? 'oui' : 'NON'})`);
l('trait aligné sur toutes lignes', mGantt.traitAligne ? 'oui' : 'NON — décalé');
l('étiquette d\'en-tête', mGantt.etiquetteTexte);
l('barres en retard', mGantt.nbBarresRetard);
l('hachures', `${mGantt.nbHachures} (largeur nulle : ${mGantt.hachuresVides}, max ${mGantt.largeurHachureMax} px)`);
l('lignes de gauche marquées', mGantt.nbLignesGaucheRetard);
l('légende', mGantt.legende);
l('débordement de la page', mGantt.debordePage ? 'OUI (défaut)' : 'non');
l('planning scrollable', mGantt.ganttScrollable ? 'oui' : 'non');

console.log('\n=== Info-bulle maison ===');
if (!mTip.present) { console.log('  ABSENTE — la bulle ne s\'est pas ouverte au survol'); }
else {
  l('taille', `${mTip.largeur} × ${mTip.hauteur} px (opacité ${mTip.opacite})`);
  l('entièrement dans l\'écran', mTip.dansEcran ? 'oui' : 'NON — coupée');
  l('multi-lignes', mTip.multiligne ? 'oui' : 'non');
  l('extrait', mTip.extrait);
}

console.log('\nerreurs JS : ' + (erreurs.length ? erreurs.join(' | ') : 'aucune'));
console.log('\nCaptures : captures_ecran/gantt-retard-liste.png, gantt-retard.png,'
          + '\n           gantt-retard-zoom.png, gantt-retard-bulle.png');
