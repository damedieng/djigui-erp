// Test de fumée de marche-detail.html — onglets Avenants et Réception.
//
// Ce que cet écran doit garantir, et qui est vérifié ici :
//   1. Le montant du marché d'origine ne bouge JAMAIS. Seul le « montant
//      courant » intègre les avenants, et seulement ceux qui sont approuvés.
//   2. Un avenant approuvé est FIGÉ : plus de bouton modifier ni supprimer.
//   3. Une réception « avec réserves » ne part pas sans le texte des réserves,
//      et l'écran le dit avant le serveur.
//   4. Les réserves ouvertes se voient, parce que c'est de l'argent retenu.
//
// ⚠️ jsdom ne calcule PAS la mise en page : logique et affichage/masquage
// seulement. Pour tout ce qui est visuel → capture-marche.mjs (vrai Chrome).
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

// Marché à 20 M : un avenant approuvé (+3 M, +20 j) et un encore en projet
// (+1 M), une réception provisoire avec réserves NON levées.
const MARCHE = {
  id: 'm1', numero: 'MA-2026-0001', objet: 'Construction du forage de Ndioum',
  type_id: 'mt-travaux', type_libelle: 'Travaux',
  montant_estime: 20000000, monnaie: 'FCFA', statut: 'en_cours',
  date_lancement: '2026-01-05', date_cloture_prevue: '2026-03-10',
  cree_le: '2026-01-05T08:00:00',
  nb_etapes: 8, nb_etapes_terminees: 2, avancement: 25,
  nb_soumissionnaires: 0,
  nb_avenants: 2, montant_avenants: 3000000, delai_avenants_jours: 20,
  montant_courant: 23000000, avenants_pct: 15,
  date_cloture_revisee: '2026-03-30',
  nb_receptions: 1, reserves_ouvertes: true,
  alertes: [
    'Les avenants accordent 20 jour(s) : la clôture serait reportée au 2026-03-30.',
    'Des réserves de réception ne sont pas levées : la retenue de garantie reste due.',
  ],
  etapes: [
    { id: 'e1', marche_id: 'm1', libelle: 'Preparation du dossier', ordre: 1,
      date_prevue: '2026-01-15', date_effective: '2026-01-15', statut: 'termine',
      obligatoire: true, modifiable: true, nb_documents: 0, valide_par: 'Administrateur',
      verrouillee: false, est_courante: false, derogation: false,
      ecart_jours: 0, ecart_en_cours: false },
    // L'etape du moment : la premiere non franchie. Son echeance est passee et
    // elle n'est pas faite : le retard COURT.
    { id: 'e2', marche_id: 'm1', libelle: 'Ouverture des plis', ordre: 2,
      date_prevue: '2026-02-01', statut: 'en_cours',
      obligatoire: true, modifiable: true, nb_documents: 0,
      verrouillee: false, est_courante: true, derogation: false,
      ecart_jours: 12, ecart_en_cours: true },
    // Verrouillee : elle vient apres une etape non terminee.
    { id: 'e3', marche_id: 'm1', libelle: 'Evaluation des offres', ordre: 3,
      date_prevue: '2026-02-10', statut: 'en_attente',
      obligatoire: true, modifiable: true, nb_documents: 0,
      verrouillee: true, raison_verrou: "Terminez d'abord : Ouverture des plis",
      est_courante: false, derogation: false },   // pas d'ecart : echeance a venir
    // Franchie hors de son rang, en l'assumant.
    { id: 'e4', marche_id: 'm1', libelle: 'Signature du contrat', ordre: 4,
      date_prevue: '2026-03-01', date_effective: '2026-02-20', statut: 'termine',
      obligatoire: true, modifiable: true, nb_documents: 0, valide_par: 'Administrateur',
      verrouillee: false, est_courante: false, derogation: true,
      motif_derogation: 'Dossier repris en cours sur support papier',
      derogation_par: 'Administrateur',
      ecart_jours: -9, ecart_en_cours: false },
  ],
  tentative: 2,
  recours_en_cours: "Contestation des criteres d'evaluation",
  incidents: [
    { id: 'inc1', marche_id: 'm1', type_incident: 'recours',
      date_incident: '2026-03-05', motif: "Contestation des criteres d'evaluation",
      auteur_recours: 'Entreprise Ndiaye & Freres', statut: 'ouvert',
      tentative: 2, cree_le: '2026-03-05T09:00:00' },
    { id: 'inc2', marche_id: 'm1', type_incident: 'infructueux',
      date_incident: '2026-02-01', motif: 'Aucune offre conforme recue',
      statut: 'clos', decision: 'Procedure relancee', date_decision: '2026-02-03',
      tentative: 1, cree_le: '2026-02-01T09:00:00' },
  ],
  soumissionnaires: [],
  avenants: [
    { id: 'av1', marche_id: 'm1', numero: 1, objet: 'Extension du réseau',
      montant_variation: 3000000, delai_jours: 20, date_avenant: '2026-02-10',
      statut: 'approuve', approuve_par: 'djigui', approuve_le: '2026-02-11T09:00:00',
      modifiable: false },
    { id: 'av2', marche_id: 'm1', numero: 2, objet: 'Reprise de la margelle',
      montant_variation: 1000000, delai_jours: 0, date_avenant: '2026-03-01',
      statut: 'projet', modifiable: true },
  ],
  receptions: [
    { id: 'r1', marche_id: 'm1', type_reception: 'provisoire',
      date_reception: '2026-04-01', resultat: 'avec_reserves',
      reserves: 'Margelle non finie', garantie_mois: 12,
      montant_retenue_garantie: 1000000, receptionne_par: 'Commission',
      cree_le: '2026-04-01T10:00:00', reserves_ouvertes: true,
      fin_garantie: '2027-03-27' },
  ],
};

