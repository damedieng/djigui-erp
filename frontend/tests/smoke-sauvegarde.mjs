// Test de fumée de sauvegarde.html — sauvegarde automatique chiffrée (mig 0042).
//
// Ce que ce test verrouille, dans l'ordre d'importance :
//
//  1. **L'écran ne ment jamais sur l'état de la protection.** Un écran bien
//     rempli qui laisse croire qu'on est sauvegardé alors que rien n'est
//     configuré est pire que pas d'écran du tout : il installe la fausse
//     tranquillité. Les alertes doivent donc apparaître, et être ALARMANTES.
//  2. **La restauration demande le bon secret.** Le mode `licence` en exige un
//     lui aussi — l'oublier faisait un écran qui ne demandait rien puis
//     échouait. Ce bug a réellement existé (attrapé côté Rust).
//  3. **On prévient avant tout ce qui est irréversible** : perdre un mot de
//     passe, écraser les données par une restauration.
import { readFileSync } from 'node:fs';
import { JSDOM, VirtualConsole } from 'jsdom';

// --- Jeux d'état ------------------------------------------------------------
// Cas nominal : licence posée, deux dossiers dont un hors machine, tout va bien.
const PARAMS_OK = {
  activee: true, cette_machine_est_serveur: true, a_la_fermeture: true,
  copies_a_conserver: 10, mode_cle: 'licence', mot_de_passe_defini: false,
  licence_definie: true, licence_fin: '7Q4X',
  // ⚠️ Cohérent avec JOURNAL[0] : le serveur écrit TOUJOURS les deux ensemble.
  // Un jeu d'essai incohérent (journal « partiel », paramètres « succès ») ferait
  // passer ou échouer des tests pour de mauvaises raisons.
  derniere_sauvegarde: '2026-07-29 19:40:00', dernier_statut: 'partiel',
};

const DESTINATIONS = [
  { id: 'd1', libelle: 'Clé USB bleue', chemin: 'E:\\Sauvegardes Djigui', actif: true,
    ordre: 1, accessible: true, dernier_essai: '2026-07-29 19:40:00',
    dernier_statut: 'succes', dernier_message: '' },
  { id: 'd2', libelle: 'Dossier Drive', chemin: 'C:\\Users\\dj\\Mon Drive', actif: true,
    ordre: 2, accessible: false, dernier_essai: '2026-07-29 19:40:00',
    dernier_statut: 'echec', dernier_message: 'Le dossier est introuvable.' },
];

const JOURNAL = [
  { id: 'j1', horodatage: '2026-07-29 19:40:00', declencheur: 'fermeture',
    nom_fichier: 'djigui-20260729-194000.djigui', taille_octets: 2411724,
    statut: 'partiel', nb_destinations_ok: 1, nb_destinations_echec: 1,
    verifiee: true, message: 'Sauvegarde écrite sur 1 destination(s), mais 1 a échoué.' },
  { id: 'j2', horodatage: '2026-07-28 18:02:00', declencheur: 'manuelle',
    nom_fichier: 'djigui-20260728-180200.djigui', taille_octets: 2390110,
    statut: 'succes', nb_destinations_ok: 2, nb_destinations_echec: 0,
    verifiee: true, message: 'Sauvegarde réussie : 2 copie(s), 14 document(s) joints, 2.3 Mo.' },
];

const SUGGESTIONS = [
  { libelle: 'Google Drive (Mon Drive)', chemin: 'C:\\Users\\dj\\Mon Drive',
    explication: 'Copie envoyée automatiquement dans votre Drive.', hors_machine: true },
  { libelle: 'Mes documents', chemin: 'C:\\Users\\dj\\Documents\\Sauvegardes Djigui',
    explication: 'Facile à retrouver, mais SUR CET ORDINATEUR.', hors_machine: false },
];

const EXPLORATION = {
  chemin: 'E:\\', parent: null, inscriptible: true,
  dossiers: [{ nom: 'Sauvegardes Djigui', chemin: 'E:\\Sauvegardes Djigui' },
             { nom: 'Photos', chemin: 'E:\\Photos' }],
};

