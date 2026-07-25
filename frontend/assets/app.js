// Djigui — helpers frontend partagés. L'UI parle à l'API du serveur local (§2.1).
// En mode client, le serveur est sur un autre poste : on pourra surcharger la
// base via window.DJIGUI_API ou localStorage('djigui_api').
const Djigui = (() => {
  const base = window.DJIGUI_API || localStorage.getItem('djigui_api') || '';

  async function api(chemin, { method = 'GET', body } = {}) {
    const opts = { method, headers: {} };
    if (body !== undefined) {
      opts.headers['Content-Type'] = 'application/json';
      opts.body = JSON.stringify(body);
    }
    // Utilisateur courant → traçabilité serveur (audit + auteur des pièces).
    try {
      const u = JSON.parse(sessionStorage.getItem('djigui_user') || 'null');
      if (u && u.id) opts.headers['X-Utilisateur-Id'] = u.id;
    } catch { /* pas de session : action anonyme */ }
    const r = await fetch(base + chemin, opts);
    const txt = await r.text();
    const data = txt ? JSON.parse(txt) : null;
    if (!r.ok) throw new Error((data && data.erreur) || `HTTP ${r.status}`);
    return data;
  }

  // Nombres à la sénégalaise : séparateur de milliers par espace, sans décimale superflue.
  const fmt = n => (Number(n) || 0).toLocaleString('fr-FR').replace(/ /g, ' ');

  const esc = s => String(s ?? '').replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  // Date à l'affichage au format français : « 2026-10-10 » → « 10/10/2026 ».
  // Accepte une date ISO (YYYY-MM-DD) ou un datetime ISO (on garde la partie date).
  const dateFr = s => {
    if (!s) return '';
    const m = String(s).slice(0, 10).match(/^(\d{4})-(\d{2})-(\d{2})$/);
    return m ? `${m[3]}/${m[2]}/${m[1]}` : String(s);
  };

  // Messages de retour (toast). type : 'ok' | 'warn' | 'danger'. Pour le
  // feedback des opérations, notamment les traitements par lot.
  function toast(message, type = 'ok') {
    let hote = document.getElementById('djigui-toasts');
    if (!hote) {
      hote = document.createElement('div');
      hote.id = 'djigui-toasts';
      document.body.appendChild(hote);
    }
    const el = document.createElement('div');
    el.className = `toast ${type}`;
    el.textContent = message;
    hote.appendChild(el);
    requestAnimationFrame(() => el.classList.add('show'));
    setTimeout(() => { el.classList.remove('show'); setTimeout(() => el.remove(), 250); }, 3200);
  }

  // Impression isolée : on écrit le contenu dans un iframe caché et on imprime
  // CE document. La fenêtre principale de l'application n'est pas affectée
  // (contrairement à window.print() sur la page, qui pouvait fermer l'appli).
  const STYLE_TICKET = `
    body { margin: 0; }
    .recu-ticket { width: 280px; font-family: "Consolas","Courier New",monospace; color:#000; font-size:12px; line-height:1.5; }
    .recu-ticket .r-c { text-align:center; }
    .recu-ticket .r-sep { border-top:1px dashed #000; margin:6px 0; }
    .recu-ticket .r-row { display:flex; justify-content:space-between; gap:8px; }
    .recu-ticket .r-big { font-size:14px; font-weight:700; }
    .recu-ticket table { width:100%; border-collapse:collapse; }
    .recu-ticket td { padding:1px 0; vertical-align:top; }
    .recu-ticket .r-right { text-align:right; white-space:nowrap; }
    @page { margin: 6mm; }`;

  function imprimer(htmlInterne) {
    const iframe = document.createElement('iframe');
    iframe.setAttribute('aria-hidden', 'true');
    iframe.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0;';
    document.body.appendChild(iframe);
    const doc = iframe.contentWindow.document;
    doc.open();
    doc.write(`<!DOCTYPE html><html><head><meta charset="utf-8"><style>${STYLE_TICKET}</style></head><body>${htmlInterne}</body></html>`);
    doc.close();
    // laisse le rendu se faire, puis imprime le document de l'iframe
    setTimeout(() => {
      try { iframe.contentWindow.focus(); iframe.contentWindow.print(); }
      catch (e) { console.error('impression', e); }
      setTimeout(() => iframe.remove(), 1500);
    }, 120);
  }

  // ---- Select cherchable (façon « Chosen », sans dépendance) --------------
  // Monte dans `mount` un sélecteur : contrôle cliquable + liste déroulante avec
  // recherche. `items` = [{id, label, sub?}]. Renvoie un objet { value, setValue,
  // setItems }. Idéal pour choisir un article partout dans l'appli.
  function selectRecherche(mount, opts = {}) {
    const state = { items: opts.items || [], value: opts.value || '', hl: -1, filt: [] };
    const placeholder = opts.placeholder || 'Choisir…';
    mount.classList.add('dj-select');
    mount.innerHTML =
      `<div class="dj-select-control placeholder"><span class="dj-select-lbl"></span><i class="ti ti-selector"></i></div>
       <div class="dj-select-drop" hidden>
         <div class="dj-select-search"><input type="text" placeholder="Rechercher…"></div>
         <div class="dj-select-list"></div>
       </div>`;
    const control = mount.querySelector('.dj-select-control');
    const lbl = mount.querySelector('.dj-select-lbl');
    const drop = mount.querySelector('.dj-select-drop');
    const search = mount.querySelector('.dj-select-search input');
    const list = mount.querySelector('.dj-select-list');

    const labelOf = id => { const it = state.items.find(x => x.id === id); return it ? it.label : ''; };
    function majControl() {
      const l = labelOf(state.value);
      lbl.textContent = l || placeholder;
      control.classList.toggle('placeholder', !l);
    }
    function rendreListe() {
      const q = search.value.trim().toLowerCase();
      state.filt = state.items.filter(it =>
        !q || it.label.toLowerCase().includes(q) || (it.sub || '').toLowerCase().includes(q));
      list.innerHTML = state.filt.length ? state.filt.map((it, i) =>
        `<div class="dj-select-opt ${i === state.hl ? 'hl' : ''}" data-i="${i}">
           <span>${esc(it.label)}</span>${it.sub ? `<span class="sub">${esc(it.sub)}</span>` : ''}</div>`).join('')
        : '<div class="dj-select-vide">Aucun résultat</div>';
    }
    const ouvrir = () => { drop.hidden = false; mount.classList.add('open'); search.value = ''; state.hl = -1; rendreListe(); search.focus(); };
    const fermer = () => { drop.hidden = true; mount.classList.remove('open'); };
    function choisir(i) {
      const it = state.filt[i]; if (!it) return;
      state.value = it.id; majControl(); fermer();
      if (opts.onChange) opts.onChange(it.id, it);
    }
    control.addEventListener('click', () => drop.hidden ? ouvrir() : fermer());
    search.addEventListener('input', () => { state.hl = -1; rendreListe(); });
    search.addEventListener('keydown', e => {
      if (e.key === 'ArrowDown') { e.preventDefault(); state.hl = Math.min(state.filt.length - 1, state.hl + 1); rendreListe(); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); state.hl = Math.max(0, state.hl - 1); rendreListe(); }
      else if (e.key === 'Enter') { e.preventDefault(); choisir(state.hl < 0 ? 0 : state.hl); }
      else if (e.key === 'Escape') { fermer(); }
    });
    list.addEventListener('click', e => { const o = e.target.closest('.dj-select-opt'); if (o) choisir(Number(o.dataset.i)); });
    document.addEventListener('click', e => { if (!mount.contains(e.target)) fermer(); });

    majControl();
    return {
      get value() { return state.value; },
      setValue(id) { state.value = id || ''; majControl(); },
      setItems(items) { state.items = items || []; majControl(); },
    };
  }

  // ---- Dialogues thématisés (remplacent alert/confirm natifs) -------------
  // Modale avec en-tête aux couleurs du thème. `confirm` renvoie une Promise<bool>,
  // `alert` une Promise résolue à la fermeture. Usage : await Djigui.confirm('…').
  function dialogue({ titre, message, mode, okLabel, danger }) {
    return new Promise(resolve => {
      const ov = document.createElement('div');
      ov.className = 'modal-overlay dlg-overlay';
      const corpsHtml = esc(message).replace(/\n/g, '<br>');
      ov.innerHTML =
        `<div class="modal dlg" role="dialog" aria-modal="true">
           <div class="dlg-head ${danger ? 'danger' : ''}">
             <i class="ti ${danger ? 'ti-alert-triangle' : (mode === 'confirm' ? 'ti-help-circle' : 'ti-info-circle')}"></i>
             <span>${esc(titre)}</span>
           </div>
           <div class="dlg-body">${corpsHtml}</div>
           <div class="dlg-actions">
             ${mode === 'confirm' ? '<button type="button" class="btn dlg-annuler">Annuler</button>' : ''}
             <button type="button" class="btn ${danger ? 'btn-danger' : 'btn-primary'} dlg-ok">${esc(okLabel)}</button>
           </div>
         </div>`;
      document.body.appendChild(ov);
      const fin = val => { document.removeEventListener('keydown', onKey); ov.remove(); resolve(val); };
      const onKey = ev => {
        if (ev.key === 'Escape') fin(mode === 'confirm' ? false : true);
        else if (ev.key === 'Enter') fin(true);
      };
      ov.querySelector('.dlg-ok').addEventListener('click', () => fin(true));
      const annuler = ov.querySelector('.dlg-annuler');
      if (annuler) annuler.addEventListener('click', () => fin(false));
      document.addEventListener('keydown', onKey);
      ov.querySelector('.dlg-ok').focus();
    });
  }
  const confirmer = (message, opts = {}) => dialogue({
    titre: opts.titre || 'Confirmation', message, mode: 'confirm',
    okLabel: opts.okLabel || 'Confirmer', danger: opts.danger,
  });
  const alerte = (message, opts = {}) => dialogue({
    titre: opts.titre || 'Information', message, mode: 'alert',
    okLabel: 'OK', danger: opts.danger,
  });

  // ---- Session utilisateur (login au démarrage) --------------------------
  // Stockée en sessionStorage : effacée à la fermeture de l'appli → on se
  // reconnecte à chaque démarrage. Persiste pendant la navigation entre écrans.
  const CLE_USER = 'djigui_user';
  const user = () => { try { return JSON.parse(sessionStorage.getItem(CLE_USER) || 'null'); } catch { return null; } };
  const setUser = u => sessionStorage.setItem(CLE_USER, JSON.stringify(u));
  const logout = () => { sessionStorage.removeItem(CLE_USER); location.replace('login.html'); };
  const estAdmin = () => { const u = user(); return !!u && u.role === 'admin'; };

  return { api, fmt, esc, dateFr, toast, imprimer, confirm: confirmer, alert: alerte,
           selectRecherche, user, setUser, logout, estAdmin };
})();