const REPONSES = { '/api/marches/m1': MARCHE, '/api/tiers': [] };

function monter() {
  const appels = [];
  const erreurs = [];
  const vc = new VirtualConsole();
  vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
  vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

  const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
  const html = readFileSync('D:/DJGUI_ERP/frontend/marche-detail.html', 'utf8')
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
        alert: () => {}, confirm: async () => true,
        selectRecherche: () => ({ setItems(){}, setValue(){}, value: '' }),
        estAdmin: () => true,
      };
    </script>`);

  // ⚠️ beforeParse : le script part pendant le parsing et lance son premier
  // appel réseau tout de suite. Les poser après ferait échouer cet appel en
  // silence et le test mentirait.
  const dom = new JSDOM(html, {
    runScripts: 'dangerously',
    url: 'http://localhost:1704/marche-detail.html?id=m1',
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
await pause(300);
const visible = el => el && w.getComputedStyle(el).display !== 'none';
const onglet = n => [...d.querySelectorAll('#onglets .tab')].find(t => t.dataset.tab === n);
const panneau = n => d.querySelector(`[data-panel="${n}"]`);
const txt = sel => (d.querySelector(sel)?.textContent || '').replace(/\s+/g, ' ').trim();

// --- Chargement -------------------------------------------------------------
v('aucune erreur JS', erreurs.length === 0);
v('le marché est chargé', appels.some(a => a.chemin === '/api/marches/m1'));
v('les 5 onglets sont présents', d.querySelectorAll('#onglets .tab').length === 5);
v('les 2 alertes sont affichées', d.querySelectorAll('#alertes li').length === 2);

// --- En-tête : le montant d'origine ne bouge pas ----------------------------
// L'en-tête reprend la présentation du détail d'un projet (bandeau + tuiles
// `.pe-bloc` + deux jauges) : styles partagés dans styles.css.
const entete = txt('#h-synthese');
v('en-tête : l\'objet du marché est affiché', txt('#h-objet') === MARCHE.objet);
v('en-tête : le statut est modifiable depuis le bandeau',
  d.getElementById('h-statut').value === 'en_cours');
v('en-tête : le montant estimé reste celui d\'origine', entete.includes('20000000'));
v('en-tête : le montant courant intègre les avenants', entete.includes('23000000'));
v('en-tête : la part des avenants est affichée', entete.includes('15 %'));
v('en-tête : la réception signale les réserves ouvertes', entete.includes('Réserves ouvertes'));
v('en-tête : le type de marché est rappelé', entete.includes('Travaux'));

// Deux jauges : la procédure et le délai écoulé. La seconde répond à la vraie
// question — « suis-je dans les temps ? ».
const jauges = txt('#h-jauges');
v('en-tête : la jauge de procédure est présente', jauges.includes('Procédure'));
v('en-tête : la jauge de délai est présente', jauges.includes('Délai écoulé'));
v('en-tête : la jauge de procédure donne le compte d\'étapes',
  jauges.includes('2/8'));
v('en-tête : les deux jauges sont dessinées',
  d.querySelectorAll('#h-jauges .barre').length === 2);

// Un marché annulé ne se « désannule » pas d'un clic : l'annulation est un acte
// motivé. Le sélecteur reste actif ici parce que ce marché est en cours.
v('en-tête : le sélecteur de statut est actif sur un marché en cours',
  !d.getElementById('h-statut').disabled);

// --- Onglet Avenants --------------------------------------------------------
v('le panneau Avenants est masqué au départ', !visible(panneau('avenants')));
onglet('avenants').click();
await pause();
v('l\'onglet Avenants affiche son panneau', visible(panneau('avenants')));
v('les 2 avenants sont listés', d.querySelectorAll('#corps-avenants tr').length === 2);

const lignes = [...d.querySelectorAll('#corps-avenants tr')];
// L'avenant n° 1 est approuvé : figé, aucun bouton d'action.
v('l\'avenant approuvé n\'a plus de bouton modifier',
  lignes[0].querySelector('[data-modifier-avenant]') === null);
v('l\'avenant approuvé n\'a plus de bouton supprimer',
  lignes[0].querySelector('[data-supprimer-avenant]') === null);
v('l\'avenant approuvé est marqué « figé »', lignes[0].textContent.includes('figé'));
// L'avenant n° 2 est un projet : tout est encore possible.
v('l\'avenant en projet peut être approuvé',
  lignes[1].querySelector('[data-approuver]') !== null);
v('l\'avenant en projet peut être modifié',
  lignes[1].querySelector('[data-modifier-avenant]') !== null);
// Le pied de table rappelle ce qui compte vraiment.
v('le pied de table donne le montant courant',
  txt('#pied-avenants').includes('23000000'));
v('le pied précise « avenants approuvés seulement »',
  txt('#pied-avenants').includes('approuvés seulement'));

// Approbation : c'est bien un POST /statut, avec le bon statut.
lignes[1].querySelector('[data-approuver]').click();
await pause(150);
const appr = appels.find(a => a.chemin === '/api/avenants/av2/statut');
v('approuver appelle la bonne route', appr !== undefined);
v('approuver envoie statut=approuve', appr && appr.body.statut === 'approuve');

// Création : l'objet est obligatoire côté écran, avant même le serveur.
d.getElementById('btn-nouvel-avenant').click();
await pause();
v('la modale avenant s\'ouvre', !d.getElementById('modal-avenant').hidden);
v('la date du jour est préremplie', d.getElementById('a-date').value.length === 10);
const avantVide = appels.length;
d.getElementById('a-objet').value = '   ';
d.getElementById('btn-enr-avenant').click();
await pause();
v('un avenant sans objet n\'est pas envoyé', appels.length === avantVide);
v('l\'écran explique pourquoi',
  toasts.some(t => (t.msg || '').includes('objet de l\'avenant')));

d.getElementById('a-objet').value = 'Travaux supplémentaires';
d.getElementById('a-montant').value = '-500000';
d.getElementById('a-delai').value = '5';
d.getElementById('btn-enr-avenant').click();
await pause(150);
const cree = appels.find(a => a.chemin === '/api/marches/m1/avenants');
v('la création part sur la bonne route', cree !== undefined);
// Une diminution est un cas réel : elle doit passer telle quelle.
v('un montant négatif est accepté', cree && cree.body.montant_variation === -500000);
v('le délai est transmis', cree && cree.body.delai_jours === 5);
v('la modale se referme après enregistrement', d.getElementById('modal-avenant').hidden);

// --- Onglet Réception -------------------------------------------------------
onglet('reception').click();
await pause();
v('l\'onglet Réception affiche son panneau', visible(panneau('reception')));
v('la réception est listée', d.querySelectorAll('#corps-receptions tr').length === 1);
const lr = d.querySelector('#corps-receptions tr');
v('la ligne à réserves ouvertes est surlignée', lr.classList.contains('retard'));
v('les réserves sont lisibles', lr.textContent.includes('Margelle non finie'));
v('la retenue de garantie est affichée', lr.textContent.includes('1000000'));
v('la fin de garantie est affichée', lr.textContent.includes('2027-03-27'));
v('le bouton « lever les réserves » est proposé',
  lr.querySelector('[data-lever]') !== null);

lr.querySelector('[data-lever]').click();
await pause(150);
v('lever les réserves appelle la bonne route',
  appels.some(a => a.chemin === '/api/receptions/r1/lever-reserves'));

// La règle dure de l'écran : des réserves sans texte ne partent pas.
d.getElementById('btn-nouvelle-reception').click();
await pause();
v('la modale réception s\'ouvre', !d.getElementById('modal-reception').hidden);
// ⚠️ On vérifie le display CALCULÉ, pas la propriété `.hidden` : une règle
// d'auteur `display:flex` bat le `display:none` de `[hidden]`, et le champ
// resterait visible sans qu'aucun test sur `.hidden` ne s'en aperçoive.
// C'est exactement ce que la capture Chrome a trouvé ici.
v('le champ réserves est masqué quand le résultat est « prononcée »',
  !visible(d.getElementById('bloc-reserves')));
d.getElementById('r-resultat').value = 'avec_reserves';
d.getElementById('r-resultat').dispatchEvent(new w.Event('change'));
await pause();
v('choisir « avec réserves » fait apparaître le champ',
  visible(d.getElementById('bloc-reserves')));

const avantR = appels.length;
d.getElementById('btn-enr-reception').click();
await pause();
v('une réception avec réserves vides n\'est pas envoyée', appels.length === avantR);
v('l\'écran dit ce qui manque',
  toasts.some(t => (t.msg || '').includes('réserves doit préciser')));

d.getElementById('r-reserves').value = 'Peinture à reprendre';
d.getElementById('r-garantie').value = '12';
d.getElementById('r-retenue').value = '500000';
d.getElementById('btn-enr-reception').click();
await pause(150);
const creeR = appels.find(a => a.chemin === '/api/marches/m1/receptions');
v('la réception part sur la bonne route', creeR !== undefined);
v('les réserves sont transmises', creeR && creeR.body.reserves === 'Peinture à reprendre');
v('la garantie est transmise', creeR && creeR.body.garantie_mois === 12);
v('la retenue de garantie est transmise', creeR && creeR.body.montant_retenue_garantie === 500000);

// --- Informations -----------------------------------------------------------
onglet('infos').click();
await pause();
v('la clôture reportée par avenant est affichée dans les informations',
  txt('#infos').includes('Clôture reportée par avenant'));

// --- Enchainement de la procedure -------------------------------------------
// Le reproche de l'utilisateur : « la saisie est plate, quelqu'un peut annuler
// l'ouverture des plis et continuer les autres etapes ». L'ecran doit rendre la
// chaine visible.
const ligneEtape = id => d.querySelector(`#corps-etapes tr[data-etape="${id}"]`);

