// Test dans un VRAI navigateur (Chrome piloté), avec mise en page réelle.
// jsdom ne calcule pas la mise en page : il validait la logique pendant que
// l'écran était cassé. Ici on mesure ce que l'utilisateur voit vraiment.
import puppeteer from 'puppeteer-core';

const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const PID = 'a8728797-7615-4e8b-bf94-4cde871d037b';
const URL = `http://localhost:1704/projet-detail.html?id=${PID}`;

const navigateur = await puppeteer.launch({
  executablePath: CHROME, headless: 'new',
  args: ['--no-sandbox', '--window-size=1600,900'],
});
const page = await navigateur.newPage();
await page.setViewport({ width: 1600, height: 900 });

const erreurs = [];
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });
page.on('pageerror', e => erreurs.push('pageerror: ' + e.message));

// Session ouverte : sinon app.js redirige vers l'écran de connexion.
await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user',
    JSON.stringify({ id: '3c72026eb10416cd12c42aff59f8bca7', nom: 'Administrateur', role: 'admin' }));
});
await page.goto(URL, { waitUntil: 'networkidle0' });
await new Promise(r => setTimeout(r, 1200));

const ok = [], ko = [];
const v = (n, c, info = '') => (c ? ok : ko).push(n + (info ? ` — ${info}` : ''));

// L'en-tête est-il RÉELLEMENT visible à l'arrivée ?
const mesurer = () => page.evaluate(() => {
  const zone = document.querySelector('.content');
  const hero = document.getElementById('projet-hero');
  const r = hero ? hero.getBoundingClientRect() : null;
  const collant = document.querySelector('.projet-collant');
  const rc = collant ? collant.getBoundingClientRect() : null;
  return {
    scroll: zone ? zone.scrollTop : -1,
    heroHaut: r ? Math.round(r.top) : null,
    heroHauteur: r ? Math.round(r.height) : null,
    heroVisible: !!r && r.height > 0 && r.bottom > 0 && r.top < window.innerHeight,
    collantHaut: rc ? Math.round(rc.top) : null,
    collantVisible: !!rc && rc.height > 0 && rc.top < window.innerHeight && rc.bottom > 0,
    tuiles: document.querySelectorAll('#p-synthese .pe-bloc').length,
    onglet: (document.querySelector('#tabs .tab.active') || {}).textContent?.trim(),
  };
});

let m = await mesurer();
v("en-tête visible à l'arrivée", m.heroVisible, `haut=${m.heroHaut}px hauteur=${m.heroHauteur}`);
v('9 tuiles de chiffres', m.tuiles === 9, `${m.tuiles} trouvées`);
// Les montants ne doivent pas être coupés dans leur tuile.
const coupes = await page.evaluate(() => [...document.querySelectorAll('#p-synthese .pe-val')]
  .filter(e => e.scrollWidth > e.clientWidth + 1).map(e => e.textContent.trim()));
v('aucun montant tronqué', coupes.length === 0, coupes.join(' / '));

// On descend franchement, comme l'utilisateur, puis on change d'onglet.
const ONGLETS = ['gantt', 'personne', 'ressources', 'jalons', 'documents', 'liste'];
for (const vue of ONGLETS) {
  await page.evaluate(() => { document.querySelector('.content').scrollTop = 900; });
  await new Promise(r => setTimeout(r, 120));
  await page.click(`#tabs .tab[data-vue="${vue}"]`);
  await new Promise(r => setTimeout(r, 350));
  m = await mesurer();
  v(`${vue} : retour en haut`, m.scroll === 0, `scrollTop=${m.scroll}`);
  v(`${vue} : en-tête visible`, m.heroVisible, `haut=${m.heroHaut}px`);
  v(`${vue} : ${m.tuiles} tuiles`, m.tuiles === 9);
}

// Barre collante : reste-t-elle en haut quand on descend ?
await page.click('#tabs .tab[data-vue="gantt"]');
await new Promise(r => setTimeout(r, 300));
await page.evaluate(() => { document.querySelector('.content').scrollTop = 900; });
await new Promise(r => setTimeout(r, 300));
m = await mesurer();
v('barre collante visible après défilement', m.collantVisible, `haut=${m.collantHaut}px`);
v('barre collante bien collée en haut', m.collantHaut !== null && m.collantHaut < 140, `haut=${m.collantHaut}px`);

await page.screenshot({ path: 'reel-gantt-defile.png' });
await page.evaluate(() => { document.querySelector('.content').scrollTop = 0; });
await new Promise(r => setTimeout(r, 200));
await page.screenshot({ path: 'reel-haut.png' });

console.log('--- RÉSULTATS (vrai navigateur) ---');
ok.forEach(x => console.log('  OK   ' + x));
ko.forEach(x => console.log('  KO   ' + x));
if (erreurs.length) {
  console.log('\n--- ERREURS CONSOLE ---');
  [...new Set(erreurs)].forEach(e => console.log('  ' + e.slice(0, 160)));
}
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
await navigateur.close();
process.exit(ko.length ? 1 : 0);