// ---- Garde d'authentification -------------------------------------------
// ===========================================================================
// Barre latérale CENTRALISÉE (2026-07-25)
//
// Avant, le menu était recopié en dur dans les 14 pages : ajouter un écran
// obligeait à modifier 14 fichiers, et on en oubliait — trois divergences
// constatées (État de caisse, Magasins, puis Agenda/Projets). Désormais il
// existe à un seul endroit : ci-dessous.
//
// Pour ajouter une entrée : une ligne dans MENU. Rien d'autre.
//   { href, icone, libelle }          entrée normale
//   { href, icone, libelle, admin:1 } réservée aux administrateurs
//   { groupe: 'Titre' }               séparateur de section
//   { pied: 1 }                       tout ce qui suit va en bas de la barre
// ===========================================================================
const MENU = [
  { href: 'accueil.html', icone: 'ti-home', libelle: 'Accueil' },
  { groupe: 'Commerce' },
  { href: 'documents.html?sens=vente', icone: 'ti-shopping-cart', libelle: 'Ventes' },
  { href: 'caisse.html', icone: 'ti-cash', libelle: 'Caisse' },
  { href: 'caisse-etat.html', icone: 'ti-wallet', libelle: 'État de caisse' },
  { href: 'documents.html', icone: 'ti-file-invoice', libelle: 'Factures' },
  { href: 'abonnements.html', icone: 'ti-repeat', libelle: 'Abonnements' },
  { href: 'agenda.html', icone: 'ti-calendar-event', libelle: 'Agenda' },
  { href: 'projets.html', icone: 'ti-briefcase', libelle: 'Projets' },
  { href: 'documents.html?sens=achat', icone: 'ti-truck', libelle: 'Achats' },
  { groupe: 'Catalogue' },
  { href: 'articles.html', icone: 'ti-box', libelle: 'Articles' },
  { href: 'magasins.html', icone: 'ti-building-warehouse', libelle: 'Magasins', admin: 1 },
  { href: '#', icone: 'ti-tools', libelle: 'Production' },
  { groupe: 'Contacts' },
  { href: 'tiers.html', icone: 'ti-users', libelle: 'Tiers' },
  { href: '#', icone: 'ti-chart-bar', libelle: 'Rapports' },
  { pied: 1 },
  { href: 'utilisateurs.html', icone: 'ti-user-shield', libelle: 'Utilisateurs', admin: 1 },
  { href: 'journal-audit.html', icone: 'ti-history', libelle: "Journal d'audit", admin: 1 },
  { href: 'parametres.html', icone: 'ti-settings', libelle: 'Paramètres' },
];