v("procedure : l'etape du moment est mise en avant",
  ligneEtape('e2') && ligneEtape('e2').classList.contains('courante'));
v("procedure : elle est nommee « etape du moment »",
  txt('#corps-etapes').includes('etape du moment') || txt('#corps-etapes').includes('\u00e9tape du moment'));
v("procedure : l'etape verrouillee est estompee",
  ligneEtape('e3') && ligneEtape('e3').classList.contains('verrouillee'));
// Un cadenas muet n'aide personne : il doit DIRE ce qu'il faut terminer.
v('procedure : le verrou dit ce qui bloque',
  ligneEtape('e3').textContent.includes("Terminez d'abord : Ouverture des plis"));
v("procedure : l'etape verrouillee propose « passer outre »",
  ligneEtape('e3').querySelector('[data-deroger]') !== null);
v('procedure : une etape ouverte ne propose pas « passer outre »',
  ligneEtape('e2').querySelector('[data-deroger]') === null);
// La derogation se voit, avec son motif au survol.
const marqueDero = ligneEtape('e4').querySelector('.retard-info[data-tip]');
v('procedure : la derogation est signalee', !!marqueDero);
v('procedure : le motif de derogation est lisible au survol',
  marqueDero && marqueDero.getAttribute('data-tip').includes('support papier'));
