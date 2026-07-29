// Test de fumée de modules.html — activation des modules.
//
// ⚠️ Ce n'est PAS un filtre d'affichage : la souscription est une **donnée de
// facturation** posée à l'installation selon la formule vendue. Ce test
// verrouille la distinction, qui est tout l'enjeu :
//
//   souscrit  → décidé par l'installateur, le client n'y touche pas
//   actif     → décidé par le client, simple confort d'affichage
//
// Et les trois garde-fous : le socle ne se masque pas, un module non souscrit
// ne s'active pas, et masquer prévient avant — sinon l'utilisateur croit qu'il
// a effacé son travail.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const MODULES = [
  { code: 'socle', libelle: 'Base', description: 'Articles, contacts, paramètres.',
    icone: 'ti-cube', famille: 'Base', ordre: 1, socle: true,
    souscrit: true, actif: true, visible: true, requiert: [] },
  { code: 'caisse', libelle: 'Caisse', description: 'Vendre au comptoir.',
    icone: 'ti-cash', famille: 'Commerce', ordre: 10, socle: false,
    souscrit: true, souscrit_le: '2026-07-28T09:00:00', souscrit_par: 'djigui',
    actif: true, visible: true, requiert: ['socle'],
    contenu: [{ libelle: 'encaissements', nb: 42 }] },
  { code: 'facturation', libelle: 'Facturation', description: 'Devis et factures.',
    icone: 'ti-file-invoice', famille: 'Commerce', ordre: 11, socle: false,
    souscrit: true, actif: true, visible: true, requiert: ['socle'] },
  { code: 'abonnements', libelle: 'Abonnements', description: 'Facturer à échéance.',
    icone: 'ti-repeat', famille: 'Commerce', ordre: 12, socle: false,
    souscrit: true, actif: false, visible: false, requiert: ['facturation'] },
  // Souscrit et affiché, avec des données : c'est lui qui doit déclencher
  // l'avertissement avant masquage.
  { code: 'marches', libelle: 'Marchés', description: 'Appels d\'offres, attribution.',
    icone: 'ti-gavel', famille: 'Projets & Marchés', ordre: 21, socle: false,
    souscrit: true, actif: true, visible: true, requiert: ['socle'],
    contenu: [{ libelle: 'marchés', nb: 8 }, { libelle: 'avenants', nb: 3 }] },
  // NON souscrits : la vitrine.
  { code: 'production', libelle: 'Production', description: 'Fabriquer à partir de recettes.',
    icone: 'ti-tools', famille: 'Commerce', ordre: 14, socle: false,
    souscrit: false, actif: true, visible: false, requiert: ['socle'] },
  { code: 'comptabilite', libelle: 'Comptabilité', description: 'Plan comptable OHADA.',
    icone: 'ti-book-2', famille: 'Pilotage', ordre: 41, socle: false,
    souscrit: false, actif: true, visible: false, requiert: ['socle'] },
];

const FORMULES = [
  { code: 'commerce', libelle: 'Commerce', description: 'Le nécessaire d\'un commerçant.',
    modules: ['caisse', 'facturation', 'magasins', 'agenda', 'rapports'] },
  { code: 'ong_projets', libelle: 'ONG / Projets', description: 'Projets et marchés.',
    modules: ['projets', 'marches', 'agenda', 'rapports'] },
  { code: 'sur_mesure', libelle: 'Sur mesure', description: 'Aucun présélectionné.', modules: [] },
];

const REPONSES = {
  '/api/modules': { modules: MODULES, formule_installee: 'commerce' },
  '/api/modules/formules': FORMULES,
};

