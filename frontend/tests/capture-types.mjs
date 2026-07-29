// Capture de l'écran Marchés (barre de recherche) et de la modale
// « Types de marché », dans le VRAI Chrome. Voir capture-marche.mjs pour la
// raison d'être : jsdom ne calcule pas la mise en page.
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

await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});
await page.goto('http://localhost:1704/marches.html', { waitUntil: 'networkidle2' });
await page.waitForSelector('#btn-types');

// --- Barre de recherche : les champs doivent avoir la MÊME hauteur ---------
const barre = await page.evaluate(() => {
  const h = s => Math.round(document.querySelector(s).getBoundingClientRect().height);
  return {
    recherche: h('#f-texte'), statut: h('#f-statut'),
    du: h('#f-du'), au: h('#f-au'), affichage: h('#f-retard'),
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-recherche.png',
                        clip: { x: 240, y: 60, width: 1360, height: 330 } });

// --- Modale des types de marché -------------------------------------------
await page.click('#btn-types');
await page.waitForSelector('#modal-types:not([hidden])');
await new Promise(r => setTimeout(r, 400));
const types = await page.evaluate(() => {
  const m = document.querySelector('#modal-types .modal');
  const r = m.getBoundingClientRect();
  return {
    largeur: Math.round(r.width), hauteur: Math.round(r.height),
    debordeH: m.scrollWidth > m.clientWidth + 1,
    familles: document.querySelectorAll('#liste-types [data-type]').length,
    etapes: document.querySelectorAll('#liste-etapes-type .etape-type').length,
    titre: document.getElementById('titre-type').textContent.trim(),
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/marche-types.png' });
await navigateur.close();

const memeHauteur = new Set(Object.values(barre)).size === 1;
console.log('Barre de recherche — hauteurs réelles des champs :');
for (const [k, v] of Object.entries(barre)) console.log(`  ${k.padEnd(10)} : ${v} px`);
console.log(`  → tous alignés : ${memeHauteur ? 'OUI' : 'NON (défaut)'}`);
console.log('\nModale « Types de marché » :');
console.log(`  taille                : ${types.largeur} × ${types.hauteur} px`);
console.log(`  débordement horizontal: ${types.debordeH ? 'OUI (défaut)' : 'non'}`);
console.log(`  familles listées      : ${types.familles}`);
console.log(`  étapes de « ${types.titre} » : ${types.etapes}`);
console.log(`  erreurs JS            : ${erreurs.length ? erreurs.join(' | ') : 'aucune'}`);
console.log('\nCaptures : captures_ecran/marche-recherche.png et marche-types.png');