const APERCU_LICENCE = {
  cree_le: '2026-07-29 19:40:00', mode_cle: 'licence', secret_requis: true,
  secret_attendu: "La clé de licence remise lors de l'installation",
  version_application: '0.1.0', nb_documents: 14, taille_fichier: 2411724,
};

function reponses({ params = PARAMS_OK, destinations = DESTINATIONS, journal = JOURNAL } = {}) {
  return {
    '/api/sauvegarde/parametres': { parametres: params, destinations, journal },
    '/api/sauvegarde/suggestions': SUGGESTIONS,
    '/api/sauvegarde/parcourir': EXPLORATION,
    '/api/sauvegarde/apercu': APERCU_LICENCE,
  };
}

function monter(etat = {}, resultatExecuter, dossierChoisi) {
  const appels = [];
  const erreurs = [];
  const vc = new VirtualConsole();
  vc.on('jsdomError', e => erreurs.push('jsdomError: ' + (e.message || e)));
  vc.on('error', (...a) => erreurs.push('console.error: ' + a.join(' ')));

  const css = readFileSync('D:/DJGUI_ERP/frontend/styles.css', 'utf8');
  const html = readFileSync('D:/DJGUI_ERP/frontend/sauvegarde.html', 'utf8')
    .replace(/<link rel="stylesheet" href="styles\.css[^"]*">/, `<style>${css}</style>`)
    .replace(/<link rel="stylesheet" href="assets\/tabler[^"]*">/, '')
    .replace(/<script src="assets\/app\.js[^"]*"><\/script>/, `<script>
      window.Djigui = {
        api: async (chemin, opts) => {
          const m = (opts && opts.method) || 'GET';
          appelsJS.push({ chemin, method: m, body: opts && opts.body });
          if (chemin === '/api/sauvegarde/executer') return EXECUTER_JS;
          if (chemin === '/api/sauvegarde/choisir-dossier') {
            if (SELECTEUR_JS === 'panne') throw new Error('sélecteur indisponible');
            return { chemin: SELECTEUR_JS };
          }
          if (m !== 'GET' && chemin !== '/api/sauvegarde/apercu') return { ok: true };
          const r = REPONSES_JS[chemin.split('?')[0]] ?? REPONSES_JS[chemin];
          if (r === undefined) throw new Error('404 ' + chemin);
          return JSON.parse(JSON.stringify(r));
        },
        fmt: n => String(n), esc: s => String(s ?? ''),
        dateFr: s => s || '', toast: (msg, t) => { toastsJS.push({ msg, t }); },
        alert: async (m) => { alertsJS.push(m); },
        confirm: async (q) => { confirmsJS.push(q); return reponseConfirm; },
        estAdmin: () => true, rafraichirMenu: () => {},
      };
    </script>`);

  const dom = new JSDOM(html, {
    runScripts: 'dangerously', url: 'http://localhost:1704/sauvegarde.html',
    virtualConsole: vc, pretendToBeVisual: true,
    // ⚠️ beforeParse et pas après : le script de la page part pendant le
    // parsing, et son premier appel réseau échouerait en silence.
    beforeParse(f) {
      f.appelsJS = appels;
      f.REPONSES_JS = reponses(etat);
      f.EXECUTER_JS = resultatExecuter || {
        statut: 'succes', nom_fichier: 'djigui-20260729-201500.djigui',
        taille_octets: 2411724, verifiee: true, anciennes_supprimees: 0,
        destinations: [{ libelle: 'Clé USB bleue', chemin: 'E:\\', reussi: true, message: 'ok' }],
        message: 'Sauvegarde réussie : 1 copie(s).',
      };
      f.toastsJS = []; f.confirmsJS = []; f.alertsJS = [];
      f.reponseConfirm = true;
      // Ce que renvoie le sélecteur de dossier DU SERVEUR :
      //   une chaîne  → dossier choisi
      //   null        → l'utilisateur a annulé
      //   'panne'     → le sélecteur n'a pas pu s'ouvrir
      f.SELECTEUR_JS = dossierChoisi === undefined ? null : dossierChoisi;
    },
  });
  return { w: dom.window, d: dom.window.document, appels, erreurs,
           toasts: dom.window.toastsJS, confirms: dom.window.confirmsJS,
           alerts: dom.window.alertsJS };
}

const ok = [], ko = [];
const v = (nom, cond) => (cond ? ok : ko).push(nom);
const pause = (ms = 150) => new Promise(r => setTimeout(r, ms));

// ===========================================================================
// CAS 1 — état nominal
// ===========================================================================
{
  const { w, d, appels, erreurs, toasts, confirms, alerts } = monter();
  await pause(350);
  const txt = sel => (d.querySelector(sel)?.textContent || '').replace(/\s+/g, ' ').trim();
  // ⚠️ On mesure le `display` CALCULÉ, jamais `.hidden` : une règle d'auteur
  // `display:flex` écrase le `display:none` de [hidden] (piège rencontré 5 fois
  // dans ce projet), et un test sur `.hidden` passerait au vert en mentant.
  const visible = el => el && w.getComputedStyle(el).display !== 'none';

  v('aucune erreur JS', erreurs.length === 0);
  v('les réglages sont chargés', appels.some(a => a.chemin === '/api/sauvegarde/parametres'));

  // --- Les destinations -----------------------------------------------------
  v('chaque destination a sa ligne', d.querySelectorAll('.dest-ligne').length === 2);
  v('le nom de la destination est affiché', txt('#liste-dest').includes('Clé USB bleue'));
  v('le chemin aussi', txt('#liste-dest').includes('E:\\Sauvegardes Djigui'));
  // Une clé USB débranchée doit se voir tout de suite, pas le jour du sinistre.
  const lignes = [...d.querySelectorAll('.dest-ligne')];
  v('une destination inaccessible est signalée',
    lignes[1].classList.contains('hs') && lignes[1].textContent.includes('inaccessible'));
  v('une destination accessible ne l\'est pas', !lignes[0].classList.contains('hs'));
  v('le motif de l\'échec est rappelé',
    txt('#liste-dest').includes('Le dossier est introuvable'));

  // --- Les alertes : elles doivent dire la vérité ----------------------------
  const alertes = txt('#alertes');
  v('un dossier inaccessible déclenche une alerte', alertes.includes('inaccessible'));
  v('l\'alerte nomme le dossier concerné', alertes.includes('Dossier Drive'));
  v('une sauvegarde partielle est signalée', alertes.includes('partiellement'));
  v('la licence posée ne déclenche PAS d\'alerte', !alertes.includes('licence n\'est pas encore'));

  // --- Les tuiles -----------------------------------------------------------
  const tuiles = txt('#tuiles');
  v('la date de dernière sauvegarde est affichée', tuiles.includes('29/07/2026'));
  v('la protection affichée est la licence', tuiles.includes('Votre licence'));

  // --- Protection -----------------------------------------------------------
  v('l\'état de protection nomme la licence', txt('#etat-protection').includes('licence'));
  v('il rappelle les 4 derniers caractères', txt('#etat-protection').includes('7Q4X'));
  v('il dit de garder la licence', txt('#etat-protection').includes('Gardez-la'));

  // --- Réglages -------------------------------------------------------------
  v('la case « automatique » reflète l\'état', d.querySelector('#p-activee').checked);
  v('la case « à la fermeture » aussi', d.querySelector('#p-fermeture').checked);
  v('le nombre de copies est repris', d.querySelector('#p-copies').value === '10');
  v('la case « cet ordinateur est le serveur » aussi',
    d.querySelector('#p-serveur').checked);

  // --- Onglets --------------------------------------------------------------
  // ⚠️ Les onglets s'identifient par data-vue (convention du projet).
  const panneau = n => d.querySelector(`[data-panneau="${n}"]`);
  v('le panneau « où sauvegarder » est visible au départ', visible(panneau('dest')));
  v('les autres panneaux sont réellement masqués',
    !visible(panneau('protection')) && !visible(panneau('journal')));
  d.querySelector('.tab[data-vue="journal"]').dispatchEvent(new w.Event('click', { bubbles: true }));
  v('cliquer un onglet montre son panneau', visible(panneau('journal')));
  v('et masque le précédent', !visible(panneau('dest')));

  // --- Historique -----------------------------------------------------------
  v('chaque sauvegarde a sa ligne', d.querySelectorAll('#corps-journal tr').length === 2);
  const jrn = txt('#corps-journal');
  v('l\'historique dit le résultat', jrn.includes('Partielle') && jrn.includes('Réussie'));
  v('il distingue fermeture et bouton',
    jrn.includes('Fermeture de Djigui') && jrn.includes('Bouton'));
  v('il montre le nombre de copies réussies sur le total', jrn.includes('1 / 2'));
  v('il indique que la copie a été relue', jrn.includes('relue'));
  v('la taille est lisible pour un humain', jrn.includes('2.3 Mo'));

  // --- Sauvegarder maintenant ----------------------------------------------
  d.querySelector('#btn-maintenant').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(250);
  const exec = appels.find(a => a.chemin === '/api/sauvegarde/executer');
  v('le bouton lance bien une sauvegarde', !!exec);
  v('elle est marquée comme manuelle', exec && exec.body.declencheur === 'manuelle');
  // En mode licence, le serveur a déjà le secret : ne rien demander.
  v('en mode licence, aucun secret n\'est demandé de l\'utilisateur',
    exec && exec.body.mot_de_passe === undefined);
  v('le résultat est annoncé', toasts.some(t => t.msg.includes('réussie')));
  v('l\'écran se recharge après', appels.filter(a => a.chemin === '/api/sauvegarde/parametres').length >= 2);

  // --- Ajout d'une destination ---------------------------------------------
  d.querySelector('#btn-add-dest').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  v('la modale d\'ajout s\'ouvre', visible(d.querySelector('#modal-dest')));
  v('les endroits proposés sont chargés',
    appels.some(a => a.chemin === '/api/sauvegarde/suggestions'));
  // L'explorateur de repli reste masqué : deux façons de choisir un dossier
  // côte à côte font hésiter sur celle qui compte.
  v('l\'explorateur de repli est masqué à l\'ouverture',
    !visible(d.querySelector('#zone-explo')));
  v('et le serveur n\'est pas sollicité pour un panneau invisible',
    !appels.some(a => a.chemin.startsWith('/api/sauvegarde/parcourir')));
  const sugg = txt('#suggestions');
  v('une suggestion hors machine est distinguée', sugg.includes('hors de cet ordinateur'));
  v('une suggestion locale prévient qu\'elle ne protège pas d\'une panne',
    sugg.includes('sur cet ordinateur'));

  // Cliquer une suggestion remplit les deux champs : sans le nom, l'utilisateur
  // se retrouve avec un chemin brut dans ses messages d'échec.
  d.querySelector('[data-sugg]').dispatchEvent(new w.Event('click', { bubbles: true }));
  v('cliquer une suggestion remplit le chemin',
    d.querySelector('#d-chemin').value === 'C:\\Users\\dj\\Mon Drive');
  v('et propose un nom lisible',
    d.querySelector('#d-libelle').value.includes('Google Drive'));

  // Le dossier retenu pour la suite du scénario.
  d.querySelector('#d-chemin').value = 'E:\\Sauvegardes Djigui';

  d.querySelector('#btn-enr-dest').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  const ajout = appels.find(a => a.chemin === '/api/sauvegarde/destinations' && a.method === 'POST');
  v('la destination est envoyée', !!ajout);
  v('elle porte le nom et le chemin',
    ajout && ajout.body.libelle && ajout.body.chemin === 'E:\\Sauvegardes Djigui');

  // --- Retirer une destination ---------------------------------------------
  d.querySelector('[data-suppr]').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  // Le point qui compte : personne ne doit croire qu'il détruit ses copies.
  v('retirer une destination demande confirmation', confirms.length > 0);
  v('la confirmation dit que les copies sont conservées',
    confirms.some(c => c.includes('PAS supprimées')));

  // --- Mot de passe : la mise en garde doit être explicite ------------------
  d.querySelector('#btn-mdp').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  v('poser un mot de passe demande confirmation',
    confirms.some(c => c.includes('Poser ce mot de passe')));
  v('elle prévient que perdre le mot de passe est définitif',
    confirms.some(c => c.includes('définitivement illisibles')));
  v('elle dit que les anciennes sauvegardes restent lisibles',
    confirms.some(c => c.includes('restent lisibles')));

  // --- Restauration ---------------------------------------------------------
  d.querySelector('#btn-restaurer').dispatchEvent(new w.Event('click', { bubbles: true }));
  v('la modale de restauration s\'ouvre', visible(d.querySelector('#modal-restore')));
  v('le bouton de restauration est bloqué tant qu\'on n\'a rien examiné',
    d.querySelector('#btn-lancer-restore').disabled);
  v('l\'avertissement de remplacement est visible',
    txt('#modal-restore').includes('remplace les données actuelles'));

  d.querySelector('#r-chemin').value = 'E:\\Sauvegardes Djigui\\djigui-20260729-194000.djigui';
  d.querySelector('#btn-lire-archive').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  const ap = txt('#apercu-archive');
  v('l\'aperçu annonce la date de la sauvegarde', ap.includes('29/07/2026'));
  v('il annonce le nombre de pièces jointes', ap.includes('14'));
  // ⚠️ LE point : le mode licence exige un secret lui aussi. Un aperçu qui
  // annoncerait « aucun secret » ferait un écran qui ne demande rien et échoue.
  v('il dit QUEL secret sera demandé', ap.includes('clé de licence'));
  v('le bouton de restauration est désormais actif',
    !d.querySelector('#btn-lancer-restore').disabled);

  d.querySelector('#btn-lancer-restore').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(150);
  v('le secret est demandé avant de restaurer', visible(d.querySelector('#modal-secret')));
  v('la question nomme ce qu\'il faut fournir',
    txt('#secret-label').includes('licence'));
  d.querySelector('#secret-valeur').value = 'DJG-MATAM-2026-7Q4X';
  d.querySelector('#btn-secret-ok').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(250);
  v('la restauration demande confirmation',
    confirms.some(c => c.includes('Restaurer la sauvegarde')));
  v('la confirmation prévient de la perte des saisies récentes',
    confirms.some(c => c.includes('sera perdu')));
  const rest = appels.find(a => a.chemin === '/api/sauvegarde/restaurer');
  v('la restauration est envoyée', !!rest);
  v('elle porte le secret saisi', rest && rest.body.mot_de_passe === 'DJG-MATAM-2026-7Q4X');
  v('elle porte le chemin du fichier', rest && rest.body.chemin.endsWith('.djigui'));
  // Sans redémarrage, la connexion serveur pointe encore sur l'ancien fichier :
  // l'écran montrerait les données d'avant et ferait croire à un échec.
  v('on impose de redémarrer Djigui',
    alerts.some(m => m.includes('FERMEZ') && m.includes('ROUVREZ')));

  // --- Aide -----------------------------------------------------------------
  const aide = txt('.aide');
  v('l\'aide dit d\'utiliser un support hors de l\'ordinateur',
    aide.includes('hors de cet ordinateur'));
  v('l\'aide explique que le fichier est chiffré', aide.includes('chiffré'));
  v('l\'aide dit de conserver la licence', aide.includes('Gardez votre licence'));
  v('l\'aide explique la relecture de contrôle', aide.includes('relit chaque copie'));
  v('l\'aide dit que seul le serveur sauvegarde', aide.includes('héberge les données'));
}

// ===========================================================================
// CAS 2 — rien n'est configuré : l'écran doit ALARMER, pas rassurer
// ===========================================================================
{
  const { d } = monter({
    params: { ...PARAMS_OK, licence_definie: false, licence_fin: null,
              mode_cle: 'integree', derniere_sauvegarde: null, dernier_statut: null },
    destinations: [], journal: [],
  });
  await pause(350);
  const txt = sel => (d.querySelector(sel)?.textContent || '').replace(/\s+/g, ' ').trim();
  const alertes = txt('#alertes');

  v('sans destination, l\'écran le dit crûment',
    alertes.includes('ne sont pas sauvegardées'));
  v('l\'alerte est de niveau danger, pas un simple avertissement',
    !!d.querySelector('#alertes .alert.danger'));
  v('il conseille un support hors de l\'ordinateur', alertes.includes('hors de cet ordinateur'));
  v('sans licence, l\'écran le signale', alertes.includes('licence n\'est pas encore enregistrée'));
  v('il explique la conséquence',
    alertes.includes('commune à toutes les installations'));
  v('la liste vide invite à agir', txt('#liste-dest').includes('Ajouter un dossier'));
  v('l\'historique vide le dit', txt('#corps-journal').includes('Aucune sauvegarde'));
  v('la tuile de protection alerte sur la clé commune', txt('#tuiles').includes('commune'));
}

// ===========================================================================
// CAS 3 — poste secondaire : il ne doit pas prétendre sauvegarder
// ===========================================================================
{
  const { d } = monter({ params: { ...PARAMS_OK, cette_machine_est_serveur: false } });
  await pause(350);
  const alertes = (d.querySelector('#alertes')?.textContent || '').replace(/\s+/g, ' ');
  v('un poste secondaire est signalé comme tel', alertes.includes('poste secondaire'));
  v('il renvoie vers la machine qui héberge les données',
    alertes.includes('héberge les données'));
}

// ===========================================================================
// CAS 4 — protection par mot de passe : il faut le demander à l'utilisateur
// ===========================================================================
{
  const { w, d, appels } = monter({
    params: { ...PARAMS_OK, mode_cle: 'motdepasse', mot_de_passe_defini: true },
  });
  await pause(350);
  const visible = el => el && w.getComputedStyle(el).display !== 'none';
  v('la protection par mot de passe est annoncée',
    d.querySelector('#etat-protection').textContent.includes('mot de passe'));
  v('elle dit que Djigui non plus ne peut pas ouvrir les fichiers',
    d.querySelector('#etat-protection').textContent.includes('Djigui non plus'));

  d.querySelector('#btn-maintenant').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  // Le serveur ne détient pas le mot de passe : sans cette demande, la
  // sauvegarde manuelle échouerait sans que personne ne comprenne pourquoi.
  v('sauvegarder demande le mot de passe', visible(d.querySelector('#modal-secret')));
  d.querySelector('#secret-valeur').value = 'ma-phrase-secrete';
  d.querySelector('#btn-secret-ok').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(250);
  const exec = appels.find(a => a.chemin === '/api/sauvegarde/executer');
  v('le mot de passe part avec la demande',
    exec && exec.body.mot_de_passe === 'ma-phrase-secrete');
}

// ===========================================================================
// CAS 5 — la sauvegarde échoue : le détail par destination doit être montré
// ===========================================================================
{
  const { w, d, toasts, alerts } = monter({}, {
    statut: 'echec', nom_fichier: '', taille_octets: 0, verifiee: false,
    anciennes_supprimees: 0,
    destinations: [
      { libelle: 'Clé USB bleue', chemin: 'E:\\', reussi: false,
        message: 'Le dossier « E:\\ » est introuvable. Vérifiez qu\'il est bien branché.' },
    ],
    message: "Aucune copie n'a pu être écrite. Vos données ne sont PAS sauvegardées.",
  });
  await pause(350);
  d.querySelector('#btn-maintenant').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(250);
  v('un échec est annoncé en danger',
    toasts.some(t => t.t === 'danger' && t.msg.includes('PAS sauvegardées')));
  v('le détail par destination est montré',
    alerts.some(m => m.includes('Clé USB bleue') && m.includes('introuvable')));
  v('le bouton redevient utilisable après l\'échec',
    !d.querySelector('#btn-maintenant').disabled);
}

// ===========================================================================
// CAS 6 — le sélecteur de dossiers Windows (demande de l'utilisateur)
// ===========================================================================
//
// ⚠️ Il passe par le SERVEUR (`POST /api/sauvegarde/choisir-dossier`) et non
// par la coquille Tauri. Une première version utilisait `__TAURI__.core.invoke`
// et échouait : la fenêtre charge une URL distante (http://localhost:1704), et
// Tauri 2 refuse tout l'IPC dans ce cas — avec un message illisible par-dessus
// le marché (« undefined », Tauri rejetant avec une chaîne et non une Error).
{
  const { w, d, appels } = monter({}, undefined, 'E:\\Sauvegardes Djigui');
  await pause(350);
  const visible = el => el && w.getComputedStyle(el).display !== 'none';

  d.querySelector('#btn-add-dest').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(200);
  v('le bouton « Parcourir » est proposé', visible(d.querySelector('#btn-parcourir')));

  d.querySelector('#btn-parcourir').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(250);
  const appel = appels.find(a => a.chemin === '/api/sauvegarde/choisir-dossier');
  v('le sélecteur du serveur est appelé', !!appel && appel.method === 'POST');
  v('le dossier choisi remplit le champ',
    d.querySelector('#d-chemin').value === 'E:\\Sauvegardes Djigui');
  v('un nom reconnaissable est proposé',
    d.querySelector('#d-libelle').value === 'Sauvegardes Djigui (E:)');
  v('l\'explorateur de repli reste inutile', !visible(d.querySelector('#zone-explo')));
}

// Annulation du sélecteur : ce n'est pas une erreur, rien ne doit bouger.
{
  const { w, d, toasts } = monter({}, undefined, null);
  await pause(350);
  d.querySelector('#btn-add-dest').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(150);
  d.querySelector('#d-chemin').value = 'E:\\Deja saisi';
  d.querySelector('#btn-parcourir').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(250);
  v('annuler le sélecteur n\'efface pas ce qui était saisi',
    d.querySelector('#d-chemin').value === 'E:\\Deja saisi');
  v('annuler n\'affiche aucun message alarmant', toasts.length === 0);
  v('le bouton reste utilisable après une annulation',
    !d.querySelector('#btn-parcourir').disabled);
}

// Panne du sélecteur : l'utilisateur doit pouvoir FINIR ce qu'il a commencé,
// pas rester devant un bouton mort.
{
  const { w, d, appels, toasts } = monter({}, undefined, 'panne');
  await pause(350);
  const visible = el => el && w.getComputedStyle(el).display !== 'none';
  d.querySelector('#btn-add-dest').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(150);
  d.querySelector('#btn-parcourir').dispatchEvent(new w.Event('click', { bubbles: true }));
  await pause(300);
  v('une panne du sélecteur est expliquée, pas subie',
    toasts.some(t => t.msg.includes('liste ci-dessous')));
  v('l\'explorateur de repli prend le relais', visible(d.querySelector('#zone-explo')));
  v('et il est réellement chargé',
    appels.some(a => a.chemin.startsWith('/api/sauvegarde/parcourir')));
  const dossier = d.querySelector('[data-aller]');
  v('on peut alors choisir un dossier dans la liste', !!dossier);
  if (dossier) {
    dossier.dispatchEvent(new w.Event('click', { bubbles: true }));
    v('cliquer un dossier le choisit',
      d.querySelector('#d-chemin').value === 'E:\\Sauvegardes Djigui');
  }
}

console.log(`\nsauvegarde : ${ok.length}/${ok.length + ko.length} tests passés`);
if (ko.length) {
  console.log('\nÉCHECS :');
  ko.forEach(n => console.log('  ✗ ' + n));
  process.exit(1);
}
