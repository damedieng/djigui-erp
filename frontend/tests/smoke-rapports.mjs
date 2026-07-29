// Test de fumée de rapports.html et completer-prix.html.
//
// Ces deux écrans partagent un enjeu : **ne jamais présenter un chiffre douteux
// comme s'il était sûr**. Une marge calculée sur un prix d'achat estimé doit se
// signaler d'elle-même. C'est ce qu'on vérifie ici en priorité.
//
// ⚠️ jsdom ne calcule PAS la mise en page : logique et affichage/masquage
// seulement, pas les hauteurs ni les débordements.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

const JOURNAL_VENTES = {
  total_ht: 105080, total_tva: 29422, total_ttc: 134502,
  total_regle: 134502, total_reste: 0,
  lignes: [
    { document_id: 'd1', numero: 'FA-2026-0001', date: '2026-07-23', type_document: 'facture',
      tiers_nom: 'Client comptoir', total_ht: 16814, total_tva: 3026, total_ttc: 19840,
      regle: 19840, reste: 0, statut: 'valide' },
    { document_id: 'd9', numero: 'FA-2026-0012', date: '2026-07-24', type_document: 'facture',
      tiers_nom: 'Client comptoir', total_ht: 1271, total_tva: 229, total_ttc: 1500,
      regle: 0, reste: 0, statut: 'annule' },
  ],
};

const JOURNAL_ACHATS = {
  total_ht: 51500, total_tva: 14420, total_ttc: 65920,
  total_regle: 0, total_reste: 65920,
  lignes: [
    { document_id: 'd2', numero: 'FA-2026-0011', date: '2026-07-24', type_document: 'facture',
      tiers_nom: 'Fournisseur Lait', total_ht: 51500, total_tva: 14420, total_ttc: 65920,
      regle: 0, reste: 65920, statut: 'valide' },
  ],
};

// Deux articles : l'un au prix d'achat sûr, l'autre estimé — c'est le second
// qui doit déclencher l'avertissement.
const MARGES = {
  total_ca: 20000, total_cout: 9000, total_marge: 11000, nb_cout_incertain: 1,
  lignes: [
    { article_id: 'a1', code: 'RIZ', designation: 'Riz parfumé', categorie_nom: 'Épicerie',
      quantite_vendue: 20, ca_ht: 12000, cout: 6000, marge: 6000, marge_pct: 50,
      cout_incertain: false },
    { article_id: 'a2', code: 'YASSA', designation: 'Poulet yassa', categorie_nom: 'Plats',
      quantite_vendue: 4, ca_ht: 8000, cout: 3000, marge: 5000, marge_pct: 63,
      cout_incertain: true },
  ],
};

const STOCK = {
  valeur_totale: 50000, nb_ruptures: 1, nb_alertes: 1, nb_valeur_incertaine: 1,
  lignes: [
    { article_id: 'a3', code: 'HUILE', designation: 'Huile 1L', categorie_nom: 'Épicerie',
      stock: 0, stock_alerte: 5, prix_achat: 1200, prix_achat_estime: false,
      valeur: 0, etat: 'rupture' },
    { article_id: 'a4', code: 'SUCRE', designation: 'Sucre 1kg', categorie_nom: 'Épicerie',
      stock: 3, stock_alerte: 10, prix_achat: 700, prix_achat_estime: true,
      valeur: 2100, etat: 'alerte' },
    { article_id: 'a1', code: 'RIZ', designation: 'Riz parfumé', categorie_nom: 'Épicerie',
      stock: 96, stock_alerte: 10, prix_achat: 500, prix_achat_estime: false,
      valeur: 48000, etat: 'ok' },
  ],
};

const ENCOURS_CLIENTS = {
  total: 62920,
  lignes: [
    { tiers_id: 't1', nom: 'Awa Ndiaye', telephone: '77 123 45 67', solde: 62920,
      facture: 200422, regle: 137502, nb_pieces_ouvertes: 2,
      plus_ancienne: '2026-04-02', anciennete_jours: 116 },
    { tiers_id: 't3', nom: 'Moussa Fall', telephone: null, solde: 0,
      facture: 5000, regle: 5000, nb_pieces_ouvertes: 1,
      plus_ancienne: '2026-07-25', anciennete_jours: 2 },
  ],
};

const A_COMPLETER = [
  { id: 'a2', code: 'YASSA', designation: 'Poulet yassa', categorie_nom: 'Plats',
    prix_vente: 3000, prix_achat: 1050, estime: true, marge_pct: 65, quantite_vendue: 40 },
  { id: 'a5', code: 'BISSAP', designation: 'Jus de bissap', categorie_nom: 'Boissons',
    prix_vente: 500, prix_achat: null, estime: false, marge_pct: null, quantite_vendue: 12 },
];

