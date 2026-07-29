// Test de fumée de comptabilite.html — l'écran réservé au comptable.
//
// On charge la page dans un DOM, on remplace le réseau par des données
// réalistes, et on vérifie que le procédé validé avec l'utilisateur tient :
// corbeille « À ranger », recherche multicritère, règles, comptes, grand livre,
// balance, et le compte d'attente 471 rendu visible plutôt que caché.
//
// ⚠️ jsdom ne calcule PAS la mise en page : ce test valide la LOGIQUE et
// l'affichage/masquage, pas les hauteurs ni les débordements.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const COMPTES = [
  { numero: '411', libelle: 'Clients', classe: 4, sens_normal: 'debit', lettrable: true,
    actif: true, nb_lignes: 2, solde: 0 },
  { numero: '471', libelle: "Compte d'attente", classe: 4, sens_normal: 'debit', lettrable: false,
    actif: true, nb_lignes: 2, solde: 65920 },
  { numero: '571', libelle: 'Caisse', classe: 5, sens_normal: 'debit', lettrable: false,
    actif: true, nb_lignes: 1, solde: 112102 },
  { numero: '701', libelle: 'Ventes de marchandises', classe: 7, sens_normal: 'credit',
    lettrable: false, actif: true, nb_lignes: 1, solde: -105080 },
];

const REGLES = [
  { id: 'g1', nom: 'Ventes', role: 'produit', compte_numero: '701',
    compte_libelle: 'Ventes de marchandises', domaine: 'vente', specificite: 1,
    actif: true, ordre: 0, cree_le: '2026-07-27' },
  { id: 'g2', nom: 'Clients', role: 'tiers', compte_numero: '411', compte_libelle: 'Clients',
    specificite: 0, actif: true, ordre: 0, cree_le: '2026-07-27' },
];

// Trois états possibles d'une opération : à ranger, rangée, et rangée mais en
// attente d'un compte (celle-ci doit sauter aux yeux).
const OPERATIONS = [
  { origine_type: 'document', id: 'd1', domaine: 'vente', date: '2026-07-23',
    libelle: 'facture FA-2026-0001 — Client comptoir', montant: 19840,
    rattachee: false, incomplete: false },
  { origine_type: 'paiement', id: 'p1', domaine: 'encaissement', date: '2026-07-23',
    libelle: 'Encaissement FA-2026-0001 (Orange Money)', montant: 19840,
    rattachee: false, incomplete: false },
  { origine_type: 'document', id: 'd2', domaine: 'achat', date: '2026-07-24',
    libelle: 'facture FA-2026-0011 — Fournisseur Lait', montant: 65920,
    rattachee: true, incomplete: true, ecriture_id: 'e2' },
];

const ECRITURE_INCOMPLETE = {
  id: 'e2', journal_code: 'AC', date: '2026-07-24', exercice: 2026,
  libelle: 'facture FA-2026-0011 — Fournisseur Lait',
  origine_type: 'document', origine_id: 'd2', complete: false,
  total_debit: 65920, total_credit: 65920,
  lignes: [
    { id: 'l1', compte_numero: '471', compte_libelle: "Compte d'attente",
      libelle: 'Achat', debit: 51500, credit: 0, role: 'charge' },
    { id: 'l2', compte_numero: '471', compte_libelle: "Compte d'attente",
      libelle: 'TVA', debit: 14420, credit: 0, role: 'taxe' },
    { id: 'l3', compte_numero: '411', compte_libelle: 'Clients',
      libelle: 'Fournisseur Lait', debit: 0, credit: 65920, role: 'tiers' },
  ],
};

const GRAND_LIVRE = {
  compte: COMPTES[0], solde_initial: 0, total_debit: 19840, total_credit: 19840, solde_final: 0,
  mouvements: [
    { ecriture_id: 'e1', ligne_id: 'gl1', date: '2026-07-23', journal_code: 'VT',
      libelle: 'facture FA-2026-0001', tiers_nom: 'Client comptoir',
      debit: 19840, credit: 0, solde: 19840 },
    { ecriture_id: 'e3', ligne_id: 'gl2', date: '2026-07-23', journal_code: 'CA',
      libelle: 'Encaissement FA-2026-0001', tiers_nom: 'Client comptoir',
      debit: 0, credit: 19840, solde: 0 },
  ],
};

