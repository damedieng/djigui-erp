// Test de fumée de production.html : on charge la page dans un DOM, on remplace
// le réseau par des données réalistes, et on vérifie que le script ne plante pas
// et que les commandes clés sont câblées.
//
// ⚠️ jsdom ne calcule PAS la mise en page : ce test valide la LOGIQUE et
// l'affichage/masquage, pas les hauteurs ni les débordements. Pour tout doute
// visuel, passer par Chrome (voir navigateur-projet.mjs).
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const ART = [
  { id: 'a-pain',   code: 'PAIN',   designation: 'Baguette', prix_achat: 0,   gere_stock: true },
  { id: 'a-farine', code: 'FARINE', designation: 'Farine',   prix_achat: 500, gere_stock: true },
  { id: 'a-sel',    code: 'SEL',    designation: 'Sel',      prix_achat: 200, gere_stock: true },
];
const RECETTE = {
  id: 'r1', article_id: 'a-pain', article_code: 'PAIN', article_designation: 'Baguette',
  nom: 'Pate a baguette', quantite_produite: 20, actif: true, cree_le: '2026-07-26',
  nb_composants: 2, cout_estime: 5290, cout_unitaire_estime: 264.5,
  composants: [
    { id: 'c1', article_id: 'a-farine', article_code: 'FARINE', article_designation: 'Farine',
      quantite: 10, perte_pct: 5, prix_achat: 500, ordre: 0 },
    { id: 'c2', article_id: 'a-sel', article_code: 'SEL', article_designation: 'Sel',
      quantite: 0.2, perte_pct: 0, prix_achat: 200, ordre: 1 },
  ],
  alertes: [],
};
// Trois ordres : un brouillon (modifiable, supprimable), un en cours, un terminé
// (figé, avec un écart de production à signaler).
const ORDRES = [
  { id: 'o1', numero: 'OF-2026-0001', article_produit_id: 'a-pain', article_code: 'PAIN',
    article_designation: 'Baguette', depot_id: 'd1', depot_nom: 'Boulangerie',
    quantite: 40, statut: 'brouillon', date: '2026-07-26', frais: 2000,
    cree_le: '2026-07-26', nb_composants: 2, cout_estime: 12500, alertes: [],
    nomenclature_id: 'r1', nomenclature_nom: 'Pate a baguette',
    composants: [
      { id: 'pc1', article_id: 'a-farine', article_code: 'FARINE', article_designation: 'Farine',
        quantite_prevue: 21, cout_unitaire: 500, cout: 10500, stock_dispo: 100, ordre: 0 },
      { id: 'pc2', article_id: 'a-sel', article_code: 'SEL', article_designation: 'Sel',
        quantite_prevue: 0.4, cout_unitaire: 200, cout: 80, stock_dispo: 5, ordre: 1 },
    ] },
  { id: 'o2', numero: 'OF-2026-0002', article_produit_id: 'a-pain', article_code: 'PAIN',
    article_designation: 'Baguette', depot_id: 'd1', depot_nom: 'Boulangerie',
    quantite: 10, statut: 'en_cours', date: '2026-07-26', frais: 0,
    cree_le: '2026-07-26', nb_composants: 0, cout_estime: 0, alertes: [], composants: [] },
  { id: 'o3', numero: 'OF-2026-0003', article_produit_id: 'a-pain', article_code: 'PAIN',
    article_designation: 'Baguette', depot_id: 'd1', depot_nom: 'Boulangerie',
    quantite: 40, quantite_produite: 38, ecart_quantite: -2, statut: 'termine',
    date: '2026-07-25', frais: 2000, cout_total: 13080, cout_unitaire: 344.21,
    cree_le: '2026-07-25', cloture_le: '2026-07-25', nb_composants: 2, cout_estime: 13080,
    alertes: ['Production inférieure au prévu : 2 unité(s) de moins que les 40 annoncées.'],
    composants: [] },
];

const REPONSES = {
  '/api/articles': ART,
  '/api/depots': [{ id: 'd1', nom: 'Boulangerie', par_defaut: true }],
  '/api/stock/depot/d1': [
    { article_id: 'a-farine', code: 'FARINE', designation: 'Farine', stock: 100 },
    { article_id: 'a-sel', code: 'SEL', designation: 'Sel', stock: 5 },
    { article_id: 'a-pain', code: 'PAIN', designation: 'Baguette', stock: 0 },
  ],
  '/api/nomenclatures': [RECETTE],
  '/api/nomenclatures/r1': RECETTE,
  '/api/ordres-production': ORDRES,
  '/api/ordres-production/o1': ORDRES[0],
  '/api/ordres-production/o3': ORDRES[2],
};

