// Test de fumée de marches.html — la liste en CARTES.
//
// L'écran affichait un tableau ; il reprend maintenant la présentation en
// cartes de l'écran Projets (styles partagés dans styles.css). Ce test vérifie
// que le passage aux cartes n'a rien perdu de ce que le tableau savait faire :
//   • la sélection multiple et le traitement par lot ;
//   • le clic qui ouvre le marché — SANS que la case à cocher l'ouvre aussi ;
//   • l'annulation avec motif ;
//   • le retard signalé, avec son explication au survol.
//
// ⚠️ jsdom ne calcule pas la mise en page → l'apparence réelle est mesurée par
// capture-marches-cartes.mjs (vrai Chrome).
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const MARCHES = [
  { id: 'm1', numero: 'MA-2026-0001', objet: 'Construction du forage de Ndioum',
    type_id: 'mt-travaux', type_libelle: 'Travaux', montant_estime: 20000000,
    montant_attribue: 21000000, ecart_montant: 1000000, monnaie: 'FCFA',
    statut: 'en_cours', date_lancement: '2026-01-05', date_cloture_prevue: '2026-03-10',
    attributaire_nom: 'Entreprise SOGEA', cree_le: '2026-01-05T08:00:00',
    nb_etapes: 8, nb_etapes_terminees: 3, avancement: 38, retard_jours: 5,
    nb_soumissionnaires: 3, nb_avenants: 2, montant_avenants: 3000000,
    delai_avenants_jours: 20, montant_courant: 24000000, avenants_pct: 14.3,
    nb_receptions: 1, reserves_ouvertes: true,
    // Alertes calculées par le serveur — désormais fournies AUSSI en liste.
    alertes: [
      'Une étape est en retard de 5 jour(s).',
      "2 étape(s) sont prévues AVANT la date de lancement du 2026-01-05 : le retard affiché vient de là.",
      'Des réserves de réception ne sont pas levées : la retenue de garantie reste due.',
    ] },
  { id: 'm2', numero: 'MA-2026-0002', objet: 'Fourniture de tables-bancs',
    type_libelle: 'Fournitures', montant_estime: 4500000, monnaie: 'FCFA',
    statut: 'realise', date_lancement: '2026-02-01', date_cloture_effective: '2026-04-15',
    cree_le: '2026-02-01T08:00:00',
    nb_etapes: 7, nb_etapes_terminees: 7, avancement: 100,
    nb_soumissionnaires: 2, nb_avenants: 0, montant_avenants: 0,
    delai_avenants_jours: 0, montant_courant: 4500000,
    nb_receptions: 1, reserves_ouvertes: false },
  { id: 'm3', numero: 'MA-2026-0003', objet: 'Étude géotechnique', monnaie: 'FCFA',
    montant_estime: 1200000, statut: 'annule', date_lancement: '2026-03-01',
    motif_annulation: 'Budget non voté', cree_le: '2026-03-01T08:00:00',
    nb_etapes: 9, nb_etapes_terminees: 1, avancement: 11,
    nb_soumissionnaires: 0, nb_avenants: 0, montant_avenants: 0,
    delai_avenants_jours: 0, montant_courant: 1200000,
    nb_receptions: 0, reserves_ouvertes: false },
];

const REPONSES = {
  '/api/marches': MARCHES,
  '/api/marche-types': [{ id: 'mt-travaux', code: 'TRAVAUX', libelle: 'Travaux',
    actif: true, duree_totale_jours: 64, nb_marches: 1, etapes: [] }],
  '/api/projets': [], '/api/utilisateurs': [], '/api/tiers': [],
};

