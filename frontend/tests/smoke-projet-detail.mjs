// Test de fumée de projet-detail.html : on charge la page dans un DOM, on
// remplace le réseau par des données réalistes, et on vérifie que le script ne
// plante pas et que les commandes clés sont bien câblées.
//
// ⚠️ jsdom n'a PAS de moteur de rendu : getBoundingClientRect renvoie toujours 0.
// Ce test ne valide donc PAS la géométrie des flèches, seulement la logique.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const PROJ = 'p1';
const T = (id, nom, d, f, parent = null) => ({
  id, projet_id: PROJ, nom, tache_parente_id: parent,
  date_debut_prevue: d, date_fin_prevue: f, debut_calcule: d, fin_calcule: f,
  statut: 'a_faire', avancement: 0, budget: 0, niveau: parent ? 2 : 1,
  a_enfants: false, nb_jours: 5, avancement_calcule: 0, ordre: 0,
});
// 3 activités : « a » est déjà prédécesseur de « b », donc « c » doit rester
// proposable dans la liste (le sélecteur exclut la tâche courante et ses liens).
const taches = [T('a', 'Conception', '2026-09-01', '2026-09-10'),
                T('b', 'Realisation', '2026-09-05', '2026-09-20'),
                T('c', 'Recette', '2026-09-21', '2026-09-30')];
const deps = [{ id: 'd1', tache_id: 'b', tache_nom: 'Realisation',
                predecesseur_id: 'a', predecesseur_nom: 'Conception',
                type: 'fin_debut', decalage: 0 }];

const REPONSES = {
  '/api/projets/p1': { id: PROJ, nom: 'Test', statut: 'en_cours', budget_global: 0,
    date_debut_prevue: '2026-09-01', date_fin_prevue: '2026-09-30',
    date_debut_calculee: '2026-09-01', date_fin_calculee: '2026-09-20',
    budget_planifie: 0, cout_ressources: 0, cout_main_oeuvre: 0, budget_taches: 0,
    avancement: 0, avancement_physique: 0, avancement_budgetaire: 0, nb_taches: 2, nb_terminees: 0 },
  '/api/projets/p1/taches': taches,
  '/api/projets/p1/ressources': [],
  // Deux personnes, plusieurs activités : de quoi vérifier les sous-totaux.
  '/api/projets/p1/assignations': [
    { id: 'x1', tache_id: 'a', intervenant_id: 'i1', intervenant_nom: 'Moussa Fall',
      intervenant_type: 'externe', tache_nom: 'Conception', type_taux: 'journalier',
      taux: 100000, heures_allouees: 7, cout: 700000 },
    { id: 'x2', tache_id: 'b', intervenant_id: 'i1', intervenant_nom: 'Moussa Fall',
      intervenant_type: 'externe', tache_nom: 'Realisation', type_taux: 'journalier',
      taux: 100000, heures_allouees: 3, cout: 300000 },
    { id: 'x3', tache_id: 'c', intervenant_id: 'i2', intervenant_nom: 'Awa Diop',
      intervenant_type: 'interne', tache_nom: 'Recette', type_taux: 'horaire',
      taux: 5000, heures_allouees: 8, cout: 40000 },
  ],
  '/api/intervenants': [
    { id: 'i1', nom: 'Moussa Fall', type: 'externe', type_taux: 'journalier', taux: 100000, actif: true },
    { id: 'i2', nom: 'Awa Diop', type: 'interne', type_taux: 'horaire', taux: 5000, actif: true },
  ],
  '/api/projets/p1/jalons': [],
  '/api/projets/p1/livrables': [],
  '/api/projets/p1/documents-joints': [],
  '/api/projets/p1/dependances': deps,
  '/api/projets/p1/coherence': { violations: [{ dependance_id: 'd1', tache_id: 'b',
      tache_nom: 'Realisation', predecesseur_id: 'a', predecesseur_nom: 'Conception',
      debut_actuel: '2026-09-05', debut_attendu: '2026-09-11', jours: 6 }],
    changements: [{ tache_id: 'b', tache_nom: 'Realisation', debut_avant: '2026-09-05',
      debut_apres: '2026-09-11', fin_avant: '2026-09-20', fin_apres: '2026-09-26', jours: 6 }] },
  '/api/tiers?role=client': [], '/api/utilisateurs': [],
};