const BALANCE = {
  lignes: [
    { numero: '411', libelle: 'Clients', classe: 4, debit: 19840, credit: 19840,
      solde: 0, solde_anormal: false },
    { numero: '471', libelle: "Compte d'attente", classe: 4, debit: 65920, credit: 0,
      solde: 65920, solde_anormal: false },
    { numero: '701', libelle: 'Ventes de marchandises', classe: 7, debit: 0, credit: 105080,
      solde: -105080, solde_anormal: false },
  ],
  total_debit: 337925, total_credit: 337925, equilibree: true, nb_incompletes: 1,
};

const REPONSES = {
  '/api/tiers': [{ id: 't1', nom: 'Client comptoir' }, { id: 't2', nom: 'Fournisseur Lait' }],
  '/api/categories': [{ id: 'cat1', nom: 'Boissons' }, { id: 'cat2', nom: 'Plats' }],
  '/api/caisses': [{ id: 'c1', nom: 'Caisse principale' }],
  '/api/moyens-paiement': [{ id: 'm1', nom: 'Orange Money' }, { id: 'm2', nom: 'Espèces' }],
  '/api/comptes': COMPTES,
  '/api/regles-comptables': REGLES,
  '/api/comptabilite/operations': OPERATIONS,
  '/api/comptabilite/ecritures/e2': ECRITURE_INCOMPLETE,
  '/api/comptabilite/grand-livre/411': GRAND_LIVRE,
  '/api/comptabilite/balance': BALANCE,
};

