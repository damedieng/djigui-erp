// Vérifie que la cloche se construit et réagit, sur une page quelconque.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';
const appjs = readFileSync('D:/DJGUI_ERP/frontend/assets/app.js', 'utf8');
const ok = [], ko = [];
const v = (n, c) => (c ? ok : ko).push(n);

const NOTIFS = [
  { cle: 'projet-retard:p1:12', categorie: 'Projets', gravite: 'urgent',
    titre: 'Projet en retard : Chantier', detail: '12 jours', lien: 'projet-detail.html?id=p1', lu: false },
  { cle: 'stock:4', categorie: 'Stock', gravite: 'attention',
    titre: '4 articles sous le seuil', detail: 'Riz', lien: 'articles.html', lu: false },
  { cle: 'rdv:2026-07-25:2', categorie: 'Agenda', gravite: 'attention',
    titre: '2 rendez-vous aujourd\'hui', detail: '', lien: 'agenda.html', lu: true },
];
const appels = [];
// La vraie feuille de style est INJECTÉE : sans elle, on ne testerait que
// l'attribut `hidden` et on manquerait les pièges d'affichage (un `display`
// d'auteur écrase le display:none de [hidden]).
const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
const html = readFileSync('D:/DJGUI_ERP/frontend/tiers.html', 'utf8')
  .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, '')
  .replace(/<script src="assets\/(vendor|tabler)[^"]*"><\/script>/g, '')
  .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
  .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '');
const vc = new VirtualConsole();
const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'http://localhost:1704/tiers.html', virtualConsole: vc });
const w = dom.window, d = w.document;
w.sessionStorage.setItem('djigui_user', JSON.stringify({ id: 'u1', nom: 'T', role: 'admin' }));
w.fetch = async (url, opts) => {
  const chemin = String(url);
  appels.push({ chemin, method: (opts && opts.method) || 'GET' });
  let corps = '{}';
  const methode = (opts && opts.method) || 'GET';
  if (chemin.includes('/api/notifications') && methode === 'GET') corps = JSON.stringify(NOTIFS);
  return { ok: true, text: async () => corps };
};
w.eval(appjs);
d.dispatchEvent(new w.Event('DOMContentLoaded'));
await new Promise(r => setTimeout(r, 200));

v('la cloche est posée', !!d.getElementById('notif-cloche'));
v('les notifications sont demandées', appels.some(a => a.chemin.includes('/api/notifications')));
const past = d.getElementById('notif-pastille');
v('la pastille compte les NON lues (2 sur 3)', past.textContent === '2' && past.hidden === false);
// On teste l'AFFICHAGE calculé, pas seulement l'attribut : c'est la différence
// entre « le code croit avoir fermé » et « l'utilisateur voit fermé ».
const visible = () => w.getComputedStyle(d.getElementById('notif-panneau')).display !== 'none';
v('le panneau est masqué au départ', !visible());

d.getElementById('notif-cloche').click();
await new Promise(r => setTimeout(r, 120));
v('le clic affiche le panneau', visible());
v('3 notifications affichées', d.querySelectorAll('.notif-item').length === 3);
v('3 catégories distinctes', d.querySelectorAll('.notif-groupe').length === 3);
v('la lue est grisée', d.querySelectorAll('.notif-item.lu').length === 1);
v('les liens pointent vers les écrans', d.querySelector('.notif-item').getAttribute('href') === 'projet-detail.html?id=p1');

// Fermeture : 2e clic sur la cloche, sur l'icone, sur la pastille, et Echap.
d.querySelector('#notif-cloche i').click();
await new Promise(r => setTimeout(r, 120));
v("un 2e clic sur l'icone masque vraiment", !visible());

d.getElementById('notif-cloche').click();
await new Promise(r => setTimeout(r, 80));
d.getElementById('notif-pastille').click();
await new Promise(r => setTimeout(r, 80));
v('un clic sur la pastille masque vraiment', !visible());

d.getElementById('notif-cloche').click();
await new Promise(r => setTimeout(r, 80));
d.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Escape' }));
await new Promise(r => setTimeout(r, 80));
v('Echap masque vraiment', !visible());

d.getElementById('notif-cloche').click();
await new Promise(r => setTimeout(r, 80));
d.body.click();
await new Promise(r => setTimeout(r, 80));
v('un clic ailleurs masque vraiment', !visible());

// On rouvre pour la suite du test.
d.getElementById('notif-cloche').click();
await new Promise(r => setTimeout(r, 120));

const avant = appels.length;
d.getElementById('notif-tout-lu').click();
await new Promise(r => setTimeout(r, 150));
v('« tout marquer lu » envoie les clés', appels.slice(avant).some(a => a.chemin.includes('/lues') && a.method === 'POST'));

console.log('--- RÉSULTATS ---');
ok.forEach(x => console.log('  OK   ' + x));
ko.forEach(x => console.log('  KO   ' + x));
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
process.exit(ko.length ? 1 : 0);
