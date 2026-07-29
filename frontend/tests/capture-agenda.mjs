// Capture de l'agenda avec calendriers superposés, dans le VRAI Chrome.
// Ce qu'on mesure et que jsdom ne saurait pas dire : la grille ne déborde pas,
// les cellules ne sont pas écrasées par le nombre d'échéances, et les repères
// superposés se distinguent réellement des rendez-vous.
import { existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';
const port = process.argv[2] || '1704';
const CH = ['C:/Program Files/Google/Chrome/Application/chrome.exe',
  process.env.LOCALAPPDATA + '/Google/Chrome/Application/chrome.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe'];
const chrome = CH.find(p => p && existsSync(p));
const nav = await puppeteer.launch({ executablePath: chrome, headless: 'new',
  args: ['--no-sandbox', '--window-size=1700,1150'] });
const page = await nav.newPage();
await page.setViewport({ width: 1700, height: 1150 });
const err = []; page.on('pageerror', e => err.push(String(e)));
page.on('console', m => { if (m.type() === 'error') err.push(m.text()); });
await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin' }));
  localStorage.removeItem('agenda-sources-masquees');   // tout allumé au départ
});
await page.goto(`http://localhost:${port}/agenda.html`, { waitUntil: 'networkidle2' });
await page.waitForSelector('#cal-grid .cal-cell');
await new Promise(r => setTimeout(r, 900));

const m = await page.evaluate(() => {
  const cells = [...document.querySelectorAll('.cal-cell')];
  const doc = document.documentElement;
  return {
    barreVisible: !document.getElementById('carte-sources').hidden,
    sources: [...document.querySelectorAll('.cal-src')].map(s => s.textContent.replace(/\s+/g,' ').trim()),
    pastillesColorees: [...document.querySelectorAll('.cal-src .pastille')]
      .every(p => getComputedStyle(p).backgroundColor !== 'rgba(0, 0, 0, 0)'),
    nbExtras: document.querySelectorAll('.cal-ext').length,
    nbRdv: document.querySelectorAll('.cal-rdv').length,
    enRetard: document.querySelectorAll('.cal-ext.retard').length,
    // Les repères superposés doivent avoir un liseré coloré, sinon rien ne les
    // distingue d'un rendez-vous.
    liseresColores: [...document.querySelectorAll('.cal-ext')]
      .every(e => getComputedStyle(e).borderLeftWidth !== '0px'),
    cellulesEcrasees: cells.filter(c => c.getBoundingClientRect().height < 40).length,
    hauteurCellule: cells.length ? Math.round(cells[0].getBoundingClientRect().height) : 0,
    // Une cellule qui déborde masquerait des échéances sans le dire.
    cellulesQuiDebordent: cells.filter(c => c.scrollHeight > c.clientHeight + 2).length,
    mentionsPlus: document.querySelectorAll('.cal-plus').length,
    pageDeborde: doc.scrollWidth > doc.clientWidth + 1,
  };
});
await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/agenda-superpose.png', fullPage: true });

// L'aperçu : lecture seule, avec un lien vers la fiche d'origine.
let ap = { present: false };
if (m.nbExtras) {
  await page.click('.cal-ext');
  await new Promise(r => setTimeout(r, 400));
  ap = await page.evaluate(() => {
    const d = document.getElementById('modal-apercu');
    if (!d || d.hidden) return { present: false };
    return {
      present: true,
      titre: document.getElementById('apercu-titre').textContent.trim(),
      corps: document.getElementById('apercu-corps').textContent.replace(/\s+/g,' ').trim().slice(0,130),
      lien: document.getElementById('apercu-lien').getAttribute('href'),
      // Aucun champ modifiable : c'est tout l'enjeu.
      champsSaisie: d.querySelectorAll('input,select,textarea').length,
    };
  });
  await page.screenshot({ path: 'D:/DJGUI_ERP/captures_ecran/agenda-apercu.png' });
}
await nav.close();
const l = (k, v) => console.log('  ' + k.padEnd(28) + ': ' + v);
console.log('\n=== Agenda superposé ===');
l('barre des calendriers', m.barreVisible ? 'visible' : 'MASQUÉE');
l('calendriers', m.sources.join(' | '));
l('pastilles colorées', m.pastillesColorees ? 'oui' : 'NON');
l('rendez-vous affichés', m.nbRdv);
l('échéances superposées', `${m.nbExtras} (en retard : ${m.enRetard})`);
l('liserés colorés', m.liseresColores ? 'oui' : 'NON');
l('cellules', `hauteur ${m.hauteurCellule} px (écrasées : ${m.cellulesEcrasees})`);
l('cellules qui débordent', m.cellulesQuiDebordent);
l('mentions « +N autre(s) »', m.mentionsPlus);
l('débordement de la page', m.pageDeborde ? 'OUI (défaut)' : 'non');
console.log('\n=== Aperçu au clic ===');
if (!ap.present) console.log("  l'aperçu ne s'est pas ouvert");
else {
  l('titre', ap.titre);
  l('contenu', ap.corps);
  l('lien vers la fiche', ap.lien);
  l('champs modifiables', ap.champsSaisie + (ap.champsSaisie === 0 ? ' (lecture seule ✓)' : ' ⚠ DEVRAIT ÊTRE 0'));
}
console.log('\nerreurs JS : ' + (err.length ? err.join(' | ') : 'aucune'));