const appels = [];
const erreurs = [];
const vc = new VirtualConsole();
vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
const html = readFileSync('D:/DJGUI_ERP/frontend/projet-detail.html', 'utf8')
  .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
  .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
  // app.js est chargé par <script src> : jsdom ne le récupère pas, on le neutralise
  // et on fournit un Djigui minimal.
  .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
    window.Djigui = {
      api: async (chemin, opts) => {
        appelsJS.push({ chemin, method: (opts && opts.method) || 'GET' });
        if (opts && opts.method && opts.method !== 'GET') return {};
        const r = REPONSES_JS[chemin.split('?')[0]] ?? REPONSES_JS[chemin];
        if (r === undefined) throw new Error('404 ' + chemin);
        return JSON.parse(JSON.stringify(r));
      },
      fmt: n => String(n), esc: s => String(s ?? ''),
      dateFr: s => s || '', toast: () => {}, alert: () => {}, confirm: async () => true,
      // Le vrai composant renvoie un objet piloté ensuite par la page.
      selectRecherche: () => ({ setItems(){}, setValue(){}, getValue(){ return ''; }, value: '' }),
    };
  </script>`);

const dom = new JSDOM(html, {
  runScripts: 'dangerously', url: 'http://localhost:1704/projet-detail.html?id=' + PROJ,
  virtualConsole: vc, pretendToBeVisual: true, beforeParse(fenetre) {
    // Réglage hérité d'une ancienne session : il ne doit plus rien masquer.
    fenetre.localStorage.setItem('proj-info-collapsed', '1');
  },
});
const w = dom.window;
w.appelsJS = appels;
w.REPONSES_JS = REPONSES;

await new Promise(r => setTimeout(r, 500));

const d = w.document;
const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);

v('aucune erreur JS', erreurs.length === 0);
v('les activités sont rendues', d.querySelectorAll('#corps-taches tr').length >= 3);
v('les dépendances ont été chargées', appels.some(a => a.chemin.includes('/dependances')));
v('la cohérence a été chargée', appels.some(a => a.chemin.includes('/coherence')));
v('bandeau « Harmoniser » présent', !!d.getElementById('btn-harmoniser'));

// Le Gantt est masqué au chargement : les flèches ne peuvent pas être mesurées.
// Elles doivent donc être retracées au moment où l'onglet devient visible —
// c'est précisément le bug « les liens disparaissent après rechargement ».
let appelsFleches = 0;
const vraiTrace = w.dessinerFleches;
w.dessinerFleches = function () { appelsFleches++; return vraiTrace.apply(this, arguments); };

// Onglet Gantt : les barres et les actions rapides doivent exister.
d.querySelector('#tabs .tab[data-vue="gantt"]').click();
await new Promise(r => setTimeout(r, 120));
v('onglet Gantt affiché', d.querySelector('[data-vue="gantt"]:not(.tab)').hidden === false);
v('barres identifiées (data-tache)', d.querySelectorAll('.g-bar[data-tache]').length === 3);
v('actions rapides présentes', d.querySelectorAll('.g-acts .g-act').length >= 4);
v('action Modifier câblée', !!d.querySelector('.gantt-left [data-edit]'));
v('action Avancement câblée', !!d.querySelector('.gantt-left [data-prog]'));
v("flèches retracées à l'ouverture de l'onglet Gantt", appelsFleches >= 1);

// Modale d'activité : le bloc prédécesseurs doit survivre à une ouverture en
// création (c'est le bug du DOM détruit).
w.ouvrirTache(null);
v('création : sélecteur toujours présent', !!d.getElementById('t-pred-sel'));
v('création : champ décalage désactivé', d.getElementById('t-pred-dec').disabled === true);
w.ouvrirTache(taches[1]);
v('modification : sélecteur réactivé', d.getElementById('t-pred-sel').disabled === false);
v('modification : lien existant affiché', d.querySelectorAll('#t-pred-liste .pred-chip').length === 1);
v('modification : t-id renseigné', d.getElementById('t-id').value === 'b');

// Enregistrement automatique au choix de l'activité.
const sel = d.getElementById('t-pred-sel');
const dispo = [...sel.options].find(o => o.value && o.value !== 'b');
v('une activité est proposée', !!dispo);
if (dispo) {
  const avant = appels.length;
  sel.value = dispo.value;
  sel.dispatchEvent(new w.Event('change'));
  await new Promise(r => setTimeout(r, 200));
  const poste = appels.slice(avant).find(a => a.chemin === '/api/dependances' && a.method === 'POST');
  v('le choix déclenche le POST (enregistrement auto)', !!poste);
}

// --- Répartition des ressources humaines (sous-total par personne) --------
const repLignes = [...d.querySelectorAll('#corps-repartition tr')];
v('tableau de répartition rempli', repLignes.length === 5);   // 3 lignes + 2 sous-totaux
v('un sous-total par personne', d.querySelectorAll('#corps-repartition .rep-sous-total').length === 2);
const st = [...d.querySelectorAll('#corps-repartition .rep-sous-total')];
v('sous-total Moussa Fall correct', st[0] && st[0].textContent.includes('1000000'));
v('total main-d\'oeuvre affiché', (d.getElementById('rep-total').textContent || '').length > 0);
v('résumé du nombre de personnes', d.getElementById('rep-resume').textContent.includes('2 personne'));

// --- Taux modifiable en place --------------------------------------------
v('taux éditable dans la liste', d.querySelectorAll('#interv-liste .taux-vif').length === 2);

// --- Réglage rapide de la durée ------------------------------------------
w.ouvrirTache(taches[0]);
const duree = d.getElementById('t-duree');
v('boutons de durée présents', d.querySelectorAll('.duree-rapide [data-jours]').length === 4);
duree.value = 5;
const plus7 = d.querySelector('.duree-rapide [data-jours="7"]');
if (plus7) {
  plus7.click();
  v('+7 ajoute bien 7 jours', Number(duree.value) === 12);
  d.querySelector('.duree-rapide [data-jours="-1"]').click();
  v('le bouton moins retire un jour', Number(duree.value) === 11);
}

// En-tête : toujours complet, et surtout STABLE d'un onglet à l'autre.
v('tuiles de synthèse rendues', d.querySelectorAll('#p-synthese .pe-bloc').length === 9);
v('chaque tuile a sa pastille', d.querySelectorAll('#p-synthese .pe-ic').length === 9);
v("l'en-tête est présent", !!d.getElementById('projet-hero'));
// Barre collante : elle doit rester visible pendant le défilement d'un long
// Gantt, être TOUJOURS remplie (jamais un bandeau vide) et identique partout.
const collant = d.querySelector('.projet-collant');
v('la barre collante existe', !!collant);
v('elle est bien collante', w.getComputedStyle(collant).position === 'sticky');
v('elle a un fond opaque', w.getComputedStyle(collant).backgroundColor !== 'transparent');
v('les onglets sont dedans', !!collant.querySelector('#tabs'));
v('les chiffres sont remplis', d.querySelectorAll('#pc-infos .pc-item').length === 5);
v('le nom du projet y figure', (d.querySelector('.pc-nom') || {}).textContent === 'Test');
let barreStable = true;
for (const vue of ['gantt', 'personne', 'ressources', 'jalons', 'documents', 'liste']) {
  d.querySelector(`#tabs .tab[data-vue="${vue}"]`).click();
  if (d.querySelectorAll('#pc-infos .pc-item').length !== 5) barreStable = false;
  if (w.getComputedStyle(d.getElementById('pc-infos')).display === 'none') barreStable = false;
}
v('la barre reste remplie sur les 6 onglets', barreStable);