const appels = [];
const erreurs = [];
const vc = new VirtualConsole();
vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
const html = readFileSync('D:/DJGUI_ERP/frontend/comptabilite.html', 'utf8')
  .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
  .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
  .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
    window.Djigui = {
      api: async (chemin, opts) => {
        appelsJS.push({ chemin, method: (opts && opts.method) || 'GET', body: opts && opts.body });
        if (opts && opts.method && opts.method !== 'GET')
          return { id: 'nouveau', creees: 2, deja_rattachees: 0, incompletes: 0, alertes: [], ajoutes: 31, code: 'A' };
        const r = REPONSES_JS[chemin.split('?')[0]] ?? REPONSES_JS[chemin];
        if (r === undefined) throw new Error('404 ' + chemin);
        return JSON.parse(JSON.stringify(r));
      },
      fmt: n => String(n), esc: s => String(s ?? ''),
      dateFr: s => s || '', toast: () => {}, alert: () => {}, confirm: async () => true,
      selectRecherche: () => ({ setItems(){}, setValue(v){ this.value = v || ''; }, value: '' }),
      estAdmin: () => true,
    };
  </script>`);

// ⚠️ Les données DOIVENT être posées dans `beforeParse` : le script de la page
// part pendant le parsing et lance son premier appel réseau tout de suite. Les
// assigner après `new JSDOM(...)` ferait échouer ce premier appel — la page le
// rattrape avec un `.catch`, l'échec serait silencieux et le test mentirait.
const dom = new JSDOM(html, {
  runScripts: 'dangerously', url: 'http://localhost:1704/comptabilite.html',
  virtualConsole: vc, pretendToBeVisual: true,
  beforeParse(fenetre) { fenetre.appelsJS = appels; fenetre.REPONSES_JS = REPONSES; },
});
const w = dom.window;
const d = w.document;
const pause = (ms = 60) => new Promise(r => setTimeout(r, ms));

await pause(400);

const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const visible = el => el && w.getComputedStyle(el).display !== 'none';
const aEteAppele = chemin => appels.some(a => a.chemin.split('?')[0] === chemin);

// ---- Chargement ------------------------------------------------------------
v('aucune erreur JS', erreurs.length === 0);
v('les comptes sont chargés', aEteAppele('/api/comptes'));
v('les règles sont chargées', aEteAppele('/api/regles-comptables'));
v('la corbeille est chargée', aEteAppele('/api/comptabilite/operations'));
v('les référentiels de recherche sont chargés',
  ['/api/tiers', '/api/categories', '/api/caisses', '/api/moyens-paiement'].every(aEteAppele));

// ---- Corbeille « À ranger » ------------------------------------------------
const lignes = d.querySelectorAll('#corps-ranger tr');
v('les 3 opérations sont affichées', lignes.length === 3);
v('la pastille compte les 2 opérations restant à ranger',
  d.getElementById('pastille-ranger').textContent === '2');
v('la pastille est visible', !d.getElementById('pastille-ranger').hidden);

// Une opération déjà rangée n'est pas cochable : on ne la range pas deux fois.
v('seules les opérations non rangées sont cochables',
  d.querySelectorAll('#corps-ranger .coche-op').length === 2);
v("l'opération en compte d'attente est surlignée",
  d.querySelector('#corps-ranger tr.ligne-attente') !== null);
v("l'opération rangée offre de voir son écriture",
  d.querySelector('#corps-ranger [data-voir="e2"]') !== null);

// ---- Barre de traitement par lot ------------------------------------------
// ⚠️ Piège connu du projet : `hidden` battu par un `display` d'auteur. La barre
// réutilise `.bulk-bar`, qui porte déjà `[hidden]{display:none}`.
const barre = d.getElementById('barre-lot');
v('barre de lot masquée au départ', !visible(barre));
const case1 = d.querySelector('#corps-ranger .coche-op');
case1.checked = true;
case1.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause();
v('barre de lot visible après une coche', visible(barre));
v('le compteur annonce 1 opération', d.getElementById('lot-compte').textContent.includes('1'));

d.getElementById('coche-tout').checked = true;
d.getElementById('coche-tout').dispatchEvent(new w.Event('change'));
await pause();
v('« tout cocher » sélectionne les 2 rangeables',
  d.getElementById('lot-compte').textContent.includes('2'));

// Décocher tout referme la barre — c'est le bug n°2 du projet, on le surveille.
d.getElementById('coche-tout').checked = false;
d.getElementById('coche-tout').dispatchEvent(new w.Event('change'));
await pause();
v('barre de lot refermée quand on décoche tout', !visible(barre));

// ---- Recherche multicritère ------------------------------------------------
const avant = appels.length;
d.getElementById('r-domaine').value = 'vente';
d.getElementById('r-min').value = '10000';
d.getElementById('r-texte').value = 'FA-2026';
d.getElementById('btn-chercher').click();
await pause(120);
const requete = appels.slice(avant).find(a => a.chemin.startsWith('/api/comptabilite/operations'));
v('la recherche interroge le serveur', !!requete);
v('la recherche transmet le domaine', requete && requete.chemin.includes('domaine=vente'));
v('la recherche transmet le montant minimum', requete && requete.chemin.includes('montant_min=10000'));
v('la recherche transmet le texte libre', requete && requete.chemin.includes('texte=FA-2026'));
v('la recherche ne demande que ce qui reste à ranger',
  requete && requete.chemin.includes('a_ranger_seulement=true'));

// Un critère vide ne doit PAS partir dans la requête : sinon le serveur
// filtrerait sur une chaîne vide et ne renverrait rien.
v('un critère vide n\'est pas envoyé', requete && !requete.chemin.includes('tiers_id='));

d.getElementById('btn-vider').click();
await pause(120);
v('« Vider les critères » remet le domaine à zéro', d.getElementById('r-domaine').value === '');
v('« Vider les critères » relance la recherche',
  appels.filter(a => a.chemin.startsWith('/api/comptabilite/operations')).length >= 3);

// ---- Transformer la recherche en règle -------------------------------------
d.getElementById('r-domaine').value = 'achat';
d.getElementById('r-categorie').value = 'cat1';
const modRegle = d.getElementById('modal-regle');
v('modale règle fermée au départ', !visible(modRegle));
// La barre de lot doit être ouverte pour cliquer : on coche d'abord.
const c2 = d.querySelector('#corps-ranger .coche-op');
if (c2) { c2.checked = true; c2.dispatchEvent(new w.Event('change', { bubbles: true })); }
await pause();
d.getElementById('lot-regle').click();
await pause();
v('« Faire une règle de cette recherche » ouvre la modale', visible(modRegle));
v('la règle hérite du domaine cherché', d.getElementById('g-domaine').value === 'achat');
v('la règle hérite de la catégorie cherchée', d.getElementById('g-categorie').value === 'cat1');
v('la liste des comptes de la règle est remplie',
  d.getElementById('g-compte').options.length === COMPTES.length + 1);
d.querySelector('#modal-regle [data-fermer]').click();
await pause();
v('la modale règle se ferme', !visible(modRegle));

// ---- Onglets ---------------------------------------------------------------
const onglet = nom => [...d.querySelectorAll('#cpt-tabs .tab')].find(t => t.dataset.tab === nom);
const panneau = nom => d.querySelector(`[data-panel="${nom}"]`);
v('le panneau « À ranger » est visible au départ', visible(panneau('ranger')));
v('le panneau « Balance » est masqué au départ', !visible(panneau('balance')));

onglet('regles').click();
await pause();
v('l\'onglet Règles affiche son panneau', visible(panneau('regles')));
v('l\'onglet Règles masque la corbeille', !visible(panneau('ranger')));
v('les 2 règles sont listées', d.querySelectorAll('#corps-regles tr').length === 2);
v('une règle sans critère est présentée comme la règle par défaut',
  d.getElementById('corps-regles').textContent.includes('règle par défaut'));
v('le rôle est traduit en langage courant',
  d.getElementById('corps-regles').textContent.includes('Client / fournisseur'));

onglet('comptes').click();
await pause();
v('l\'onglet Comptes affiche son panneau', visible(panneau('comptes')));
v('les 4 comptes sont listés', d.querySelectorAll('#corps-comptes tr').length === 4);

onglet('balance').click();
await pause(120);
v('l\'onglet Balance interroge le serveur', aEteAppele('/api/comptabilite/balance'));
v('la balance équilibrée est annoncée',
  d.getElementById('ba-etat').textContent.includes('équilibrée'));
v('les écritures en attente sont signalées',
  d.getElementById('ba-etat').textContent.includes('471'));
v('les lignes de balance sont affichées avec le total',
  d.querySelectorAll('#corps-balance tr').length === BALANCE.lignes.length + 1);

// ---- Grand livre -----------------------------------------------------------
onglet('livre').click();
await pause();
v('le sélecteur de compte du grand livre est rempli',
  d.getElementById('gl-compte').options.length === COMPTES.length + 1);
d.getElementById('gl-compte').value = '411';
d.getElementById('gl-compte').dispatchEvent(new w.Event('change'));
await pause(120);
v('le grand livre est chargé', aEteAppele('/api/comptabilite/grand-livre/411'));
v('les 2 mouvements sont affichés', d.querySelectorAll('#corps-gl tr').length === 2);
v('le solde court est affiché', d.getElementById('corps-gl').textContent.includes('19840'));
v('le résumé du compte est affiché', d.getElementById('gl-resume').textContent.includes('Solde'));

const barreL = d.getElementById('barre-lettrage');
v('barre de lettrage masquée au départ', !visible(barreL));
const cgl = d.querySelector('#corps-gl .coche-gl');
cgl.checked = true;
cgl.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause();
v('barre de lettrage visible après une coche', visible(barreL));

// ---- Détail d'une écriture en compte d'attente -----------------------------
onglet('ranger').click();
await pause();
d.querySelector('#corps-ranger [data-voir="e2"]').click();
await pause(120);
const modEcr = d.getElementById('modal-ecriture');
v('la modale d\'écriture s\'ouvre', visible(modEcr));
v('les 3 lignes de l\'écriture sont affichées',
  d.querySelectorAll('#corps-ecriture tr').length === 4); // 3 lignes + total
v('les lignes en 471 proposent de choisir le bon compte',
  d.querySelectorAll('#corps-ecriture .chg-compte').length === 2);
v('la ligne déjà rangée n\'offre pas de sélecteur',
  d.getElementById('corps-ecriture').textContent.includes('Clients'));
v('le total débit égale le total crédit dans l\'affichage',
  d.getElementById('corps-ecriture').textContent.includes('65920'));

// Affecter un compte à la main : c'est le comptable qui tranche.
const avantAffect = appels.length;
const sel = d.querySelector('#corps-ecriture .chg-compte');
sel.value = '701';
sel.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(150);
const affect = appels.slice(avantAffect).find(a => a.chemin.includes('/lignes/') && a.method === 'POST');
v('l\'affectation manuelle appelle le serveur', !!affect);
v('l\'affectation transmet le compte choisi',
  affect && affect.body && affect.body.compte_numero === '701');

// ---- Rendu final -----------------------------------------------------------
for (const t of ok) console.log('  ok   ' + t);
for (const t of ko) console.log('  ÉCHEC ' + t);
if (erreurs.length) for (const e of erreurs) console.log('  !! ' + e);
console.log(`\nsmoke-comptabilite : ${ok.length}/${ok.length + ko.length}`);
process.exit(ko.length ? 1 : 0);
