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

// ⚠️ Les totaux sont DÉDUITS de MENU, pas écrits à la main : sinon ajouter un
// écran fait échouer huit tests sans rapport, et on prend l'habitude de
// « corriger » le chiffre sans regarder ce qui a changé.
//
// 7 entrées réservées à l'administrateur : Magasins, Compléter mes prix,
// Comptabilité, Modules, Sauvegarde, Utilisateurs, Journal d'audit. L'écran
// comptable est réservé — le commerçant ne doit jamais le voir ; les prix
// d'achat non plus (donnée sensible). « Modules » pose une donnée de
// FACTURATION, et « Sauvegarde » donne accès aux dossiers de la machine.
const SOURCE_MENU = readFileSync('D:/DJGUI_ERP/frontend/assets/app.js', 'utf8');
const ENTREES_MENU = [...SOURCE_MENU.matchAll(/\{\s*href:\s*'[^']+'[^}]*\}/g)].map(m => m[0]);
const ADMIN = ENTREES_MENU.length;
const CAISSIER = ENTREES_MENU.filter(e => !/admin:\s*1/.test(e)).length;
// Pied de barre : Utilisateurs, Journal d'audit, Modules, Sauvegarde, Paramètres.
const PIED = 5;

const { d, erreurs } = await charger('accueil.html', 'admin');
v('app.js sans erreur', erreurs.length === 0);
v(`admin : ${ADMIN} entrées`, d.querySelectorAll('.nav-item').length === ADMIN);
v('la marque est présente', !!d.querySelector('.brand-name'));
// Groupes : Commerce, Projets & Marchés, Paie & RH, Catalogue, Contacts.
// « Projets & Marchés » est isolé parce que l'application peut être livrée avec
// ces deux modules seuls (cas d'une ONG) — voir MENU dans app.js.
//
// ⚠️ Le total est DÉDUIT de MENU, pas écrit à la main : sinon ajouter un groupe
// fait échouer un test sans rapport, et on prend l'habitude de « corriger » le
// chiffre sans regarder ce qui a changé.
// ⚠️ Ancré sur un début de ligne : la ligne de COMMENTAIRE qui documente le
// format (« //   { groupe: 'Titre' } séparateur de section ») était sinon
// comptée comme un vrai groupe, et le test réclamait 6 groupes au lieu de 5.
const GROUPES = [...SOURCE_MENU.matchAll(/^\s*\{\s*groupe:\s*'([^']+)'/gm)].map(m => m[1]);
v(`les ${GROUPES.length} groupes sont là`,
  d.querySelectorAll('.nav-label').length === GROUPES.length);
v('le groupe « Projets & Marchés » existe',
  [...d.querySelectorAll('.nav-groupe')].some(g => g.dataset.groupe === 'Projets & Marchés'));
v('Projets et Marchés sont dans ce groupe', (() => {
  const corps = d.querySelector('.nav-groupe-corps[data-corps="Projets & Marchés"]');
  const libelles = [...corps.querySelectorAll('.nav-item')].map(a => a.textContent.trim());
  return libelles.length === 2 && libelles.includes('Projets') && libelles.includes('Marchés');
})());
// ⚠️ Les groupes sont DÉPLIÉS par défaut : rien ne doit disparaître au premier
// lancement. Seul un repli explicite de l'utilisateur est mémorisé.
v('les groupes sont dépliés par défaut',
  [...d.querySelectorAll('.nav-groupe-corps')].every(c => !c.hidden));
// Pied : Modules, Utilisateurs, Journal d'audit, Paramètres.
v(`le pied contient ${PIED} entrées`,
  d.querySelectorAll('.sidebar-foot .nav-item').length === PIED);
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

// ===========================================================================
// Filtrage par MODULE (migration 0040)
//
// Un module non souscrit ou masqué ne figure PAS dans le menu de travail — il
// reste seulement en vitrine sur l'écran Modules. C'est la contrepartie de la
// facturation : ce qui n'est pas vendu ne s'utilise pas.
// ===========================================================================
async function chargerAvecModules(visibles) {
  const html = readFileSync('D:/DJGUI_ERP/frontend/accueil.html', 'utf8')
    .replace(/<script src="assets\/app.js[^"]*"><\/script>/, `<script>${appjs}</script>`)
    .replace(/<script src="assets\/(vendor|tabler)[^"]*"><\/script>/g, '');
  const dom = new JSDOM(html, { runScripts: 'outside-only',
    url: 'http://localhost:1704/accueil.html', virtualConsole: new VirtualConsole() });
  const w = dom.window;
  w.sessionStorage.setItem('djigui_user', JSON.stringify({ id: 'u1', nom: 'Test', role: 'admin' }));
  if (visibles) w.localStorage.setItem('djigui-modules-visibles', JSON.stringify(visibles));
  w.fetch = async () => ({ ok: true, text: async () => '{}' });
  w.eval(appjs);
  w.document.dispatchEvent(new w.Event('DOMContentLoaded'));
  await new Promise(r => setTimeout(r, 60));
  return w.document;
}
const libelles = d => [...d.querySelectorAll('.nav-item')].map(a => a.textContent.trim());

// Cas ONG : projets et marchés, sans caisse ni facturation.
{
  const dm = await chargerAvecModules(['socle', 'projets', 'marches', 'agenda']);
  const l = libelles(dm);
  v('module : Projets et Marchés sont là', l.includes('Projets') && l.includes('Marchés'));
  v('module : la Caisse est absente', !l.includes('Caisse'));
  v('module : les Ventes sont absentes', !l.includes('Ventes'));
  v('module : la Production est absente', !l.includes('Production'));
  // Le socle reste : sans lui il n'y a plus d'application.
  v('module : le socle reste (Articles, Tiers, Paramètres)',
    l.includes('Articles') && l.includes('Tiers') && l.includes('Paramètres'));
  // ⚠️ « Agenda » appartient au groupe Commerce dans le MENU : le groupe reste
  // donc affiché ici, et c'est juste — il n'est pas vide.
  v('module : le groupe « Commerce » reste tant qu\'une entrée subsiste',
    [...dm.querySelectorAll('.nav-groupe')].some(g => g.dataset.groupe === 'Commerce'));
  v('module : le groupe « Projets & Marchés » reste',
    [...dm.querySelectorAll('.nav-groupe')].some(g => g.dataset.groupe === 'Projets & Marchés'));
}

// Un groupe dont plus AUCUNE entrée n'est visible ne doit pas laisser un titre
// orphelin : un intitulé sans rien dessous fait croire à un écran cassé.
{
  const dm = await chargerAvecModules(['socle', 'projets', 'marches']);
  const groupes = [...dm.querySelectorAll('.nav-groupe')].map(g => g.dataset.groupe);
  v('module : un groupe entièrement masqué disparaît', !groupes.includes('Commerce'));
  v('module : les groupes non vides restent', groupes.includes('Projets & Marchés'));
  v('module : aucun titre de groupe sans entrée',
    [...dm.querySelectorAll('.nav-groupe-corps')]
      .every(c => c.querySelectorAll('.nav-item').length > 0));
}

// ⚠️ ROBUSTESSE : une liste vide (réponse serveur inattendue, ancienne version)
// ne doit PAS vider le menu. Trop de menu se corrige d'un clic ; plus de menu
// du tout ressemble à une panne.
{
  const dm = await chargerAvecModules([]);
  v("module : une liste vide n'efface pas le menu", libelles(dm).length === ADMIN);
}
{
  const dm = await chargerAvecModules(null);
  v('module : sans information, tout reste visible', libelles(dm).length === ADMIN);
}

console.log('\n--- RÉUSSIS ---'); ok.forEach(x => console.log('  OK   ' + x));
if (ko.length) { console.log('\n--- ÉCHECS ---'); ko.forEach(x => console.log('  KO   ' + x)); }
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
process.exit(ko.length ? 1 : 0);
