// Capture de la modale « Nouveau marché » dans le VRAI Chrome.
//
// ⚠️ Raison d'être : jsdom ne calcule pas la mise en page. Les 240 tests de
// fumée étaient verts pendant que cette modale était cassée (classes CSS
// inexistantes). Pour tout ce qui est visuel, on passe par Chrome.
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

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

// Session admin : sans elle, la garde d'authentification renvoie au login.
await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});

await page.goto('http://localhost:1704/marches.html', { waitUntil: 'networkidle2' });
await page.waitForSelector('#btn-nouveau');
await page.click('#btn-nouveau');
await page.waitForSelector('#modal-marche:not([hidden])');

// On choisit « Travaux » pour montrer la procédure la plus fournie.
await page.select('#m-type', await page.evaluate(() => {
  const o = [...document.querySelectorAll('#m-type option')]
    .find(x => x.textContent.trim() === 'Travaux');
  return o ? o.value : '';
}));
await page.type('#m-objet', "Construction d'une salle de classe");
await page.type('#m-estime', '25000000');
await new Promise(r => setTimeout(r, 400));

// Mesures réelles : c'est ce que jsdom ne sait pas faire.
const mesures = await page.evaluate(() => {
  const m = document.querySelector('#modal-marche .modal');
  const champs = [...document.querySelectorAll('#form-marche .field')];
  const etapes = document.querySelectorAll('#etapes-liste .etape-ligne');
  const r = m.getBoundingClientRect();
  return {
    largeur: Math.round(r.width), hauteur: Math.round(r.height),
    debordeH: m.scrollWidth > m.clientWidth + 1,
    nbChamps: champs.length,
    // Un champ écrasé à 0 px est le bug classique du projet.
    champsEcrases: champs.filter(c => c.getBoundingClientRect().height < 20).length,
    nbEtapes: etapes.length,
    datesRemplies: [...document.querySelectorAll('#etapes-liste .etape-date')]
      .filter(i => i.value).length,
    resume: document.getElementById('etapes-resume').textContent.trim(),
    croixVisible: !!document.querySelector('#modal-marche .ti-x'),
  };
});

await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-modale.png' });
await navigateur.close();

console.log('Mesures réelles de la modale :');
console.log(`  taille               : ${mesures.largeur} × ${mesures.hauteur} px`);
console.log(`  débordement horizontal: ${mesures.debordeH ? 'OUI (défaut)' : 'non'}`);
console.log(`  champs du formulaire : ${mesures.nbChamps} (écrasés : ${mesures.champsEcrases})`);
console.log(`  étapes affichées     : ${mesures.nbEtapes} — ${mesures.resume}`);
console.log(`  dates pré-remplies   : ${mesures.datesRemplies}`);
console.log(`  croix de fermeture   : ${mesures.croixVisible ? 'présente' : 'ABSENTE'}`);
console.log(`  erreurs JS           : ${erreurs.length ? erreurs.join(' | ') : 'aucune'}`);
console.log('\nCapture : captures_ecran/marche-modale.png');