function monter() {
  const appels = [];
  const erreurs = [];
  const vc = new VirtualConsole();
  vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
  vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

  const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
  const html = readFileSync('D:/DJGUI_ERP/frontend/modules.html', 'utf8')
    .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
    .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
    .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
      window.Djigui = {
        api: async (chemin, opts) => {
          appelsJS.push({ chemin, method: (opts && opts.method) || 'GET', body: opts && opts.body });
          if (opts && opts.method && opts.method !== 'GET') return { ok: true };
          const r = REPONSES_JS[chemin.split('?')[0]] ?? REPONSES_JS[chemin];
          if (r === undefined) throw new Error('404 ' + chemin);
          return JSON.parse(JSON.stringify(r));
        },
        fmt: n => String(n), esc: s => String(s ?? ''),
        dateFr: s => s || '', toast: (msg, t) => { toastsJS.push({ msg, t }); },
        alert: () => {}, confirm: async (q) => { confirmsJS.push(q); return reponseConfirm; },
        estAdmin: () => true, rafraichirMenu: () => { menuRafraichiJS.n++; },
      };
    </script>`);

  const dom = new JSDOM(html, {
    runScripts: 'dangerously', url: 'http://localhost:1704/modules.html',
    virtualConsole: vc, pretendToBeVisual: true,
    beforeParse(f) {
      f.appelsJS = appels; f.REPONSES_JS = REPONSES;
      f.toastsJS = []; f.confirmsJS = []; f.menuRafraichiJS = { n: 0 };
      f.reponseConfirm = true;
    },
  });
  return { w: dom.window, d: dom.window.document, appels, erreurs,
           toasts: dom.window.toastsJS, confirms: dom.window.confirmsJS,
           menu: dom.window.menuRafraichiJS };
}

const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const pause = (ms = 120) => new Promise(r => setTimeout(r, ms));

const { w, d, appels, erreurs, toasts, confirms, menu } = monter();
await pause(350);
const visible = el => el && w.getComputedStyle(el).display !== 'none';
const carte = code => d.querySelector(`.proj-carte[data-code="${code}"]`);
const txt = sel => (d.querySelector(sel)?.textContent || '').replace(/\s+/g, ' ').trim();

// --- Rendu ------------------------------------------------------------------
v('aucune erreur JS', erreurs.length === 0);
v('les modules sont chargés', appels.some(a => a.chemin === '/api/modules'));
v('les formules sont chargées', appels.some(a => a.chemin === '/api/modules/formules'));
v('chaque module a sa carte', d.querySelectorAll('.proj-carte').length === MODULES.length);

// --- La séparation « mes modules » / vitrine --------------------------------
// C'est tout l'enjeu : la vitrine montre ce que le système sait faire, sans
// jamais encombrer le menu de travail.
v('les modules souscrits sont dans « Mes modules »',
  d.querySelector('#mes-modules').contains(carte('caisse')));
v('les non souscrits sont dans la vitrine',
  d.querySelector('#vitrine').contains(carte('production')));
v('une carte de vitrine est marquée à part',
  carte('production').classList.contains('vitrine'));
v('une carte souscrite ne l\'est pas', !carte('caisse').classList.contains('vitrine'));
v('la vitrine annonce qu\'elle n\'encombre pas le menu',
  txt('#vitrine').includes('apparaissent pas dans votre menu'));

// --- Les trois garde-fous ---------------------------------------------------
const bascule = code => carte(code).querySelector('[data-bascule]');
v('un module souscrit a son interrupteur', !!bascule('caisse'));
// Le socle ne se masque pas : sans lui il n'y a plus d'application.
v('le socle n\'a PAS d\'interrupteur', !bascule('socle'));
v('le socle dit pourquoi', carte('socle').textContent.includes('Ne peut pas être masqué'));
// Un module non souscrit ne s'active pas depuis cet écran.
v('un module non souscrit n\'a pas d\'interrupteur', !bascule('production'));
v('il renvoie vers Djigui',
  carte('production').textContent.includes('Contactez Djigui'));
v('il est marqué « Non souscrit »',
  carte('production').textContent.includes('Non souscrit'));

// L'état actif/masqué se lit sur la carte.
v('un module masqué est marqué comme tel',
  carte('abonnements').textContent.includes('Masqué'));
v('un module affiché aussi', carte('caisse').textContent.includes('Affiché'));
v('l\'interrupteur reflète l\'état', bascule('caisse').checked && !bascule('abonnements').checked);

// --- Masquer : on prévient AVANT ---------------------------------------------
bascule('marches').checked = false;
bascule('marches').dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(250);
v('masquer un module demande confirmation', confirms.length > 0);
const q = confirms[confirms.length - 1] || '';
v('la confirmation dit ce que contient le module', q.includes('8 marchés'));
v('elle liste tout le contenu', q.includes('3 avenants'));
// ⚠️ LE point : l'utilisateur doit savoir que rien n'est effacé.
v('elle rassure : les données sont CONSERVÉES', q.includes('CONSERVÉES'));
v('elle dit que tout revient à la réactivation', q.includes('réaffichez'));
const maj = appels.find(a => a.chemin === '/api/modules/marches/actif');
v('la requête part avec le bon état', maj && maj.body.actif === false);
// Le menu doit être reconstruit : c'est tout l'intérêt de l'opération.
v('le menu est rafraîchi', menu.n > 0);
v('le message rappelle que les données sont gardées',
  toasts.some(t => (t.msg || '').includes('données sont conservées')));

// --- Recherche ---------------------------------------------------------------
d.getElementById('rech').value = 'comptab';
d.getElementById('rech').dispatchEvent(new w.Event('input'));
await pause(150);
v('la recherche filtre les cartes', d.querySelectorAll('.proj-carte').length === 1);
v('elle trouve dans la vitrine aussi', carte('comptabilite') !== null);
// La recherche porte aussi sur la description, pas seulement sur le nom.
d.getElementById('rech').value = 'recettes';
d.getElementById('rech').dispatchEvent(new w.Event('input'));
await pause(150);
v('la recherche porte sur la description', carte('production') !== null);
d.getElementById('rech').value = '';
d.getElementById('rech').dispatchEvent(new w.Event('input'));
await pause(150);

// On peut masquer la vitrine pour ne voir que ses modules.
d.getElementById('voir-vitrine').checked = false;
d.getElementById('voir-vitrine').dispatchEvent(new w.Event('change'));
await pause(150);
v('la vitrine peut être masquée', carte('production') === null);
v('mes modules restent affichés', carte('caisse') !== null);
d.getElementById('voir-vitrine').checked = true;
d.getElementById('voir-vitrine').dispatchEvent(new w.Event('change'));
await pause(150);

// --- La formule (installation) ------------------------------------------------
v('la formule installée est affichée', txt('#tuiles').includes('Commerce'));
d.getElementById('btn-formule').click();
await pause(200);
v('la modale de formule s\'ouvre', !d.getElementById('modal-formule').hidden);
v('les formules sont proposées',
  d.querySelectorAll('#liste-formules input[name="formule"]').length === FORMULES.length);
v('la formule en cours est cochée',
  d.querySelector('input[name="formule"][value="commerce"]').checked);
// Le socle n'est pas une option : il est toujours là.
v('le socle n\'est pas proposé à la vente',
  d.querySelector('#cases-modules [data-mod="socle"]') === null);
v('les modules souscrits sont cochés',
  d.querySelector('#cases-modules [data-mod="caisse"]').checked);

// Choisir une formule ne fait que PRÉ-COCHER : la liste reste ajustable.
const radioOng = d.querySelector('input[name="formule"][value="ong_projets"]');
radioOng.checked = true;
radioOng.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(150);
v('choisir une formule recoche les modules',
  d.querySelector('#cases-modules [data-mod="marches"]').checked
  && !d.querySelector('#cases-modules [data-mod="caisse"]').checked);

// Cocher un module dont la dépendance manque doit AVERTIR.
const cAb = d.querySelector('#cases-modules [data-mod="abonnements"]');
cAb.checked = true;
cAb.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(150);
v('les dépendances manquantes sont annoncées',
  !d.getElementById('avert-formule').hidden
  && txt('#avert-formule').includes('Facturation'));

d.getElementById('btn-enr-formule').click();
await pause(250);
const env = appels.find(a => a.chemin === '/api/modules/formule');
v('la formule est envoyée', env !== undefined);
v('elle porte le code de la formule', env && env.body.formule === 'ong_projets');
v('elle porte la liste AJUSTÉE, pas celle de la formule',
  env && env.body.modules.includes('abonnements'));
v('la modale se referme', d.getElementById('modal-formule').hidden);

// --- Aide ---------------------------------------------------------------------
const aide = txt('.aide');
v('l\'aide explique la formule', aide.includes('formule'));
v('l\'aide dit que masquer n\'efface rien', aide.includes('efface rien'));
v('l\'aide dit que masquer ne change pas la facture', aide.includes('ne change pas votre facture'));
v('l\'aide explique les modules grisés', aide.includes('grisés'));

console.log(`\nmodules : ${ok.length}/${ok.length + ko.length} tests passés`);
if (ko.length) {
  console.log('\nÉCHECS :');
  ko.forEach(n => console.log('  ✗ ' + n));
  if (erreurs.length) { console.log('\nErreurs JS :'); erreurs.forEach(e => console.log('  ' + e)); }
  process.exit(1);
}