v("procedure : l'auteur de la derogation est indique",
  marqueDero && marqueDero.getAttribute('data-tip').includes('Administrateur'));

// La modale de derogation exige un motif.
ligneEtape('e3').querySelector('[data-deroger]').click();
await pause();
v("derogation : la modale s'ouvre", !d.getElementById('modal-derogation').hidden);
v("derogation : le contexte rappelle l'etape et le verrou",
  txt('#derogation-contexte').includes('Ouverture des plis'));
const avantDero = appels.length;
d.getElementById('btn-confirmer-derogation').click();
await pause();
v("derogation : sans motif, rien n'est envoye", appels.length === avantDero);
d.getElementById('d-motif').value = 'Dossier repris en cours';
d.getElementById('btn-confirmer-derogation').click();
await pause(150);
const dero = appels.find(a => a.chemin === '/api/marche-etapes/e3/statut');
v('derogation : la requete part avec le motif',
  dero && dero.body.motif_derogation === 'Dossier repris en cours');
v("derogation : elle termine bien l'etape", dero && dero.body.statut === 'termine');

// --- Incidents de procedure --------------------------------------------------
v('incidents : le bandeau est affiche', d.querySelector('#incidents .card') !== null);
v('incidents : le recours ouvert est signale', txt('#incidents').includes('Recours'));
v('incidents : son auteur est nomme', txt('#incidents').includes('Ndiaye'));
v("incidents : l'incident clos affiche sa decision",
  txt('#incidents').includes('relanc'));
