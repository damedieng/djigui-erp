// Test de fumée : les RETARDS doivent se voir sur le planning.
//
// Le reproche de l'utilisateur : « sur les projets, on ne voit pas ce qui est en
// retard ». C'est exact — les barres du Gantt sont colorées **par niveau**, donc
// la couleur est déjà prise et ne peut pas dire l'état. Ce test verrouille les
// quatre réponses apportées :
//   1. un trait « aujourd'hui » sur la frise (sans lui, aucun retard ne se juge) ;
//   2. des hachures rouges sur la part dépassée de la barre ;
//   3. la ligne en jaune dans la liste ET dans le tableau de gauche du Gantt ;
//   4. une pastille « i » qui dit AU SURVOL ce qui est en retard — y compris sur
//      une activité parente dont la branche est repliée.
//
// ⚠️ Les dates sont calées sur la date du JOUR : un jeu figé ferait sortir le
// trait « aujourd'hui » de la frise et le test ne vérifierait plus rien.
// ⚠️ jsdom ne calcule pas la mise en page → le rendu réel est mesuré par
// capture-gantt-retard.mjs (vrai Chrome).
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const jour = n => {
  const d = new Date(); d.setDate(d.getDate() + n);
  return d.toISOString().slice(0, 10);
};
const AUJ = jour(0);

const PROJ = 'p1';
// Un parent avec deux enfants : l'un très en retard, l'autre à venir. Plus une
// activité de premier niveau terminée bien qu'échue — elle ne doit RIEN déclencher.
const taches = [
  { id: 'par', projet_id: PROJ, nom: 'Gros œuvre', tache_parente_id: null,
    date_debut_prevue: jour(-40), date_fin_prevue: jour(10),
    debut_calcule: jour(-40), fin_calcule: jour(10),
    statut: 'en_cours', avancement: 30, budget: 0, niveau: 1, a_enfants: true,
    nb_jours: 50, avancement_calcule: 30, ordre: 0,
    retard_jours: 12, nb_en_retard: 1 },
  { id: 'ret', projet_id: PROJ, nom: 'Fondations', tache_parente_id: 'par',
    date_debut_prevue: jour(-40), date_fin_prevue: jour(-12),
    debut_calcule: jour(-40), fin_calcule: jour(-12),
    statut: 'en_cours', avancement: 60, budget: 0, niveau: 2, a_enfants: false,
    nb_jours: 29, avancement_calcule: 60, ordre: 1,
    retard_jours: 12, nb_en_retard: 1 },
  { id: 'suite', projet_id: PROJ, nom: 'Élévation', tache_parente_id: 'par',
    date_debut_prevue: jour(1), date_fin_prevue: jour(10),
    debut_calcule: jour(1), fin_calcule: jour(10),
    statut: 'a_faire', avancement: 0, budget: 0, niveau: 2, a_enfants: false,
    nb_jours: 10, avancement_calcule: 0, ordre: 2 },
  { id: 'fini', projet_id: PROJ, nom: 'Terrassement', tache_parente_id: null,
    date_debut_prevue: jour(-50), date_fin_prevue: jour(-30),
    debut_calcule: jour(-50), fin_calcule: jour(-30),
    statut: 'terminee', avancement: 100, budget: 0, niveau: 1, a_enfants: false,
    nb_jours: 21, avancement_calcule: 100, ordre: 3 },
];

const REPONSES = {
  '/api/projets/p1': { id: PROJ, nom: 'Chantier', statut: 'en_cours', budget_global: 0,
    date_debut_prevue: jour(-50), date_fin_prevue: jour(10),
    date_debut_calculee: jour(-50), date_fin_calculee: jour(10),
    budget_planifie: 0, cout_ressources: 0, cout_main_oeuvre: 0, budget_taches: 0,
    avancement: 30, avancement_physique: 30, avancement_budgetaire: 0,
    nb_taches: 4, nb_terminees: 1 },
  '/api/projets/p1/taches': taches,
  '/api/projets/p1/ressources': [],
  '/api/projets/p1/assignations': [],
  '/api/intervenants': [],
  '/api/projets/p1/jalons': [],
  '/api/projets/p1/livrables': [],
  '/api/projets/p1/documents-joints': [],
  '/api/projets/p1/dependances': [],
  '/api/projets/p1/coherence': { violations: [], changements: [] },
  '/api/tiers?role=client': [], '/api/utilisateurs': [],
};