// Quelles entrées s'allument sur quelle page. `projet-detail.html` doit
// allumer « Projets », et `documents.html` change de sens selon le paramètre.
function entreeActive(href) {
  const page = location.pathname.split('/').pop() || 'accueil.html';
  const sens = new URLSearchParams(location.search).get('sens') || '';
  const cible = href.split('?')[0];
  const sensCible = (href.split('?')[1] || '').replace('sens=', '');
  if (page === 'projet-detail.html') return cible === 'projets.html';
  if (page === 'facture.html') return cible === 'documents.html' && !sensCible;
  if (cible !== page) return false;
  // Ventes / Factures / Achats pointent tous vers documents.html.
  return sensCible === sens;
}

function construireMenu() {
  const aside = document.querySelector('.sidebar');
  if (!aside || aside.dataset.rempli) return;
  aside.dataset.rempli = '1';
  const admin = Djigui.estAdmin();

  let html = `<div class="brand">
      <div class="brand-mark"><img src="assets/logo-djigui.png" alt="Djigui"></div>
      <div><div class="brand-name">Djigui</div><div class="brand-sub">Gestion commerciale</div></div>
    </div>`;
  let pied = '';
  let auPied = false;
  for (const e of MENU) {
    if (e.pied) { auPied = true; continue; }
    if (e.groupe) { html += `<div class="nav-label">${Djigui.esc(e.groupe)}</div>`; continue; }
    // Les entrées réservées disparaissent purement pour un non-admin.
    if (e.admin && !admin) continue;
    const actif = entreeActive(e.href);
    // L'entrée active n'est pas un lien : on ne recharge pas la page courante.
    const lien = actif ? '' : ` href="${e.href}"`;
    const ligne = `<a class="nav-item${actif ? ' active' : ''}"${lien}${e.admin ? ' data-admin' : ''}>` +
      `<i class="ti ${e.icone}"></i>${Djigui.esc(e.libelle)}</a>`;
    if (auPied) pied += ligne; else html += ligne;
  }
  aside.innerHTML = html + `<div class="sidebar-foot">${pied}</div>`;
}

