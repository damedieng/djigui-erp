// Vérifie que la barre latérale centralisée se construit correctement.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const appjs = readFileSync('D:/DJGUI_ERP/frontend/assets/app.js', 'utf8');
const ok = [], ko = [];
const v = (n, c) => (c ? ok : ko).push(n);

async function charger(page, role) {
  const html = readFileSync('D:/DJGUI_ERP/frontend/' + page, 'utf8')
    .replace(/<script src="assets\/app.js[^"]*"><\/script>/, `<script>${appjs}</script>`)
    .replace(/<script src="assets\/(vendor|tabler)[^"]*"><\/script>/g, '');
  const vc = new VirtualConsole();
  const erreurs = [];
  vc.on('jsdomError', e => erreurs.push(e.message));
  const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'http://localhost:1704/' + page, virtualConsole: vc });
  const w = dom.window;
  w.sessionStorage.setItem('djigui_user', JSON.stringify({ id: 'u1', nom: 'Test', role }));
  w.fetch = async () => ({ ok: true, text: async () => '{}' });
  // On n'exécute que app.js (pas le script propre à la page).
  w.eval(appjs);
  w.document.dispatchEvent(new w.Event('DOMContentLoaded'));
  await new Promise(r => setTimeout(r, 50));
  return { d: w.document, erreurs };
}

const ADMIN = 17, CAISSIER = 14;   // 3 entrées data-admin

const { d, erreurs } = await charger('accueil.html', 'admin');
v('app.js sans erreur', erreurs.length === 0);
v(`admin : ${ADMIN} entrées`, d.querySelectorAll('.nav-item').length === ADMIN);
v('la marque est présente', !!d.querySelector('.brand-name'));
v('les 3 groupes sont là', d.querySelectorAll('.nav-label').length === 3);
v('le pied contient 3 entrées', d.querySelectorAll('.sidebar-foot .nav-item').length === 3);
v('Accueil est actif', d.querySelector('.nav-item.active')?.textContent.trim() === 'Accueil');
v("l'entrée active n'est pas un lien", !d.querySelector('.nav-item.active').getAttribute('href'));

const c = await charger('accueil.html', 'caissier');
v(`caissier : ${CAISSIER} entrées (admin retirées)`, c.d.querySelectorAll('.nav-item').length === CAISSIER);
v('caissier : pas de Magasins', ![...c.d.querySelectorAll('.nav-item')].some(a => a.textContent.includes('Magasins')));

// L'entrée active suit la page, y compris sur le détail d'un projet.
const cas = [['projets.html', 'Projets'], ['projet-detail.html', 'Projets'],
             ['tiers.html', 'Tiers'], ['agenda.html', 'Agenda'],
             ['caisse-etat.html', 'État de caisse'], ['abonnements.html', 'Abonnements']];
for (const [page, attendu] of cas) {
  const r = await charger(page, 'admin');
  const actif = r.d.querySelector('.nav-item.active')?.textContent.trim();
  v(`${page} → « ${attendu} » actif`, actif === attendu);
  v(`${page} : ${ADMIN} entrées`, r.d.querySelectorAll('.nav-item').length === ADMIN);
}

console.log('\n--- RÉUSSIS ---'); ok.forEach(x => console.log('  OK   ' + x));
if (ko.length) { console.log('\n--- ÉCHECS ---'); ko.forEach(x => console.log('  KO   ' + x)); }
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
process.exit(ko.length ? 1 : 0);
