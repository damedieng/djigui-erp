// Mesure de l'écran « Paramètres de paie » dans le VRAI Chrome.
//
// ⚠️ jsdom ne calcule pas la mise en page : sept onglets, six tableaux
// éditables et un formulaire, c'est exactement le genre d'écran où un champ
// s'écrase à 0 px ou un tableau déborde sans que rien ne le signale.
//
// Usage : node capture-paie-parametres.mjs [port]
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
  args: ['--no-sandbox', '--window-size=1600,1100'],
});
const page = await navigateur.newPage();
await page.setViewport({ width: 1600, height: 1100 });

const erreurs = [];
page.on('pageerror', e => erreurs.push(String(e)));
page.on('console', m => { if (m.type() === 'error') erreurs.push(m.text()); });
await page.evaluateOnNewDocument(() => {
  sessionStorage.setItem('djigui_user', JSON.stringify({
    id: '543bbf1d-33e1-4f22-92c1-71ca1cbf9457', nom: 'Administrateur', role: 'admin',
  }));
});

await page.goto(`http://localhost:${port}/paie-parametres.html`, { waitUntil: 'networkidle2' });
await page.waitForSelector('#corps-irpp tr');
await new Promise(r => setTimeout(r, 700));

const ok = [], ko = [];
const v = (nom, cond, detail = '') => (cond ? ok : ko).push(nom + (detail ? ` — ${detail}` : ''));

// --- Le bandeau d'avertissement : la raison d'être de l'écran ---------------
const etat = await page.evaluate(() => ({
  bandeauVisible: getComputedStyle(document.querySelector('#bandeau-verif')).display !== 'none',
  bandeauTexte: document.querySelector('#txt-verif').textContent.replace(/\s+/g, ' ').trim(),
  nbAlertes: document.querySelectorAll('#alertes .alert').length,
  alertes: [...document.querySelectorAll('#alertes .alert')]
    .map(a => a.textContent.replace(/\s+/g, ' ').trim()),
  nbTranches: document.querySelectorAll('#corps-irpp tr').length,
  // Le bandeau de période porte le geste « nouvelle période ».
  periode: document.querySelector('#periode-irpp').textContent.replace(/\s+/g, ' ').trim(),
  boutonPeriode: !!document.querySelector('[data-periode="irpp"]'),
  boutonVerifie: !!document.querySelector('[data-verifie="irpp"]'),
}));

console.log('\n--- État initial ---');
console.log('  Tranches IR :', etat.nbTranches, '| alertes :', etat.nbAlertes);
console.log('  Bandeau     :', etat.bandeauTexte.slice(0, 110) + '…');
console.log('  Période     :', etat.periode.slice(0, 120));

v('le bandeau « à vérifier » est affiché', etat.bandeauVisible);
// ⚠️ Le point décisif : personne ne doit croire que Djigui connaît la loi.
v('il dit que les sources se contredisent',
  etat.bandeauTexte.includes('se contredisent'));
v('il renvoie au texte en vigueur', etat.bandeauTexte.includes('DGID'));
v('les 6 tranches d\'impôt sont dessinées', etat.nbTranches === 6, `${etat.nbTranches}`);
v('l\'alerte du taux accident du travail remonte',
  etat.alertes.some(a => a.includes('accident du travail')));
v('le bandeau de période annonce la date de début', etat.periode.includes('2026-01-01'));
v('le bouton « Nouvelle période » est proposé', etat.boutonPeriode);
v('le bouton de confirmation de conformité aussi', etat.boutonVerifie);

// --- Chaque onglet doit s'ouvrir et être réellement rempli ------------------
const ONGLETS = ['irpp', 'cotisations', 'famille', 'trimf', 'abattement', 'primes', 'employeur'];
for (const vue of ONGLETS) {
  await page.evaluate(v => document.querySelector(`.tab[data-vue="${v}"]`).click(), vue);
  await new Promise(r => setTimeout(r, 200));
  const m = await page.evaluate(v => {
    const p = document.querySelector(`[data-panneau="${v}"]`);
    const r = p.getBoundingClientRect();
    const autres = [...document.querySelectorAll('[data-panneau]')]
      .filter(x => x.dataset.panneau !== v)
      .filter(x => getComputedStyle(x).display !== 'none').length;
    // Un champ à moins de 20 px est écrasé (flex mal réglé).
    const champsPlats = [...p.querySelectorAll('input, select')]
      .filter(c => getComputedStyle(c).display !== 'none')
      .filter(c => c.type !== 'checkbox')
      .filter(c => c.getBoundingClientRect().height < 20).length;
    return {
      visible: getComputedStyle(p).display !== 'none',
      hauteur: Math.round(r.height),
      autres,
      nbChamps: p.querySelectorAll('input, select').length,
      champsPlats,
      deborde: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  }, vue);
  v(`onglet ${vue} : s'affiche`, m.visible);
  v(`onglet ${vue} : les autres se ferment`, m.autres === 0, `${m.autres} restés ouverts`);
  v(`onglet ${vue} : pas écrasé`, m.hauteur > 80, `${m.hauteur} px`);
  v(`onglet ${vue} : contient des champs`, m.nbChamps > 0, `${m.nbChamps}`);
  v(`onglet ${vue} : aucun champ écrasé`, m.champsPlats === 0, `${m.champsPlats}`);
  v(`onglet ${vue} : aucun débordement horizontal`, m.deborde <= 0, `${m.deborde} px`);
  await page.screenshot({ path: `D:/DJGUI_ERP/captures_ecran/paie-param-${vue}.png` });
}

// --- La saisie doit se répercuter dans le brouillon -------------------------
await page.evaluate(() => document.querySelector('.tab[data-vue="irpp"]').click());
await new Promise(r => setTimeout(r, 200));
const saisie = await page.evaluate(() => {
  const champ = document.querySelector('#corps-irpp input[data-cle="taux"]');
  champ.value = '12';
  champ.dispatchEvent(new Event('input', { bubbles: true }));
  return { relu: document.querySelector('#corps-irpp input[data-cle="taux"]').value };
});
v('la saisie d\'un taux est acceptée', saisie.relu === '12');

// L'ajout d'une ligne doit être immédiat, sans aller-retour serveur : on
// prépare son jeu de valeurs AVANT de valider une période.
await page.click('[data-ajout="irpp"]');
await new Promise(r => setTimeout(r, 250));
const apresAjout = await page.evaluate(() =>
  document.querySelectorAll('#corps-irpp tr').length);
v('ajouter une tranche fonctionne hors ligne', apresAjout === 7, `${apresAjout} lignes`);

// --- L'aide -----------------------------------------------------------------
const aide = await page.evaluate(() =>
  document.querySelector('.aide').textContent.replace(/\s+/g, ' '));
v('l\'aide dit qu\'aucun taux n\'est dans le logiciel',
  aide.includes('ne connaît aucun taux'));
v('l\'aide explique qu\'on ouvre une période au lieu de modifier',
  aide.includes('on ouvre une nouvelle période'));
v('l\'aide explique la conséquence sur les bulletins passés',
  aide.includes('recalculé avec les taux de juin'));

v('aucune erreur JavaScript', erreurs.length === 0, erreurs.join(' | '));

console.log(`\ncapture paie-paramètres : ${ok.length}/${ok.length + ko.length} vérifications`);
if (ko.length) { console.log('\nÉCHECS :'); ko.forEach(x => console.log('  ✗ ' + x)); }

await navigateur.close();
process.exit(ko.length ? 1 : 0);