// ===========================================================================
// Sections d'aide repliables (2026-07-25)
//
// Chaque écran porte une aide en langage simple — c'est une exigence du projet.
// Mais elle prend beaucoup de place une fois qu'on connaît l'écran : elle est
// donc REPLIÉE par défaut, et son état est retenu par écran.
// Centralisé ici : les 14 pages en profitent sans être modifiées.
// ===========================================================================
function aidesRepliables() {
  const page = location.pathname.split('/').pop() || 'accueil.html';
  document.querySelectorAll('.aide').forEach((aide, i) => {
    const titre = aide.querySelector('.aide-titre');
    if (!titre || aide.dataset.repliable) return;
    aide.dataset.repliable = '1';
    const cle = `aide-ouverte:${page}:${i}`;
    // Repliée par défaut : seule une ouverture explicite est mémorisée.
    const ouverte = localStorage.getItem(cle) === '1';
    aide.classList.toggle('repliee', !ouverte);
    const fleche = document.createElement('i');
    fleche.className = 'ti ti-chevron-down aide-fleche';
    titre.appendChild(fleche);
    titre.setAttribute('role', 'button');
    titre.setAttribute('tabindex', '0');
    const basculer = () => {
      const fermee = aide.classList.toggle('repliee');
      localStorage.setItem(cle, fermee ? '0' : '1');
    };
    titre.addEventListener('click', basculer);
    titre.addEventListener('keydown', e => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); basculer(); }
    });
  });
}