v("incidents : l'ecran explique que les etapes n'avancent pas",
  txt('#incidents').includes("n'avancent pas"));
v('incidents : un recours ouvert peut etre clos',
  d.querySelector('#incidents [data-clore]') !== null);

// Declarer un appel d'offres infructueux.
d.getElementById('btn-infructueux').click();
await pause();
v("incident : la modale s'ouvre", !d.getElementById('modal-incident').hidden);
// Le champ « qui conteste » n'a de sens que pour un recours.
v('incident : le champ auteur est masque pour un infructueux',
  !visible(d.getElementById('bloc-auteur')));
const avantInc = appels.length;
d.getElementById('btn-enr-incident').click();
await pause();
v("incident : sans motif, rien n'est envoye", appels.length === avantInc);
d.getElementById('i-motif').value = 'Aucune offre recue';
d.getElementById('btn-enr-incident').click();
await pause(200);
const inc = appels.find(a => a.chemin === '/api/marches/m1/incidents');
v('incident : la requete part', inc !== undefined);
v('incident : le type est transmis', inc && inc.body.type_incident === 'infructueux');
v('incident : le motif est transmis', inc && inc.body.motif === 'Aucune offre recue');

// Pour un recours, le champ auteur reapparait.
d.getElementById('btn-recours').click();
await pause();
v('recours : le champ auteur reapparait', visible(d.getElementById('bloc-auteur')));
d.getElementById('modal-incident').hidden = true;

// --- Retours utilisateur du 2026-07-28 --------------------------------------
// 1. La date prevue n'est PAS editable : elle est le fruit de la procedure.
v("etapes : la date prevue n'est plus un champ de saisie",
  d.querySelector('#corps-etapes input[type="date"]') === null);
v('etapes : la date prevue reste lisible',
  ligneEtape('e1').textContent.includes('2026-01-15'));

// 2. Le NUMERO d'etape situe dans la procedure.
v('etapes : chaque ligne porte sa pastille de numero',
  d.querySelectorAll('#corps-etapes .etape-num').length === 4);