function monter() {
  const appels = [];
  const erreurs = [];
  const vc = new VirtualConsole();
  vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
  vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

  const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
  const html = readFileSync('D:/DJGUI_ERP/frontend/projet-detail.html', 'utf8')
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
        dateFr: s => s || '', toast: () => {}, alert: () => {}, confirm: async () => true,
        selectRecherche: () => ({ setItems(){}, setValue(){}, value: '' }),
        estAdmin: () => true,
      };
    </script>`);

  const dom = new JSDOM(html, {
    runScripts: 'dangerously',
    url: 'http://localhost:1704/projet-detail.html?id=p1',
    virtualConsole: vc, pretendToBeVisual: true,
    beforeParse(f) { f.appelsJS = appels; f.REPONSES_JS = REPONSES; },
  });
  return { w: dom.window, d: dom.window.document, appels, erreurs };
}

const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const pause = (ms = 150) => new Promise(r => setTimeout(r, ms));

const { w, d, appels, erreurs } = monter();
await pause(400);

v('aucune erreur JS', erreurs.length === 0);

// --- 1. La liste : jaune + pastille « i » -----------------------------------
const ligne = id => d.querySelector(`#corps-taches tr[data-id="${id}"]`);
v('liste : l\'activité en retard est marquée',
  ligne('ret') && ligne('ret').classList.contains('en-retard'));
v('liste : l\'activité parente est marquée aussi',
  ligne('par') && ligne('par').classList.contains('en-retard'));
// Une activité échue mais TERMINÉE n'est pas en retard : c'est du travail fait.
v('liste : une activité terminée n\'est jamais en retard',
  ligne('fini') && !ligne('fini').classList.contains('en-retard'));
v('liste : une activité à venir n\'est pas marquée',
  ligne('suite') && !ligne('suite').classList.contains('en-retard'));

const pastille = id => ligne(id) && ligne(id).querySelector('.retard-info');
v('liste : la pastille « i » est présente sur l\'activité en retard', !!pastille('ret'));
v('liste : pas de pastille sur une activité à l\'heure', !pastille('suite'));
// Le survol doit dire COMBIEN de jours, sinon la pastille n'apprend rien.
v('liste : l\'info-bulle donne le nombre de jours',
  pastille('ret').getAttribute('data-tip').includes('12'));
v('liste : l\'info-bulle donne la date de fin prévue',
  pastille('ret').getAttribute('data-tip').includes('prévue'));
// Sur une parente, elle doit NOMMER l'activité fautive : savoir qu'il y a un
// retard sans savoir où, c'est inutilisable quand la branche est repliée.
v('liste : sur une parente, l\'info-bulle nomme l\'activité en retard',
  pastille('par').getAttribute('data-tip').includes('Fondations'));

// --- 2. Le Gantt -------------------------------------------------------------
const tab = n => [...d.querySelectorAll('[data-vue]')].find(x => x.dataset.vue === n);
const ongletGantt = [...d.querySelectorAll('.tab, .tabs-folder span, [data-tab]')]
  .find(x => (x.dataset.tab || '') === 'gantt' || x.textContent.trim() === 'Gantt');
if (ongletGantt) ongletGantt.click();
await pause(300);

const zone = d.getElementById('gantt-zone');
v('gantt : le planning est rendu', zone.querySelector('.gantt') !== null);

// Le trait « aujourd'hui » : la référence sans laquelle aucun retard ne se juge.
v('gantt : le trait « aujourd\'hui » est posé sur les lignes',
  zone.querySelectorAll('.g-row .g-today').length > 0);