// ===========================================================================
// Notifications du jour (2026-07-25)
//
// Une cloche dans la barre du haut, sur TOUTES les pages. Le contenu est
// recalculé par le serveur à chaque ouverture : pas d'alerte périmée.
// Sources : projets et activités en retard, jalons, livrables, liens de
// précédence, rendez-vous du jour, stock bas, abonnements à facturer,
// caisses non fermées.
// ===========================================================================
const GRAVITES = {
  urgent:    ['#c0392b', 'ti-alert-triangle-filled'],
  attention: ['#b8860b', 'ti-alert-circle'],
  info:      ['#2f7fd1', 'ti-info-circle'],
};

async function chargerNotifications(panneau, pastille) {
  try {
    const liste = await Djigui.api('/api/notifications');
    const nouvelles = liste.filter(n => !n.lu);
    // La pastille ne compte que ce qui n'a pas encore été vu.
    pastille.textContent = nouvelles.length > 99 ? '99+' : String(nouvelles.length);
    pastille.hidden = nouvelles.length === 0;

    if (!liste.length) {
      panneau.querySelector('.notif-corps').innerHTML =
        '<div class="notif-vide"><i class="ti ti-checks"></i><div>Rien à signaler aujourd\'hui.</div></div>';
      return liste;
    }
    // Regroupement par catégorie, dans l'ordre déjà trié par le serveur.
    const groupes = new Map();
    for (const n of liste) {
      if (!groupes.has(n.categorie)) groupes.set(n.categorie, []);
      groupes.get(n.categorie).push(n);
    }
    panneau.querySelector('.notif-corps').innerHTML = [...groupes].map(([cat, items]) => `
      <div class="notif-groupe">${Djigui.esc(cat)}</div>
      ${items.map(n => {
        const [coul, icone] = GRAVITES[n.gravite] || GRAVITES.info;
        return `<a class="notif-item${n.lu ? ' lu' : ''}" href="${n.lien}" data-cle="${Djigui.esc(n.cle)}">
          <i class="ti ${icone}" style="color:${coul}"></i>
          <div class="notif-texte">
            <div class="notif-titre">${Djigui.esc(n.titre)}</div>
            <div class="notif-detail">${Djigui.esc(n.detail)}</div>
          </div></a>`;
      }).join('')}`).join('');
    return liste;
  } catch (e) {
    panneau.querySelector('.notif-corps').innerHTML =
      `<div class="notif-vide">Notifications indisponibles.<br><span class="muted">${Djigui.esc(e.message)}</span></div>`;
    return [];
  }
}