v("etapes : l'etape du moment est reperee par sa pastille",
  ligneEtape('e2').querySelector('.etape-num.courante') !== null);
v('etapes : une etape faite porte la pastille verte',
  ligneEtape('e1').querySelector('.etape-num.faite') !== null);
v('etapes : une etape a venir garde son chiffre lisible',
  ligneEtape('e3').querySelector('.etape-num').textContent.trim() === '3');

// 3. « Validee par » doit etre un NOM, jamais un identifiant technique.
const cellules = [...ligneEtape('e1').querySelectorAll('td')].map(x => x.textContent);
v('etapes : le validateur est affiche par son nom',
  cellules.some(t => t.includes('Administrateur')));
v("etapes : aucun identifiant technique n'apparait a l'ecran",
  !txt('#corps-etapes').match(/[0-9a-f]{8}-[0-9a-f]{4}-/));

// 4. Changer l'etat DEMANDE la date et l'observation.
const avantEtat = appels.length;
const selStatut = ligneEtape('e2').querySelector('.sel-statut');
selStatut.value = 'termine';
selStatut.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(150);
v("etat : rien n'est envoye avant la saisie", appels.length === avantEtat);
v("etat : la modale de saisie s'ouvre", !d.getElementById('modal-etape').hidden);
v("etat : la liste deroulante est remise a l'etat reel",
  selStatut.value === 'en_cours');
v('etat : la date est proposee', d.getElementById('e-date').value.length === 10);
v("etat : le contexte rappelle l'etape et la date prevue",
  txt('#etape-contexte').includes('2026-02-01'));

d.getElementById('e-date').value = '2026-02-03';
d.getElementById('e-obs').value = '4 plis recus, ouverture contradictoire';
d.getElementById('btn-confirmer-etape').click();
await pause(200);
const maj = appels.find(a => a.chemin === '/api/marche-etapes/e2/statut');
v('etat : la requete part', maj !== undefined);
v("etat : la date SAISIE est transmise, pas celle du jour",
  maj && maj.body.date_effective === '2026-02-03');
v("etat : l'observation est transmise",
  maj && maj.body.observations === '4 plis recus, ouverture contradictoire');
v('etat : le statut demande est transmis', maj && maj.body.statut === 'termine');

// Rouvrir une etape franchie : l'avertissement est DANS la modale, avant d'agir.
const selFait = ligneEtape('e1').querySelector('.sel-statut');
selFait.value = 'en_attente';
selFait.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(150);
v('reouverture : la modale previent de la mise en cause',
  txt('#etape-avertissement').includes('fonde celles qui suivent'));
v('reouverture : les etapes concernees sont nommees',
  txt('#etape-avertissement').includes('Signature du contrat'));
d.getElementById('modal-etape').hidden = true;

// --- Colonne « Ecart » (demande user du 2026-07-28) -------------------------
// La distance entre realise et prevu : + retard (rouge), - avance ou 0 (vert).
v('ecart : la colonne est dans l\'en-tete',
  [...d.querySelectorAll('#corps-etapes')].length === 1
  && txt('.tab-panel[data-panel="etapes"] thead').includes('Écart'));

const ecart = id => ligneEtape(id).querySelector('.ecart');
v('ecart : une etape a l\'heure est en VERT',
  ecart('e1') && ecart('e1').classList.contains('ok'));
v('ecart : une etape a l\'heure affiche 0 j',
  ecart('e1').textContent.trim() === '0 j');
v('ecart : une avance est en vert et signee -',
  ecart('e4') && ecart('e4').classList.contains('ok')
  && ecart('e4').textContent.trim() === '-9 j');
v('ecart : un retard est en ROUGE',
  ecart('e2') && ecart('e2').classList.contains('retard'));
v('ecart : un retard est signe +', ecart('e2').textContent.trim() === '+12 j');
// Un retard EN COURS se distingue d'un retard constate : l'acte n'est pas pose.
v('ecart : un retard en cours est marque a part',
  ecart('e2').classList.contains('encours'));