// Retour en haut à chaque onglet : c'est ce qui garantit qu'on retrouve
// TOUJOURS les mêmes informations en arrivant, quel que soit l'onglet.
const zoneDefil = d.querySelector('.content');
let remonteToujours = true;
for (const vue of ['gantt', 'ressources', 'jalons', 'documents', 'liste']) {
  zoneDefil.scrollTop = 400;                    // l'utilisateur avait descendu
  d.querySelector(`#tabs .tab[data-vue="${vue}"]`).click();
  if (zoneDefil.scrollTop !== 0) remonteToujours = false;
}
v("on revient en haut à chaque changement d'onglet", remonteToujours);

// Le repli reste une décision de l'utilisateur, et il fonctionne dans les deux sens.
const bloc = d.getElementById('p-collapsible');
d.getElementById('p-toggle').click();
v('le chevron replie', bloc.hidden === true);
d.getElementById('p-toggle').click();
v('le chevron déplie', bloc.hidden === false);
// Régression corrigée : le repli était mémorisé, donc les informations
// s'affichaient puis disparaissaient à chaque ouverture.
v('les infos ne se replient pas toutes seules', d.getElementById('p-collapsible').hidden === false);
v('le réglage hérité a été purgé', w.localStorage.getItem('proj-info-collapsed') === null);
v('les tuiles sont réellement affichées',
  w.getComputedStyle(d.getElementById('p-synthese')).display !== 'none');
// Régressions à ne plus jamais réintroduire : l'en-tête se vidait à l'écran et
// la hauteur du haut de page changeait selon l'onglet.
v("l'en-tête n'est pas collant", w.getComputedStyle(d.getElementById('projet-hero')).position !== 'sticky');
v('aucun mode compact résiduel', !d.getElementById('projet-hero').classList.contains('mini'));
v("le bandeau du titre est visible", w.getComputedStyle(d.querySelector('.hero-bandeau')).display !== 'none');
v('les jauges restent visibles', w.getComputedStyle(d.getElementById('p-jauges')).display !== 'none');

// La zone au-dessus des onglets doit être identique sur TOUS les onglets :
// c'est ce qui empêche le contenu de sauter quand on navigue.
const hautDePage = () => {
  const avant = [];
  let n = d.getElementById('projet-hero');
  while (n) { avant.push(n.id || n.className); n = n.nextElementSibling; if (n && n.id === 'tabs') break; }
  return avant.join('|');
};
const refHaut = hautDePage();
let stable = true;
for (const vue of ['gantt', 'personne', 'ressources', 'jalons', 'documents', 'liste']) {
  d.querySelector(`#tabs .tab[data-vue="${vue}"]`).click();
  if (hautDePage() !== refHaut) stable = false;
  if (d.getElementById('projet-hero').classList.contains('mini')) stable = false;
}
v("le haut de page ne change pas d'un onglet à l'autre", stable);

console.log('\n--- RÉUSSIS ---');
ok.forEach(x => console.log('  OK   ' + x));
if (ko.length) { console.log('\n--- ÉCHECS ---'); ko.forEach(x => console.log('  KO   ' + x)); }
if (erreurs.length) { console.log('\n--- ERREURS JS ---'); erreurs.forEach(e => console.log('  ' + e)); }
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
process.exit(ko.length || erreurs.length ? 1 : 0);