function clocheNotifications() {
  const barre = document.querySelector('.topbar-right');
  if (!barre || document.getElementById('notif-cloche')) return;

  const enveloppe = document.createElement('div');
  enveloppe.className = 'notif-wrap';
  enveloppe.innerHTML = `
    <button class="icon-btn notif-bouton" id="notif-cloche" title="Notifications du jour">
      <i class="ti ti-bell"></i><span class="notif-pastille" id="notif-pastille" hidden>0</span>
    </button>
    <div class="notif-panneau" id="notif-panneau" hidden>
      <div class="notif-tete">
        <span>Aujourd'hui</span>
        <button class="btn-lien" id="notif-tout-lu">Tout marquer comme lu</button>
      </div>
      <div class="notif-corps"><div class="notif-vide">Chargement…</div></div>
      <div class="notif-pied"><button class="btn-lien" id="notif-reafficher">Réafficher les alertes masquées</button></div>
    </div>`;
  // Avant le bloc utilisateur, pour rester à droite du titre.
  barre.insertBefore(enveloppe, barre.firstChild);

  const panneau = document.getElementById('notif-panneau');
  const pastille = document.getElementById('notif-pastille');
  let liste = [];

  const rafraichir = async () => { liste = await chargerNotifications(panneau, pastille); };

  const fermer = () => { panneau.hidden = true; };
  const basculer = () => {
    const ouvrir = panneau.hidden;
    panneau.hidden = !ouvrir;
    if (ouvrir) rafraichir();
  };
  // La cloche ouvre ET referme. On écoute sur l'enveloppe pour attraper aussi
  // les clics sur l'icône ou la pastille, qui sont à l'intérieur du bouton.
  enveloppe.addEventListener('click', e => {
    if (!e.target.closest('#notif-cloche')) return;   // clic dans le panneau
    e.preventDefault();
    e.stopPropagation();
    basculer();
  });
  // Un clic ailleurs referme. En phase de capture : même si un autre écran
  // arrête la propagation de ses propres clics, la fermeture reste garantie.
  document.addEventListener('click', e => {
    if (!panneau.hidden && !e.target.closest('.notif-wrap')) fermer();
  }, true);
  // Échap referme aussi : réflexe attendu pour tout panneau superposé.
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape') fermer();
  });

  // Ouvrir une notification la marque lue : on l'a traitée en allant voir.
  panneau.addEventListener('click', async e => {
    const item = e.target.closest('.notif-item');
    if (!item) return;
    try { await Djigui.api('/api/notifications/lues', { method: 'POST', body: { cles: [item.dataset.cle] } }); }
    catch { /* la navigation prime : on n'empêche pas le clic */ }
  });

  document.getElementById('notif-tout-lu').addEventListener('click', async () => {
    const cles = liste.filter(n => !n.lu).map(n => n.cle);
    if (!cles.length) return;
    try { await Djigui.api('/api/notifications/lues', { method: 'POST', body: { cles } }); await rafraichir(); }
    catch (e) { Djigui.toast('Erreur : ' + e.message, 'danger'); }
  });
  document.getElementById('notif-reafficher').addEventListener('click', async () => {
    try { await Djigui.api('/api/notifications/reafficher', { method: 'POST' }); await rafraichir(); }
    catch (e) { Djigui.toast('Erreur : ' + e.message, 'danger'); }
  });

  // Premier calcul au chargement (pour la pastille), puis toutes les 5 minutes :
  // les données bougent pendant la journée (une vente, une tâche terminée…).
  rafraichir();
  setInterval(rafraichir, 5 * 60 * 1000);
}