v('ecart : un retard constate ne l\'est pas',
  !ecart('e4').classList.contains('encours'));
// L'explication au survol passe par l'info-bulle maison.
v('ecart : l\'explication est une info-bulle maison',
  ecart('e2').hasAttribute('data-tip'));
v('ecart : elle dit que l\'etape n\'est toujours pas faite',
  ecart('e2').getAttribute('data-tip').includes("toujours pas"));
v('ecart : sur une avance, elle dit « en avance »',
  ecart('e4').getAttribute('data-tip').includes('avance'));
// Une etape sans ecart affiche un tiret, pas une cellule vide muette.
v('ecart : sans ecart, un tiret',
  ligneEtape('e3').querySelector('.ecart') === null
  && [...ligneEtape('e3').querySelectorAll('td')].some(t => t.textContent.includes('—')));

// --- Controle chronologique (capture controle_sur_date.jpg) -----------------
// « Publication de l'avis » faite le 04/11/2025 alors que la preparation du
// dossier l'a ete le 28/07/2026 : un acte ne peut pas preceder celui qui le
// fonde. Le calendrier doit refuser AVANT que le serveur ne le fasse.
//
// Jeu de test : e1 faite le 15/01, e4 faite le 20/02. Une etape situee entre
// les deux est donc bornee par ces deux dates.
const selE2 = ligneEtape('e2').querySelector('.sel-statut');
selE2.value = 'termine';
selE2.dispatchEvent(new w.Event('change', { bubbles: true }));
await pause(150);

const champDate = d.getElementById('e-date');
v('date : le calendrier a une borne basse', champDate.min === '2026-01-15');
v('date : le calendrier a une borne haute', champDate.max === '2026-02-20');
// Les bornes sont EXPLIQUEES, pas seulement appliquees : un champ qui refuse
// sans dire pourquoi est incomprehensible.
// On compare SANS accents : le test ne doit pas casser sur un « é » alors que
// c'est le sens qui compte.
const sansAccents = t => t.normalize('NFD').replace(/[̀-ͯ]/g, '');
const bornes = sansAccents(txt('#etape-bornes'));
v('date : la borne basse est expliquee',
  bornes.includes('avant le') && bornes.includes('Preparation du dossier'));
v('date : la borne haute est expliquee',
  bornes.includes('apres le') && bornes.includes('Signature du contrat'));

// Une date trop tot n'est pas envoyee.
const avantTrop = appels.length;
champDate.value = '2025-11-04';
d.getElementById('btn-confirmer-etape').click();
await pause(150);
v('date : une date anterieure a l\'etape precedente n\'est PAS envoyee',
  appels.length === avantTrop);
v('date : le refus nomme l\'etape bloquante',
  toasts.some(t => sansAccents(t.msg || '').includes('Preparation du dossier')
                && sansAccents(t.msg || '').includes('impossible')));

// Une date trop tard non plus.
const avantTard = appels.length;
champDate.value = '2026-03-15';
d.getElementById('btn-confirmer-etape').click();
await pause(150);
v('date : une date posterieure a une etape deja faite n\'est PAS envoyee',
  appels.length === avantTard);

// Une date coherente passe.
champDate.value = '2026-02-01';
d.getElementById('btn-confirmer-etape').click();
await pause(200);
const env = appels.filter(a => a.chemin === '/api/marche-etapes/e2/statut').pop();
v('date : une date coherente est acceptee', env && env.body.date_effective === '2026-02-01');

// --- Aide -------------------------------------------------------------------
v('l\'onglet Avenants a sa section d\'aide',
  panneau('avenants').querySelector('.aide') !== null);
v('l\'onglet Réception a sa section d\'aide',
  panneau('reception').querySelector('.aide') !== null);

// ===========================================================================
console.log(`\nmarche-detail : ${ok.length}/${ok.length + ko.length} tests passés`);
if (ko.length) {
  console.log('\nÉCHECS :');
  ko.forEach(n => console.log('  ✗ ' + n));
  if (erreurs.length) { console.log('\nErreurs JS :'); erreurs.forEach(e => console.log('  ' + e)); }
  process.exit(1);
}
