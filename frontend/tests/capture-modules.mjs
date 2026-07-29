// Capture de l'écran Modules dans le VRAI Chrome.
// jsdom ne calcule pas la mise en page : c'est ici qu'on vérifie que les cartes
// ne sont pas écrasées, que la vitrine se distingue réellement des modules
// souscrits, et que les interrupteurs sont visibles.
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';
const port = process.argv[2] || '1704';
const CH = ['C:/Program Files/Google/Chrome/Application/chrome.exe',
  process.env.LOCALAPPDATA + '/Google/Chrome/Application/chrome.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe'];
const chrome = CH.find(p => p && existsSync(p));
const nav = await puppeteer.launch({ executablePath: chrome, headless: 'new',
  args: ['--no-sandbox', '--window-size=1700,1100'] });
const page = await nav.newPage();
await page.setViewport({ width: 1700, height: 1100 });
const err = []; page.on('pageerror', e => err.push(String(e)));
page.on('console', m => { if (m.type() === 'error') err.push(m.text()); });
await page.evaluateOnNewDocument(() => sessionStorage.setItem('djigui_user',
  JSON.stringify({ id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin' })));
await page.goto(`http://localhost:${port}/modules.html`, { waitUntil: 'networkidle2' });
await page.waitForSelector('.proj-carte');
await new Promise(r => setTimeout(r, 500));
const m = await page.evaluate(() => {
  const cartes = [...document.querySelectorAll('.proj-carte')];
  const vitrine = cartes.filter(c => c.classList.contains('vitrine'));
  const opac = c => parseFloat(getComputedStyle(c).opacity);
  return {
    nb: cartes.length,
    ecrasees: cartes.filter(c => c.getBoundingClientRect().height < 100).length,
    hauteurs: `${Math.min(...cartes.map(c => Math.round(c.getBoundingClientRect().height)))}–${Math.max(...cartes.map(c => Math.round(c.getBoundingClientRect().height)))} px`,
    nbVitrine: vitrine.length,
    // La vitrine doit se DISTINGUER visuellement, sinon la séparation est vaine.
    vitrineEstompee: vitrine.length ? vitrine.every(c => opac(c) < 0.9) : true,
    souscritOpaque: cartes.filter(c => !c.classList.contains('vitrine')).every(c => opac(c) > 0.95),
    interrupteurs: document.querySelectorAll('.bascule input').length,
    interrupteurVisible: (() => { const b = document.querySelector('.bascule');
      return b ? b.getBoundingClientRect().width > 30 : false; })(),
    tuiles: [...document.querySelectorAll('#tuiles .pe-bloc')].map(t =>
      t.querySelector('.pe-lbl').textContent.trim() + ' = ' + t.querySelector('.pe-val').textContent.trim()),
    familles: [...document.querySelectorAll('.fam-titre')].map(f => f.textContent.trim()),
    debordePage: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/modules.png', fullPage: true });
await nav.close();
const l = (k, v) => console.log('  ' + k.padEnd(26) + ': ' + v);
console.log('\n=== Écran Modules ===');
l('cartes', `${m.nb} (écrasées : ${m.ecrasees})`);
l('hauteurs', m.hauteurs);
l('cartes en vitrine', `${m.nbVitrine} (estompées : ${m.vitrineEstompee ? 'oui' : 'NON'})`);
l('cartes souscrites nettes', m.souscritOpaque ? 'oui' : 'NON');
l('interrupteurs', `${m.interrupteurs} (visible : ${m.interrupteurVisible ? 'oui' : 'NON'})`);
l('familles', m.familles.join(' | '));
l('débordement de la page', m.debordePage ? 'OUI (défaut)' : 'non');
console.log('\n  tuiles :'); m.tuiles.forEach(t => console.log('    ' + t));
console.log('\nerreurs JS : ' + (err.length ? err.join(' | ') : 'aucune'));