// Toute page (sauf le login) exige un utilisateur connecté ; sinon on redirige
// vers l'écran de connexion.
(() => {
  const page = location.pathname.split('/').pop() || 'index.html';
  const publiques = ['login.html'];
  if (!publiques.includes(page) && !Djigui.user()) {
    location.replace('login.html');
    return;
  }
  // Une fois la page prête : afficher l'utilisateur + bouton déconnexion, et
  // masquer les entrées réservées aux administrateurs pour les caissiers.
  document.addEventListener('DOMContentLoaded', () => {
    const u = Djigui.user();
    if (!u) return;
    // Le menu est construit ici, à partir de la liste unique MENU.
    construireMenu();
    aidesRepliables();
    clocheNotifications();
    // Masque les éléments réservés admin si l'utilisateur n'est pas admin.
    if (u.role !== 'admin') {
      document.querySelectorAll('[data-admin]').forEach(el => el.remove());
    }
    // Injecte le bloc utilisateur + déconnexion dans la barre du haut.
    const barre = document.querySelector('.topbar-right');
    if (barre && !document.getElementById('djigui-user-chip')) {
      const chip = document.createElement('div');
      chip.id = 'djigui-user-chip';
      chip.className = 'user-chip';
      chip.innerHTML =
        `<div class="user-meta"><div class="user-nom">${Djigui.esc(u.nom)}</div>` +
        `<div class="user-role">${u.role === 'admin' ? 'Administrateur' : 'Caissier'}</div></div>` +
        `<button class="icon-btn" id="djigui-logout" title="Se déconnecter"><i class="ti ti-logout"></i></button>`;
      barre.appendChild(chip);
      document.getElementById('djigui-logout').addEventListener('click', () => {
        if (confirm('Se déconnecter ?')) Djigui.logout();
      });
    }
    // Bouton « hamburger » : replier / déplier la barre latérale (façon mobile),
    // pour gagner de la place (Gantt, caisse…). État mémorisé sur toutes les pages.
    const app = document.querySelector('.app');
    const topbar = document.querySelector('.topbar');
    if (app && topbar && topbar.firstElementChild && !document.getElementById('sb-toggle')) {
      if (localStorage.getItem('sidebar-collapsed') === '1') app.classList.add('sidebar-collapsed');
      const btn = document.createElement('button');
      btn.id = 'sb-toggle';
      btn.className = 'sb-toggle';
      btn.title = 'Afficher / masquer le menu';
      btn.innerHTML = '<i class="ti ti-menu-2"></i>';
      btn.addEventListener('click', () => {
        const now = app.classList.toggle('sidebar-collapsed');
        localStorage.setItem('sidebar-collapsed', now ? '1' : '0');
      });
      // Groupe le bouton avec le bloc titre (garde l'alignement de la topbar).
      const titre = topbar.firstElementChild;
      const wrap = document.createElement('div');
      wrap.style.cssText = 'display:flex;align-items:center;gap:12px';
      topbar.insertBefore(wrap, titre);
      wrap.appendChild(btn);
      wrap.appendChild(titre);
    }
  });
})();

// ---- Fermeture de l'application : confirmation thématisée (via Tauri) ----
// Le shell Tauri empêche la fermeture et émet « demande-fermeture » ; on montre
// une modale du thème, et on ne quitte que si l'utilisateur confirme.
(() => {
  // La confirmation de fermeture est désormais gérée NATIVEMENT par la coquille
  // Tauri (boîte Windows), et non plus par cette page : `emit` réussissant même
  // sans auditeur, la fenêtre pouvait refuser de se fermer si ce script n'était
  // pas prêt. Rien à faire ici.
  const T = window.__TAURI__;
  if (!T) return;
})();

// ---- Nom de la boutique : injecté partout depuis les paramètres ----------
// Remplit les éléments `.company-name` (nom) et `.company-ninea` (NINEA) sur
// toutes les pages, y compris l'écran de connexion. L'API est publique.
// Masque un identifiant (NINEA) : garde les 2 premiers et 2 derniers caractères.
function masquerNinea(v) {
  const s = String(v || '').trim();
  return s.length <= 4 ? s : s.slice(0, 2) + '***..' + s.slice(-2);
}
document.addEventListener('DOMContentLoaded', async () => {
  try {
    const p = await Djigui.api('/api/parametres');
    const nom = (p.raison_sociale || '').trim();
    if (nom) {
      document.querySelectorAll('.company-name').forEach(el => { el.textContent = nom; el.hidden = false; });
    }
    // Logo de la boutique : remplace le logo par défaut là où c'est demandé.
    const logo = (p.logo || '').trim();
    if (logo) {
      document.querySelectorAll('img.company-logo').forEach(img => { img.src = logo; });
    }
    // NINEA masqué dans les PAGES pour la confidentialité (ex. 2R2202558 → 2R***..58).
    // Les DOCUMENTS (facture.html) l'affichent en entier via leur propre rendu.
    document.querySelectorAll('.company-ninea').forEach(el => {
      const v = (p.ninea || '').trim();
      if (v) { el.textContent = 'NINEA ' + masquerNinea(v); el.hidden = false; } else { el.hidden = true; }
    });
  } catch { /* paramètres indisponibles : on garde l'affichage par défaut */ }
});

// Hors caisse, aucun ticket en attente : on remet le compteur du garde-fou de
// fermeture à zéro (la caisse le tient à jour de son côté).
if (!location.pathname.endsWith('caisse.html')) {
  Djigui.api('/api/etat/tickets-attente', { method: 'PUT', body: { n: 0 } }).catch(() => {});
}