const appels = [];
const erreurs = [];
const vc = new VirtualConsole();
vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
const html = readFileSync('D:/DJGUI_ERP/frontend/production.html', 'utf8')
  .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
  .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
  .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
    window.Djigui = {
      api: async (chemin, opts) => {
        appelsJS.push({ chemin, method: (opts && opts.method) || 'GET' });
        if (opts && opts.method && opts.method !== 'GET') return { id: 'nouveau', numero: 'OF-2026-0009', alertes: [] };
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
// s'exécute pendant le parsing et lance son premier appel réseau tout de suite.
// Les assigner après `new JSDOM(...)` ferait échouer ce premier appel (la page le
// rattrape avec un `.catch`, donc l'échec serait silencieux et le test mentirait).
const dom = new JSDOM(html, {
  runScripts: 'dangerously', url: 'http://localhost:1704/production.html',
  virtualConsole: vc, pretendToBeVisual: true,
  beforeParse(fenetre) { fenetre.appelsJS = appels; fenetre.REPONSES_JS = REPONSES; },
});
const w = dom.window;

await new Promise(r => setTimeout(r, 400));

const d = w.document;
const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const visible = el => el && w.getComputedStyle(el).display !== 'none';

v('aucune erreur JS', erreurs.length === 0);
v('les ordres sont chargés', appels.some(a => a.chemin.startsWith('/api/ordres-production')));
v('les recettes sont chargées', appels.some(a => a.chemin === '/api/nomenclatures'));
v('3 ordres affichés', d.querySelectorAll('#corps-ordres tr[data-id]').length === 3);
v('1 recette affichée', d.querySelectorAll('#corps-recettes tr[data-id]').length === 1);

// Un ordre terminé n'offre ni clôture, ni annulation, ni suppression : c'est de
// l'historique. Le brouillon, lui, offre tout.
const ligne = id => d.querySelector(`#corps-ordres tr[data-id="${id}"]`);
const actions = id => [...ligne(id).querySelectorAll('.icon-act')].map(b => b.dataset.act);
v('le brouillon propose clôturer/annuler/supprimer',
  ['cloturer', 'annuler', 'supprimer'].every(a => actions('o1').includes(a)));
v('l\'ordre terminé ne propose que « voir »',
  actions('o3').length === 1 && actions('o3')[0] === 'voir');
v('l\'écart de production est affiché', ligne('o3').textContent.includes('-2'));

// Barre de traitement par lot : masquée tant que rien n'est coché.
const barre = d.getElementById('barre-lot');
v('barre de lot masquée au départ', !visible(barre));
const case1 = ligne('o1').querySelector('.coche');
case1.checked = true;
case1.dispatchEvent(new w.Event('change', { bubbles: true }));
await new Promise(r => setTimeout(r, 50));
v('barre de lot visible après une coche', visible(barre));
v('le compteur annonce 1 ordre', d.getElementById('lot-compte').textContent.includes('1'));
d.getElementById('coche-tout').checked = true;
d.getElementById('coche-tout').dispatchEvent(new w.Event('change'));
await new Promise(r => setTimeout(r, 50));
v('« tout cocher » sélectionne les 3', d.getElementById('lot-compte').textContent.includes('3'));

// Modales : fermées au départ, ouvertes sur commande.
const modOrdre = d.getElementById('modal-ordre');
v('modale ordre fermée au départ', !visible(modOrdre));
d.getElementById('btn-nouvel-ordre').click();
await new Promise(r => setTimeout(r, 50));
v('« Nouvel ordre » ouvre la modale', visible(modOrdre));
v('la date du jour est pré-remplie', !!d.getElementById('ordre-date').value);
v('le magasin par défaut est sélectionné', d.getElementById('ordre-depot').value === 'd1');
v('les recettes actives sont proposées',
  [...d.getElementById('ordre-recette').options].some(o => o.value === 'r1'));

// Choisir une recette recopie les composants au prorata de la quantité.
d.getElementById('ordre-qte').value = '40';
d.getElementById('ordre-recette').value = 'r1';
d.getElementById('ordre-recette').dispatchEvent(new w.Event('change'));
await new Promise(r => setTimeout(r, 120));
const lignesComp = [...d.querySelectorAll('#ordre-composants tr[data-i]')];
v('la recette a rempli 2 composants', lignesComp.length === 2);
const qtes = lignesComp.map(tr => Number(tr.querySelector('.comp-qte').value));
v('prorata ×2 + 5 % de perte sur la farine → 21', qtes.includes(21));
v('sel proratisé → 0,4', qtes.includes(0.4));
v('le coût estimé est calculé', d.getElementById('ordre-cout').value !== '—');

// Ajout / retrait d'un composant à la main.
d.getElementById('btn-ajout-composant').click();
await new Promise(r => setTimeout(r, 50));
v('ajout d\'un composant', d.querySelectorAll('#ordre-composants tr[data-i]').length === 3);
d.querySelector('#ordre-composants [data-retirer]').click();
await new Promise(r => setTimeout(r, 50));
v('retrait d\'un composant', d.querySelectorAll('#ordre-composants tr[data-i]').length === 2);
d.getElementById('ordre-fermer').click();
await new Promise(r => setTimeout(r, 50));
v('la modale ordre se ferme VRAIMENT', !visible(modOrdre));

// Ouverture d'un ordre terminé : lecture seule, bouton Enregistrer masqué.
ligne('o3').querySelector('[data-act="voir"]').click();
await new Promise(r => setTimeout(r, 150));
v('l\'ordre terminé s\'ouvre', visible(modOrdre));
v('son alerte d\'écart est affichée', d.getElementById('ordre-alertes').textContent.includes('moins que'));
v('le bouton Enregistrer est masqué (lecture seule)',
  d.querySelector('#form-ordre button[type=submit]').hidden === true);
v('les champs sont désactivés', d.getElementById('ordre-qte').disabled === true);
d.getElementById('ordre-fermer').click();
await new Promise(r => setTimeout(r, 50));
v('les champs sont réactivés à la fermeture', d.getElementById('ordre-qte').disabled === false);

// Clôture : la consommation réelle est pré-remplie avec le prévu, et le coût suit.
const modClo = d.getElementById('modal-cloture');
v('modale clôture fermée au départ', !visible(modClo));
ligne('o1').querySelector('[data-act="cloturer"]').click();
await new Promise(r => setTimeout(r, 150));
v('la clôture s\'ouvre', visible(modClo));
v('quantité produite pré-remplie avec le prévu', d.getElementById('clo-qte').value === '40');
v('2 composants à confirmer', d.querySelectorAll('#clo-composants tr[data-article]').length === 2);
v('coût total calculé', d.getElementById('clo-cout-total').textContent !== '—');
v('report du prix de revient coché par défaut', d.getElementById('clo-maj-prix').checked);
const conso = d.querySelector('#clo-composants .clo-conso');
conso.value = '30';
conso.dispatchEvent(new w.Event('input', { bubbles: true }));
await new Promise(r => setTimeout(r, 50));
// 30 kg × 500 + 0,4 kg × 200 + 2000 de frais = 17080
v('le coût suit la consommation saisie', d.getElementById('clo-cout-total').textContent.includes('17080'));
d.getElementById('clo-fermer').click();
await new Promise(r => setTimeout(r, 50));
v('la modale clôture se ferme VRAIMENT', !visible(modClo));

// Annulation : le motif passe par une modale (obligatoire), pas un prompt natif.
const modMotif = d.getElementById('modal-motif');
v('modale motif fermée au départ', !visible(modMotif));
ligne('o1').querySelector('[data-act="annuler"]').click();
await new Promise(r => setTimeout(r, 50));
v('« Annuler » demande un motif', visible(modMotif));
v('le motif est obligatoire', d.getElementById('motif-texte').required === true);
d.getElementById('motif-fermer').click();
await new Promise(r => setTimeout(r, 50));
v('la modale motif se ferme VRAIMENT', !visible(modMotif));

// Onglets et recettes.
const panRecettes = d.querySelector('[data-panel="recettes"]');
v('l\'onglet Recettes est masqué au départ', panRecettes.hidden === true);
d.querySelector('#prod-tabs .tab[data-tab="recettes"]').click();
await new Promise(r => setTimeout(r, 50));
v('le clic bascule sur Recettes', panRecettes.hidden === false);
const modRec = d.getElementById('modal-recette');
d.getElementById('btn-nouvelle-recette').click();
await new Promise(r => setTimeout(r, 50));
v('« Nouvelle recette » ouvre la modale', visible(modRec));
d.getElementById('btn-ajout-comp-rec').click();
await new Promise(r => setTimeout(r, 50));
v('ajout d\'un composant de recette', d.querySelectorAll('#rec-composants tr[data-i]').length === 1);
d.getElementById('rec-fermer').click();
await new Promise(r => setTimeout(r, 50));
v('la modale recette se ferme VRAIMENT', !visible(modRec));

// Chaque écran doit expliquer son comportement en langage simple.
v('deux sections d\'aide (une par onglet)', d.querySelectorAll('.aide').length === 2);

console.log('--- RÉSULTATS ---');
ok.forEach(x => console.log('  OK   ' + x));
ko.forEach(x => console.log('  KO   ' + x));
if (erreurs.length) erreurs.forEach(e => console.log('  !! ' + e));
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
process.exit(ko.length ? 1 : 0);