const REPONSES = {
  '/api/rapports/journal-ventes': JOURNAL_VENTES,
  '/api/rapports/journal-achats': JOURNAL_ACHATS,
  '/api/rapports/marges': MARGES,
  '/api/rapports/stock': STOCK,
  '/api/rapports/encours-clients': ENCOURS_CLIENTS,
  '/api/rapports/encours-fournisseurs': { total: 65920, lignes: [] },
  '/api/prix/a-completer': A_COMPLETER,
};

function monter(fichier, url) {
  const appels = [];
  const erreurs = [];
  const vc = new VirtualConsole();
  vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
  vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

  const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
  const html = readFileSync('D:/DJGUI_ERP/frontend/' + fichier, 'utf8')
    .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
    .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
    .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
      window.Djigui = {
        api: async (chemin, opts) => {
          appelsJS.push({ chemin, method: (opts && opts.method) || 'GET', body: opts && opts.body });
          if (opts && opts.method && opts.method !== 'GET')
            return { estimes: 2, ignores: 1, effaces: 2, enregistres: 1 };
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

  // ⚠️ Données posées dans beforeParse : le script part pendant le parsing et
  // lance son premier appel réseau tout de suite. Les poser après ferait
  // échouer cet appel en silence (.catch) et le test mentirait.
  const dom = new JSDOM(html, {
    runScripts: 'dangerously', url: 'http://localhost:1704/' + url,
    virtualConsole: vc, pretendToBeVisual: true,
    beforeParse(f) { f.appelsJS = appels; f.REPONSES_JS = REPONSES; },
  });
  return { w: dom.window, d: dom.window.document, appels, erreurs };
}

const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const pause = (ms = 80) => new Promise(r => setTimeout(r, ms));

// ===========================================================================
// rapports.html
// ===========================================================================
{
  const { w, d, appels, erreurs } = monter('rapports.html', 'rapports.html');
  await pause(300);
  const visible = el => el && w.getComputedStyle(el).display !== 'none';
  const onglet = n => [...d.querySelectorAll('#rap-tabs .tab')].find(t => t.dataset.tab === n);
  const panneau = n => d.querySelector(`[data-panel="${n}"]`);
  const appele = c => appels.some(a => a.chemin.split('?')[0] === c);

  v('rapports : aucune erreur JS', erreurs.length === 0);
  v('rapports : le journal des ventes se charge en premier', appele('/api/rapports/journal-ventes'));
  v('rapports : on ne charge QUE l\'onglet visible',
    !appele('/api/rapports/stock') && !appele('/api/rapports/marges'));

  // Période : « Ce mois » est posé au démarrage et part dans la requête.
  const premier = appels.find(a => a.chemin.startsWith('/api/rapports/journal-ventes'));
  v('rapports : la période est transmise', premier.chemin.includes('du='));
  v('rapports : le libellé de période est affiché',
    d.getElementById('libelle-periode').textContent.length > 0);

  v('rapports : les tuiles de synthèse sont remplies',
    d.querySelectorAll('#tuiles-ventes .tuile').length === 5);
  v('rapports : les 2 pièces sont listées',
    d.querySelectorAll('#corps-ventes tr').length === 2);
  // Une facture annulée reste visible mais barrée : la masquer donnerait
  // l'impression d'un trou dans la numérotation.
  v('rapports : la facture annulée est présente mais marquée',
    d.querySelector('#corps-ventes tr.annule') !== null);

  // Changement de période → nouvelle requête.
  const avant = appels.length;
  d.querySelector('#chips-periode [data-p="annee"]').click();
  await pause(150);
  v('rapports : changer de période relance la requête', appels.length > avant);

  // Onglet Marges : l'avertissement « prix estimé » doit apparaître.
  onglet('marges').click();
  await pause(200);
  v('rapports : l\'onglet Marges affiche son panneau', visible(panneau('marges')));
  v('rapports : les marges sont chargées', appele('/api/rapports/marges'));
  v('rapports : les 2 articles sont listés', d.querySelectorAll('#corps-marges tr').length === 2);
  const bandeau = d.getElementById('bandeau-estime');
  v('rapports : un coût incertain déclenche l\'avertissement', visible(bandeau));
  v('rapports : l\'avertissement renvoie vers « Compléter mes prix »',
    bandeau.innerHTML.includes('completer-prix.html'));
  v('rapports : l\'article au coût incertain porte le badge',
    d.querySelector('#corps-marges tr:nth-child(2)').textContent.includes('prix estimé'));
  v('rapports : l\'article au coût sûr ne porte PAS le badge',
    !d.querySelector('#corps-marges tr:nth-child(1)').textContent.includes('prix estimé'));

  // Onglet Stock : les ennuis d'abord, et les états colorés.
  onglet('stock').click();
  await pause(200);
  v('rapports : le stock est chargé', appele('/api/rapports/stock'));
  v('rapports : la rupture est surlignée', d.querySelector('#corps-stock tr.rupture') !== null);
  v('rapports : le stock bas est surligné', d.querySelector('#corps-stock tr.alerte') !== null);
  v('rapports : le prix estimé est signalé dans le stock',
    d.getElementById('corps-stock').textContent.includes('estimé'));

  // Onglet Encours : ancienneté mise en avant.
  onglet('clients').click();
  await pause(200);
  v('rapports : les encours clients sont chargés', appele('/api/rapports/encours-clients'));
  v('rapports : la créance ancienne est surlignée',
    d.querySelector('#corps-clients tr.alerte') !== null);
  v('rapports : l\'ancienneté en jours est affichée',
    d.getElementById('corps-clients').textContent.includes('116'));

  // Un rapport vide dit quelque chose d'utile, il ne reste pas blanc.
  onglet('fournisseurs').click();
  await pause(200);
  v('rapports : un encours vide affiche un message clair',
    d.getElementById('corps-fournisseurs').textContent.includes('Vous ne devez rien'));
}

// ===========================================================================
// completer-prix.html
// ===========================================================================
{
  const { w, d, appels, erreurs } = monter('completer-prix.html', 'completer-prix.html');
  await pause(300);
  const visible = el => el && w.getComputedStyle(el).display !== 'none';

  v('prix : aucune erreur JS', erreurs.length === 0);
  v('prix : la liste est chargée', appels.some(a => a.chemin === '/api/prix/a-completer'));
  v('prix : les 2 articles sont listés', d.querySelectorAll('#corps tr[data-id]').length === 2);
  v('prix : l\'article estimé porte le badge « prix estimé »',
    d.querySelector('#corps tr[data-id="a2"]').textContent.includes('prix estimé'));
  v('prix : l\'article sans prix porte le badge « sans prix »',
    d.querySelector('#corps tr[data-id="a5"]').textContent.includes('sans prix'));
  v('prix : le résumé avertit sur les deux cas', visible(d.getElementById('resume')));

  // Le bouton d'enregistrement reste inactif tant qu'on n'a rien saisi.
  const enr = d.getElementById('btn-enregistrer');
  v('prix : « Enregistrer » est désactivé au départ', enr.disabled);

  // Saisie : la marge se recalcule pendant la frappe.
  const champ = d.querySelector('#corps input[data-id="a5"]');
  champ.value = '200';
  champ.dispatchEvent(new w.Event('input', { bubbles: true }));
  await pause();
  const ligne = d.querySelector('#corps tr[data-id="a5"]');
  v('prix : la marge se recalcule à la frappe',
    ligne.querySelector('.marge').textContent === '60 %');
  v('prix : la ligne modifiée est mise en évidence', ligne.classList.contains('modifiee'));
  v('prix : « Enregistrer » s\'active après une saisie', !enr.disabled);

  // Un prix d'achat supérieur au prix de vente doit se voir tout de suite.
  champ.value = '600';
  champ.dispatchEvent(new w.Event('input', { bubbles: true }));
  await pause();
  v('prix : une marge négative est signalée en rouge',
    ligne.querySelector('.marge').classList.contains('neg'));

  // Effacer la saisie retire la ligne des modifications.
  champ.value = '';
  champ.dispatchEvent(new w.Event('input', { bubbles: true }));
  await pause();
  v('prix : vider le champ annule la modification', !ligne.classList.contains('modifiee'));
  v('prix : « Enregistrer » se redésactive', enr.disabled);

  // Enregistrement : on envoie bien ce qui a été saisi, et rien d'autre.
  champ.value = '150';
  champ.dispatchEvent(new w.Event('input', { bubbles: true }));
  await pause();
  const avant = appels.length;
  enr.click();
  await pause(200);
  const envoi = appels.slice(avant).find(a => a.chemin === '/api/prix/reels');
  v('prix : l\'enregistrement appelle le serveur', !!envoi);
  v('prix : un seul prix est envoyé', envoi && envoi.body.prix.length === 1);
  v('prix : le prix envoyé est celui saisi',
    envoi && envoi.body.prix[0].article_id === 'a5' && envoi.body.prix[0].prix_achat === 150);

  // Proposer / effacer les estimations.
  const av2 = appels.length;
  d.getElementById('btn-proposer').click();
  await pause(200);
  v('prix : « Proposer des prix » appelle l\'estimation',
    appels.slice(av2).some(a => a.chemin === '/api/prix/estimer' && a.method === 'POST'));
  const av3 = appels.length;
  d.getElementById('btn-effacer').click();
  await pause(200);
  v('prix : « Effacer » appelle la suppression des estimations',
    appels.slice(av3).some(a => a.chemin === '/api/prix/effacer-estimations' && a.method === 'POST'));
}

for (const t of ok) console.log('  ok   ' + t);
for (const t of ko) console.log('  ÉCHEC ' + t);
console.log(`\nsmoke-rapports : ${ok.length}/${ok.length + ko.length}`);
process.exit(ko.length ? 1 : 0);