function monter() {
  const appels = [];
  const erreurs = [];
  const vc = new VirtualConsole();
  vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
  vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

  const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
  const html = readFileSync('D:/DJGUI_ERP/frontend/marches.html', 'utf8')
    .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
    .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
    .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
      window.Djigui = {
        api: async (chemin, opts) => {
          appelsJS.push({ chemin, method: (opts && opts.method) || 'GET', body: opts && opts.body });
          if (opts && opts.method && opts.method !== 'GET') return { modifies: 2 };
          const r = REPONSES_JS[chemin.split('?')[0]] ?? REPONSES_JS[chemin];
          if (r === undefined) throw new Error('404 ' + chemin);
          return JSON.parse(JSON.stringify(r));
        },
        fmt: n => String(n), esc: s => String(s ?? ''),
        dateFr: s => s || '', toast: (msg, t) => { toastsJS.push({ msg, t }); },
        alert: () => {}, confirm: async () => true,
        selectRecherche: () => ({ setItems(){}, setValue(){}, value: '' }),
        estAdmin: () => true,
      };
    </script>`);

  const dom = new JSDOM(html, {
    runScripts: 'dangerously', url: 'http://localhost:1704/marches.html',
    virtualConsole: vc, pretendToBeVisual: true,
    beforeParse(f) { f.appelsJS = appels; f.REPONSES_JS = REPONSES; f.toastsJS = []; },
  });
  return { w: dom.window, d: dom.window.document, appels, erreurs,
           toasts: dom.window.toastsJS };
}

const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const pause = (ms = 120) => new Promise(r => setTimeout(r, ms));

const { w, d, appels, erreurs, toasts } = monter();
await pause(350);
const visible = el => el && w.getComputedStyle(el).display !== 'none';
const carte = id => d.querySelector(`.proj-carte[data-id="${id}"]`);
const txt = el => (el?.textContent || '').replace(/\s+/g, ' ').trim();

// --- Rendu ------------------------------------------------------------------
v('aucune erreur JS', erreurs.length === 0);
v('les marchés sont chargés', appels.some(a => a.chemin.startsWith('/api/marches')));
v('les 3 marchés sont en cartes', d.querySelectorAll('.proj-carte').length === 3);
v('l\'ancien tableau a bien disparu', d.querySelector('#corps') === null);
v('le nombre affiché est rappelé',
  txt(d.getElementById('compte-affiche')).includes('3 marchés'));

const c1 = carte('m1');
v('la carte porte le numéro du marché', txt(c1).includes('MA-2026-0001'));
v('la carte porte l\'objet', txt(c1).includes('Ndioum'));
v('la carte porte le type', txt(c1).includes('Travaux'));
v('la carte porte l\'attributaire', txt(c1).includes('SOGEA'));
v('la carte porte l\'avancement en étapes', txt(c1).includes('3/8'));
v('la carte a une jauge d\'avancement', c1.querySelector('.barre > div') !== null);
// Le montant affiché est celui qui ENGAGE : attribué + avenants approuvés.
v('la carte montre le montant courant', txt(c1).includes('24000000'));
v('la carte signale les avenants', txt(c1).includes('2 avenant'));
v('la carte signale les réserves ouvertes', txt(c1).includes('Réserves ouvertes'));

// Un marché sans attributaire ne doit pas afficher un vide muet.
v('un marché non attribué le dit clairement',
  txt(carte('m3')).includes('Pas encore attribué'));
// Un marché annulé ne propose plus le bouton d'annulation.
v('un marché annulé n\'a plus de bouton « annuler »',
  carte('m3').querySelector('[data-annuler]') === null);
v('un marché en cours a le bouton « annuler »',
  c1.querySelector('[data-annuler]') !== null);

// --- Retard -----------------------------------------------------------------
v('le retard est signalé sur la carte', txt(c1).includes('Retard 5 j'));
const past = c1.querySelector('.retard-info');
v('la pastille « i » est présente', !!past);
// Info-bulle maison (data-tip), pas le `title` du navigateur.
v('l\'explication passe par l\'info-bulle maison', past.hasAttribute('data-tip'));
v('l\'explication dit que rien n\'est bloqué',
  past.getAttribute('data-tip').includes("Rien n'est bloqué"));
v('un marché à l\'heure n\'a pas de pastille',
  carte('m2').querySelector('.retard-info') === null);

// --- Sélection multiple -----------------------------------------------------
const barre = d.getElementById('barre-lot');
v('la barre de lot est masquée au départ', !visible(barre));
const coche = id => carte(id).querySelector('.coche');
coche('m1').checked = true;
coche('m1').dispatchEvent(new w.Event('change', { bubbles: true }));
await pause();
v('cocher une carte fait apparaître la barre de lot', visible(barre));
v('le compte est au singulier pour un seul marché',
  txt(d.getElementById('lot-compte')) === '1 marché sélectionné');
// La carte cochée doit se distinguer, sinon on ne sait plus sur quoi on agit.
v('la carte cochée est marquée', carte('m1').classList.contains('choisie'));
v('les autres cartes ne le sont pas', !carte('m2').classList.contains('choisie'));

d.getElementById('coche-tout').checked = true;
d.getElementById('coche-tout').dispatchEvent(new w.Event('change'));
await pause();
v('« tout sélectionner » coche les 3 cartes',
  d.querySelectorAll('.coche:checked').length === 3);
v('le compte passe au pluriel',
  txt(d.getElementById('lot-compte')).includes('3 marchés'));

// Traitement par lot : la bonne route, avec les bons identifiants.
d.querySelector('[data-lot="suspendu"]').click();
await pause(200);
const lot = appels.find(a => a.chemin === '/api/marches/lot/statut');
v('le traitement par lot appelle la bonne route', lot !== undefined);
v('le lot porte le statut demandé', lot && lot.body.statut === 'suspendu');
v('le lot porte les 3 identifiants', lot && lot.body.ids.length === 3);

// --- Clic sur la carte ------------------------------------------------------
// ⚠️ Le piège de la carte cliquable : la case à cocher et les boutons ne
// doivent PAS déclencher l'ouverture du marché.
let alle = null;
d.defaultView.location.href = 'http://localhost:1704/marches.html';
const carteM2 = carte('m2');
carteM2.querySelector('.coche').click();
await pause();
v('cliquer la case à cocher n\'ouvre pas le marché',
  d.defaultView.location.pathname.endsWith('marches.html'));

// Le bouton « annuler » ouvre la modale de motif, il n'ouvre pas le marché.
// ⚠️ On RE-cherche la carte : le traitement par lot ci-dessus a rechargé la
// liste, donc `c1` pointe sur un nœud détaché du document et son clic ne
// remonterait plus jusqu'au gestionnaire.
carte('m1').querySelector('[data-annuler]').click();
await pause();
v('« annuler » ouvre la demande de motif',
  !d.getElementById('modal-motif').hidden);
v('le motif est vidé à l\'ouverture', d.getElementById('motif-texte').value === '');

d.getElementById('modal-motif').hidden = true;

// --- CRUD complet -----------------------------------------------------------
// Standard du projet : chaque module offre créer / lire / MODIFIER / SUPPRIMER.
// La liste n'avait que « créer » et « annuler » — c'était le manque signalé.
v('chaque carte a un bouton modifier',
  [...d.querySelectorAll('.proj-carte')].every(c => c.querySelector('[data-modifier]')));
v('chaque carte a un bouton supprimer',
  [...d.querySelectorAll('.proj-carte')].every(c => c.querySelector('[data-supprimer]')));

// Modifier : la modale se rouvre PRÉ-REMPLIE.
carte('m1').querySelector('[data-modifier]').click();
await pause(150);
v('modifier ouvre la modale', !d.getElementById('modal-marche').hidden);
v('la modale est titrée « Modifier »',
  txt(d.getElementById('titre-modale')).startsWith('Modifier'));
v('l\'objet est pré-rempli',
  d.getElementById('m-objet').value === 'Construction du forage de Ndioum');
v('le montant est pré-rempli', Number(d.getElementById('m-estime').value) === 20000000);
v('le type est pré-rempli', d.getElementById('m-type').value === 'mt-travaux');
// ⚠️ On ne réécrit pas la procédure d'un marché lancé : le volet des étapes
// disparaît et le type se fige.
v('le volet des étapes est masqué en modification',
  !visible(d.getElementById('volet-etapes')));
v('le type ne se change pas sur un marché lancé',
  d.getElementById('m-type').disabled);

d.getElementById('m-objet').value = 'Forage de Ndioum (tranche 2)';
d.getElementById('btn-enregistrer').click();
await pause(200);
const maj = appels.find(a => a.chemin === '/api/marches/m1' && a.method === 'PUT');
v('enregistrer envoie un PUT', maj !== undefined);
v('le PUT porte le nouvel objet', maj && maj.body.objet === 'Forage de Ndioum (tranche 2)');
// Aucune étape envoyée : le marché garde les siennes.
v('le PUT n\'envoie aucune étape', maj && maj.body.etapes.length === 0);
v('la modale se referme', d.getElementById('modal-marche').hidden);

// Créer à nouveau : la modale doit être REMISE À NEUF (piège classique d'une
// modale partagée qui garde l'état précédent).
d.getElementById('btn-nouveau').click();
await pause(150);
v('« nouveau » vide l\'objet', d.getElementById('m-objet').value === '');
v('« nouveau » réaffiche le volet des étapes',
  visible(d.getElementById('volet-etapes')));
v('« nouveau » réactive le choix du type', !d.getElementById('m-type').disabled);
d.getElementById('modal-marche').hidden = true;

// Supprimer une carte.
carte('m2').querySelector('[data-supprimer]').click();
await pause(200);
const del = appels.find(a => a.chemin === '/api/marches/m2' && a.method === 'DELETE');
v('supprimer envoie un DELETE', del !== undefined);

// Suppression groupée.
d.getElementById('coche-tout').checked = true;
d.getElementById('coche-tout').dispatchEvent(new w.Event('change'));
await pause();
d.getElementById('btn-lot-supprimer').click();
await pause(200);
const delLot = appels.find(a => a.chemin === '/api/marches/lot/supprimer');
v('la suppression groupée appelle la bonne route', delLot !== undefined);
v('la suppression groupée porte les identifiants',
  delLot && delLot.body.ids.length === 3);

// --- Alertes de cohérence sur les cartes ------------------------------------
// Elles ne doivent plus être réservées au détail : on doit voir depuis la liste
// ce qui cloche, sans ouvrir chaque dossier.
const alerte = carte('m1').querySelector('.badge.danger[data-tip]');
v('les alertes remontent sur la carte', !!alerte);
v('l\'alerte annonce le nombre de points',
  alerte && txt(alerte).includes('point(s) à vérifier'));
v('le détail des alertes est dans l\'info-bulle',
  alerte && alerte.getAttribute('data-tip').includes('étape'));
v('un marché sans alerte n\'affiche pas la pastille',
  carte('m3').querySelector('.badge.danger[data-tip]') === null);

// --- Onglets de filtre ------------------------------------------------------
// Ils filtrent COTE CLIENT : c'est ce qui permet d'afficher un compteur juste
// sur chaque onglet sans interroger le serveur six fois.
const onglets = [...d.querySelectorAll('#tabs-statut .tab')];
v('onglets : les 6 filtres sont presents', onglets.length === 6);
v('onglets : chaque onglet porte son compteur',
  onglets.every(t => t.querySelector('.cpt')));
const cpt = f => onglets.find(t => t.dataset.filtre === f).querySelector('.cpt').textContent;
v('onglets : le compteur « Tous » vaut le nombre de marches', cpt('') === '3');
v('onglets : le compteur « En retard » ne compte que les retards', cpt('retard') === '1');
v('onglets : le compteur « Annules » est juste', cpt('annule') === '1');

// Filtrer reduit reellement la liste affichee.
onglets.find(t => t.dataset.filtre === 'retard').click();
await pause(150);
v('onglets : filtrer « En retard » ne laisse que le marche en retard',
  d.querySelectorAll('.proj-carte').length === 1
  && carte('m1') !== null);
v("onglets : l'onglet actif est marque",
  onglets.find(t => t.dataset.filtre === 'retard').classList.contains('active'));
v('onglets : le compte affiche suit le filtre',
  txt(d.getElementById('compte-affiche')).startsWith('1 marché'));
// Un filtre sans resultat doit le DIRE, pas laisser un vide muet.
onglets.find(t => t.dataset.filtre === 'suspendu').click();
await pause(150);
v('onglets : un filtre sans resultat affiche un message',
  txt(d.getElementById('grille')).includes('Aucun marché'));
onglets.find(t => t.dataset.filtre === '').click();
await pause(150);
v('onglets : revenir a « Tous » reaffiche tout',
  d.querySelectorAll('.proj-carte').length === 3);

// Le panneau blanc soude aux onglets delimite la zone (motif `.tabs-folder + .card`).
const panneau = d.querySelector('#tabs-statut + .card');
v('onglets : un panneau blanc suit les onglets', panneau !== null);
v('onglets : la grille de cartes est DANS le panneau',
  panneau && panneau.contains(d.getElementById('grille')));
v('onglets : la barre de selection est dans le panneau',
  panneau && panneau.contains(d.getElementById('coche-tout')));

// L'export du suivi reste disponible (le Kanban a ete retire, pas l'export).
v('export : le bouton du suivi est present', d.getElementById('btn-export-suivi') !== null);
d.getElementById('btn-export-suivi').click();
await pause(200);
v('export : il appelle la bonne route',
  appels.some(a => a.chemin === '/api/marches/export-suivi' && a.method === 'POST'));

// La vue Kanban a bien ete retiree.
v('la vue Kanban n\'existe plus', d.querySelector('.kb, .bascule-vue, #phases') === null);

// --- Aide -------------------------------------------------------------------
const aide = txt(d.querySelector('.aide'));
v('l\'aide explique les cartes', aide.includes('carte'));
v('l\'aide explique la sélection multiple', aide.includes('sélectionner plusieurs'));
v('l\'aide explique modifier', aide.includes('Modifier'));
v('l\'aide explique supprimer et le renvoi vers l\'annulation',
  aide.includes('Supprimer') && aide.includes('annule'));

console.log(`\nmarches-cartes : ${ok.length}/${ok.length + ko.length} tests passés`);
if (ko.length) {
  console.log('\nÉCHECS :');
  ko.forEach(n => console.log('  ✗ ' + n));
  if (erreurs.length) { console.log('\nErreurs JS :'); erreurs.forEach(e => console.log('  ' + e)); }
  process.exit(1);
}