v('gantt : le trait « aujourd\'hui » est étiqueté dans l\'en-tête',
  zone.querySelector('.g-days-head .g-today-h') !== null);
v('gantt : l\'étiquette dit « aujourd\'hui »',
  (zone.querySelector('.g-today-lbl') || {}).textContent === "aujourd'hui");

// Les hachures : le retard ne peut pas passer par la couleur (prise par le niveau).
const barre = id => zone.querySelector(`.g-bar[data-tache="${id}"]`);
v('gantt : la barre en retard porte la classe en-retard',
  barre('ret') && barre('ret').classList.contains('en-retard'));
v('gantt : la barre en retard porte des hachures',
  barre('ret') && barre('ret').querySelector('.g-bar-retard') !== null);
v('gantt : une barre à l\'heure n\'a ni classe ni hachures',
  barre('suite') && !barre('suite').classList.contains('en-retard')
  && barre('suite').querySelector('.g-bar-retard') === null);
v('gantt : l\'info-bulle de la barre mentionne le retard',
  barre('ret').getAttribute('data-tip').includes('retard'));
// ⚠️ Les hachures doivent avoir une LARGEUR RÉELLE. Un départ à 100 % les rend
// invisibles — c'est le défaut qu'a révélé la mesure dans Chrome, que ce test
// ne voyait pas. Ici « Fondations » est finie depuis 12 jours : toute sa barre
// est échue, donc les hachures partent de 0.
const depart = parseFloat(barre('ret').querySelector('.g-bar-retard').style.left);
v('gantt : les hachures ne sont pas réduites à néant', depart < 100);
v('gantt : les hachures ne débordent pas de la barre', depart >= 0);
v('gantt : une activité entièrement échue est hachurée sur toute sa barre',
  depart === 0);

// Une PARENTE dont la fin est encore à venir mais qui porte le retard d'un
// enfant : là, les hachures ne doivent commencer qu'au trait « aujourd'hui »,
// sinon elles couvriraient du travail réellement fait dans les délais.
const departPar = parseFloat(barre('par').querySelector('.g-bar-retard').style.left);
v('gantt : sur une parente en cours, les hachures partent d\'aujourd\'hui',
  departPar > 0 && departPar < 100);

// Le tableau de gauche doit être marqué aussi, sinon la ligne saute à l'œil
// d'un côté et pas de l'autre.
v('gantt : la ligne de gauche est marquée en retard',
  zone.querySelectorAll('.g-left-row.en-retard').length >= 2);
v('gantt : la pastille « i » est présente à gauche',
  zone.querySelector('.g-left-row.en-retard .retard-info') !== null);

// La légende doit expliquer les deux nouveaux signes.
const legende = (zone.querySelector('.g-legende') || {}).textContent || '';
v('gantt : la légende explique « En retard »', legende.includes('En retard'));
v('gantt : la légende explique « Aujourd\'hui »', legende.includes("Aujourd'hui"));

// --- 3. Rien n'est recalculé -------------------------------------------------
// Barrière « cascade » : afficher un retard ne doit déclencher AUCUNE écriture.
const ecritures = appels.filter(a => a.method !== 'GET');
v('aucune écriture n\'est déclenchée par l\'affichage d\'un retard',
  ecritures.length === 0);

// --- 4. L'aide explique le code visuel --------------------------------------
const aide = (d.querySelector('.aide') || {}).textContent || '';
v('l\'aide explique le trait « aujourd\'hui »', aide.includes("aujourd'hui"));
v('l\'aide explique les hachures', aide.includes('hachures'));
v('l\'aide rappelle que rien n\'est corrigé tout seul',
  aide.includes('jamais corrigé tout seul'));

console.log(`\ngantt-retard : ${ok.length}/${ok.length + ko.length} tests passés`);
if (ko.length) {
  console.log('\nÉCHECS :');
  ko.forEach(n => console.log('  ✗ ' + n));
  if (erreurs.length) { console.log('\nErreurs JS :'); erreurs.forEach(e => console.log('  ' + e)); }
  process.exit(1);
}
