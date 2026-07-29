// Jeu de démonstration RÉALISTE : marchés + projet, via l'API.
//
// Pourquoi par l'API et non par une migration : ces données sont une
// **démonstration**, pas un socle du logiciel. Elles doivent pouvoir être
// créées, montrées, puis retirées sans toucher au schéma. Tout passe donc par
// les mêmes routes que l'écran — ce qui teste l'application au passage.
//
// STRICTEMENT ADDITIF : rien n'est supprimé ni modifié dans l'existant. Chaque
// objet porte un marqueur dans ses observations pour pouvoir être retrouvé.
//
// Usage :
//   node seed-demo-marches-projets.mjs [port]            → crée
//   node seed-demo-marches-projets.mjs [port] --purger   → retire la démo
//
// Contexte retenu : une collectivité / ONG au Sénégal (région de Matam), qui
// passe ses marchés de travaux, fournitures et services. Montants, délais et
// entreprises sont calés sur des ordres de grandeur réels du secteur.

const port = (process.argv[2] || '1704').replace(/\D/g, '') || '1704';
const PURGER = process.argv.includes('--purger');
const B = `http://127.0.0.1:${port}/api`;
const MARQUE = '[demo-djigui]';

const ACTEUR = '543bbf1d-33e1-4f22-92c1-71ca1cbf9457';   // Administrateur

async function api(chemin, methode = 'GET', corps) {
  const r = await fetch(B + chemin, {
    method: methode,
    headers: { 'Content-Type': 'application/json', 'X-Utilisateur': ACTEUR },
    body: corps === undefined ? undefined : JSON.stringify(corps),
  });
  const texte = await r.text();
  if (!r.ok) throw new Error(`${methode} ${chemin} → ${r.status} ${texte.slice(0, 300)}`);
  return texte ? JSON.parse(texte) : null;
}

// Dates relatives à aujourd'hui : la démonstration doit rester crédible dans
// six mois, et surtout montrer de VRAIS retards par rapport à la date du jour.
const J = n => {
  const d = new Date(); d.setDate(d.getDate() + n);
  return d.toISOString().slice(0, 10);
};

// ===========================================================================
// Purge
// ===========================================================================
async function purger() {
  const marches = await api('/marches');
  let n = 0;
  for (const m of marches.filter(x => (x.observations || '').includes(MARQUE))) {
    try { await api('/marches/' + m.id, 'DELETE'); n++; }
    catch { await api(`/marches/${m.id}/annuler`, 'POST', { motif: 'Retrait du jeu de démonstration' }); }
  }
  const projets = await api('/projets');
  let p = 0;
  for (const x of projets.filter(y => (y.note || '').includes(MARQUE))) {
    await api('/projets/' + x.id, 'DELETE'); p++;
  }
  console.log(`Démo retirée : ${n} marché(s), ${p} projet(s).`);
}

// ===========================================================================
// Tiers : les entreprises qui soumissionnent
// ===========================================================================
// ⚠️ `code` est OBLIGATOIRE sur un tiers (clé métier lisible) : l'API refuse
// sans lui. `adresse` fait aussi partie du contrat.
const ENTREPRISES = [
  { code: 'F-SOGEA', nom: 'SOGEA SATOM Sénégal', ninea: '0021456789',
    telephone: '33 869 45 12', adresse: 'Km 4,5 bd du Centenaire, Dakar' },
  { code: 'F-NDIAYE', nom: 'Entreprise Ndiaye & Frères BTP', ninea: '0034567812',
    telephone: '77 645 33 21', adresse: 'Quartier Navel, Matam' },
  { code: 'F-CSE', nom: "CSE — Compagnie Sahélienne d'Entreprises", ninea: '0012398745',
    telephone: '33 839 92 00', adresse: 'Zone industrielle, Dakar' },
  { code: 'F-MSY', nom: 'Ets Mamadou Sy Distribution', ninea: '0045612378',
    telephone: '76 512 44 08', adresse: 'Marché central, Ourossogui' },
  { code: 'F-SMS', nom: 'Sénégalaise de Mobilier Scolaire', ninea: '0056781234',
    telephone: '33 951 20 74', adresse: 'Route de Thiès, Rufisque' },
  { code: 'F-BETD', nom: 'BET Diagne Ingénierie', ninea: '0067812345',
    telephone: '77 234 90 15', adresse: 'Sacré-Cœur 3, Dakar' },
];

async function creerTiers() {
  const existants = await api('/tiers');
  const carte = new Map(existants.map(t => [t.nom, t.id]));
  const out = new Map();
  for (const e of ENTREPRISES) {
    if (carte.has(e.nom)) { out.set(e.nom, carte.get(e.nom)); continue; }
    const t = await api('/tiers', 'POST', {
      code: e.code, nom: e.nom, type_role: 'fournisseur', nature: 'entreprise',
      ninea: e.ninea, telephone: e.telephone, adresse: e.adresse,
    });
    out.set(e.nom, t.id);
  }
  return out;
}

// ===========================================================================
// Les marchés — un par situation à démontrer
// ===========================================================================
async function creerMarches(tiers) {
  const types = await api('/marche-types');
  const t = code => (types.find(x => x.code === code) || {}).id || null;
  const cree = [];

  const nouveau = async (spec) => {
    const m = await api('/marches', 'POST', {
      ...spec, observations: `${spec.observations || ''} ${MARQUE}`.trim(),
    });
    cree.push(m);
    return m;
  };
  const etapes = async id => (await api('/marches/' + id)).etapes;
  const finir = async (id, jusqua, decalage = {}) => {
    const es = await etapes(id);
    for (let i = 0; i < jusqua && i < es.length; i++) {
      await api(`/marche-etapes/${es[i].id}`, 'PUT', {
        date_effective: decalage[i] || es[i].date_prevue,
        statut: 'termine',
      });
    }
    return await etapes(id);
  };

  // --- 1. En préparation : rien n'est encore lancé -------------------------
  await nouveau({
    objet: "Réhabilitation du poste de santé de Ourossogui",
    type_id: t('TRAVAUX'), montant_estime: 34_500_000, date_lancement: J(-6),
    lieu_execution: 'Ourossogui, département de Matam',
    observations: "Financement PNDL. Dossier en cours de préparation.",
  });

  // --- 2. Dépouillement en cours : 4 offres reçues, aucune retenue ---------
  const m2 = await nouveau({
    objet: "Construction de 6 salles de classe au CEM de Thilogne",
    type_id: t('TRAVAUX'), montant_estime: 78_000_000, date_lancement: J(-52),
    lieu_execution: 'Thilogne, département de Matam',
    observations: "Programme d'urgence éducation. Ouverture des plis effectuée.",
  });
  await finir(m2.id, 4);   // dossier, publication, réception des offres, ouverture des plis
  const offres = [
    ['SOGEA SATOM Sénégal', 81_450_000, 150, 78, 72],
    ['Entreprise Ndiaye & Frères BTP', 74_900_000, 180, 71, 84],
    ["CSE — Compagnie Sahélienne d'Entreprises", 88_200_000, 120, 85, 65],
    ['Ets Mamadou Sy Distribution', 69_300_000, 240, 52, 92],
  ];
  for (const [nom, montant, delai, nt, nf] of offres) {
    await api(`/marches/${m2.id}/soumissionnaires`, 'POST', {
      tiers_id: tiers.get(nom) || null, nom,
      montant_offre: montant, montant_offre_ttc: Math.round(montant * 1.18),
      delai_jours: delai, note_technique: nt, note_financiere: nf,
      // La moins-disante est écartée : offre anormalement basse, capacité
      // technique insuffisante. C'est le cas d'école du dépouillement.
      statut: nf === 92 ? 'non_conforme' : 'conforme',
      motif: nf === 92 ? "Offre anormalement basse ; capacité technique non justifiée" : null,
      date_depot: J(-24),
    });
  }

  // --- 3. En cours d'exécution, AVEC AVENANTS et du retard ----------------
  const m3 = await nouveau({
    objet: "Construction du forage pastoral de Nabadji Civol",
    type_id: t('TRAVAUX'), montant_estime: 52_000_000, date_lancement: J(-140),
    lieu_execution: 'Nabadji Civol, département de Matam',
    observations: "Hydraulique rurale. Chantier démarré, sujétions imprévues rencontrées.",
  });
  const es3 = await etapes(m3.id);
  // Attribution réelle : l'entreprise retenue au dépouillement.
  const s3 = await api(`/marches/${m3.id}/soumissionnaires`, 'POST', {
    tiers_id: tiers.get('CSE — Compagnie Sahélienne d\'Entreprises'),
    nom: "CSE — Compagnie Sahélienne d'Entreprises",
    montant_offre: 54_800_000, delai_jours: 180, note_technique: 82, note_financiere: 74,
    statut: 'conforme', date_depot: J(-110),
  });
  await api(`/soumissionnaires/${s3.id}/attribuer`, 'POST', {});
  // Les 7 premières étapes sont franchies, avec des dérives de quelques jours —
  // c'est ce qui rend une frise crédible.
  await finir(m3.id, 7, { 0: J(-128), 2: J(-98), 4: J(-80), 6: J(-58) });
  // La 8e est en cours et A DÉPASSÉ sa date : c'est le retard à voir à l'écran.
  const es3b = await etapes(m3.id);
  if (es3b[7]) {
    await api(`/marche-etapes/${es3b[7].id}`, 'PUT',
      { date_prevue: J(-19), statut: 'en_cours',
        observations: "Notification transmise, ordre de service en attente de signature." });
  }
  // Deux avenants : un approuvé (nappe plus profonde), un encore en projet.
  const av1 = await api(`/marches/${m3.id}/avenants`, 'POST', {
    objet: "Approfondissement du forage de 12 m et fourniture de tubage supplémentaire",
    montant_variation: 6_240_000, delai_jours: 30, date_avenant: J(-45),
    motif: "Nappe atteinte plus profondément que prévu au dossier géotechnique.",
  });
  await api(`/avenants/${av1.id}/statut`, 'POST', { statut: 'approuve' });
  await api(`/marches/${m3.id}/avenants`, 'POST', {
    objet: "Extension du réseau de distribution vers le village de Sinthiou",
    montant_variation: 3_100_000, delai_jours: 21, date_avenant: J(-8),
    motif: "Demande du comité de gestion, en attente de validation du bailleur.",
  });

  // --- 4. Fournitures livrées, RÉCEPTIONNÉES AVEC RÉSERVES ----------------
  const m4 = await nouveau({
    objet: "Fourniture de 400 tables-bancs pour les écoles élémentaires",
    type_id: t('FOURNITURES'), montant_estime: 18_000_000, date_lancement: J(-190),
    lieu_execution: 'Inspection d\'académie de Matam',
    observations: "Marché à commandes. Livraison en deux vagues.",
  });
  const s4 = await api(`/marches/${m4.id}/soumissionnaires`, 'POST', {
    tiers_id: tiers.get('Sénégalaise de Mobilier Scolaire'),
    nom: 'Sénégalaise de Mobilier Scolaire',
    montant_offre: 17_600_000, delai_jours: 90, note_technique: 76, note_financiere: 88,
    statut: 'conforme', date_depot: J(-165),
  });
  await api(`/soumissionnaires/${s4.id}/attribuer`, 'POST', {});
  await finir(m4.id, 8);
  await api(`/marches/${m4.id}/receptions`, 'POST', {
    type_reception: 'provisoire', date_reception: J(-34), resultat: 'avec_reserves',
    reserves: "38 tables-bancs présentent un défaut de vernis ; 12 assises mal fixées. "
            + "Reprise demandée sous 30 jours.",
    garantie_mois: 12, montant_retenue_garantie: 880_000,
    receptionne_par: "Commission de réception — IA Matam",
    observations: "Procès-verbal signé contradictoirement.",
  });

  // --- 5. Terminé proprement : provisoire + définitive, réserves levées ----
  const m5 = await nouveau({
    objet: "Étude d'impact environnemental du périmètre irrigué de Dondou",
    type_id: t('INTELLECT'), montant_estime: 12_500_000, date_lancement: J(-300),
    lieu_execution: 'Dondou, vallée du fleuve Sénégal',
    observations: "Étude préalable exigée par le bailleur.",
  });
  const s5 = await api(`/marches/${m5.id}/soumissionnaires`, 'POST', {
    tiers_id: tiers.get('BET Diagne Ingénierie'), nom: 'BET Diagne Ingénierie',
    montant_offre: 12_150_000, delai_jours: 120, note_technique: 91, note_financiere: 79,
    statut: 'conforme', date_depot: J(-270),
  });
  await api(`/soumissionnaires/${s5.id}/attribuer`, 'POST', {});
  await finir(m5.id, 9);
  const r5 = await api(`/marches/${m5.id}/receptions`, 'POST', {
    type_reception: 'provisoire', date_reception: J(-120), resultat: 'avec_reserves',
    reserves: "Volet socio-économique à compléter (enquête ménages incomplète).",
    garantie_mois: 6, montant_retenue_garantie: 607_500,
    receptionne_par: "Comité de pilotage",
  });
  await api(`/receptions/${r5.id}/lever-reserves`, 'POST', { date: J(-95) });
  await api(`/marches/${m5.id}/receptions`, 'POST', {
    type_reception: 'definitive', date_reception: J(-60), resultat: 'prononcee',
    receptionne_par: "Comité de pilotage",
    observations: "Rapport final validé. Retenue de garantie libérée.",
  });
  await api(`/marches/${m5.id}/statut`, 'POST', { statut: 'realise' });

  // --- 6. Suspendu : le cas qu'on oublie toujours de tester ---------------
  const m6 = await nouveau({
    objet: "Entretien routier de la piste Matam — Ogo (14 km)",
    type_id: t('SERVICES'), montant_estime: 26_800_000, date_lancement: J(-75),
    lieu_execution: 'Axe Matam — Ogo',
    observations: "Suspendu en attendant la confirmation de la ligne budgétaire.",
  });
  await finir(m6.id, 3);
  await api(`/marches/${m6.id}/statut`, 'POST', { statut: 'suspendu' });

  // --- 7. Annulé avec motif : l'histoire ne s'efface pas ------------------
  const m7 = await nouveau({
    objet: "Acquisition d'un véhicule de liaison 4x4",
    type_id: t('FOURNITURES'), montant_estime: 31_000_000, date_lancement: J(-100),
    observations: "Procédure relancée ultérieurement.",
  });
  await finir(m7.id, 2);
  await api(`/marches/${m7.id}/annuler`, 'POST', {
    motif: "Crédits redéployés vers l'urgence hydraulique par arrêté du 12 du mois.",
  });

  return cree;
}

// ===========================================================================
// Le projet — pour voir les retards sur le Gantt
// ===========================================================================
async function creerProjet() {
  const p = await api('/projets', 'POST', {
    nom: "Programme d'hydraulique villageoise — Matam 2026",
    statut: 'en_cours',
    date_debut_prevue: J(-120), date_fin_prevue: J(120),
    budget_global: 210_000_000,
    note: `Programme financé sur ressources propres et appui bailleur. ${MARQUE}`,
  });

  // 3 phases, chacune avec ses activités. Les dates sont choisies pour que
  // certaines activités soient EN RETARD par rapport à aujourd'hui : c'est
  // précisément ce que le planning doit rendre visible.
  const phases = [
    ["Phase 1 — Études et préparation", J(-120), J(-40), [
      ["Études géophysiques des sites", J(-120), J(-95), 'terminee', 100],
      ["Élaboration des dossiers d'appel d'offres", J(-94), J(-70), 'terminee', 100],
      ["Validation environnementale", J(-69), J(-40), 'terminee', 100],
    ]],
    ["Phase 2 — Travaux de forage", J(-39), J(60), [
      // En retard : fin dépassée, pas terminée.
      ["Forage de Nabadji Civol", J(-39), J(-12), 'en_cours', 75],
      ["Forage de Dondou", J(-30), J(-5), 'en_cours', 60],
      // Bloquée ET en retard : le cas le plus parlant.
      ["Équipement en pompes solaires", J(-4), J(25), 'bloquee', 15],
      ["Construction des châteaux d'eau", J(10), J(60), 'a_faire', 0],
    ]],
    ["Phase 3 — Mise en service", J(61), J(120), [
      ["Raccordement des bornes-fontaines", J(61), J(90), 'a_faire', 0],
      ["Formation des comités de gestion", J(85), J(105), 'a_faire', 0],
      ["Réception et transfert aux communes", J(106), J(120), 'a_faire', 0],
    ]],
  ];

  let budgets = [0, 18_000_000, 0];
  let i = 0;
  for (const [nom, debut, fin, activites] of phases) {
    const parent = await api('/taches', 'POST', {
      projet_id: p.id, nom, date_debut_prevue: debut, date_fin_prevue: fin,
      statut: 'en_cours', avancement: 0, budget: 0,
    });
    const couts = [
      [6_500_000, 4_200_000, 3_100_000],
      [38_000_000, 36_500_000, 44_000_000, 52_000_000],
      [12_000_000, 5_400_000, 3_800_000],
    ][i];
    let k = 0;
    for (const [an, ad, af, st, av] of activites) {
      await api('/taches', 'POST', {
        projet_id: p.id, tache_parente_id: parent.id, nom: an,
        date_debut_prevue: ad, date_fin_prevue: af,
        statut: st, avancement: av, budget: couts[k++],
      });
    }
    i++;
  }
  void budgets;
  return p;
}

// ===========================================================================
(async () => {
  try {
    if (PURGER) { await purger(); return; }

    console.log('Création du jeu de démonstration…\n');
    const tiers = await creerTiers();
    console.log(`  ${tiers.size} entreprise(s) disponibles`);

    const marches = await creerMarches(tiers);
    console.log(`  ${marches.length} marché(s) créés`);

    const projet = await creerProjet();
    console.log(`  projet « ${projet.nom} »`);

    // Récapitulatif lu depuis l'API : on montre ce que l'application RENVOIE,
    // pas ce qu'on croit avoir écrit.
    console.log('\n=== MARCHÉS ===');
    for (const m of await api('/marches')) {
      if (!(m.observations || '').includes(MARQUE)) continue;
      const bits = [
        m.numero, m.statut.padEnd(9),
        `${m.avancement}%`.padStart(4),
        `${(m.montant_courant || 0).toLocaleString('fr-FR')} F`.padStart(18),
      ];
      if (m.retard_jours != null) bits.push(`RETARD ${m.retard_jours}j`);
      if (m.nb_avenants) bits.push(`${m.nb_avenants} avenant(s)`);
      if (m.reserves_ouvertes) bits.push('RÉSERVES');
      console.log('  ' + bits.join('  ') + '\n      ' + m.objet);
      for (const a of m.alertes || []) console.log('      ⚠ ' + a);
    }

    console.log('\n=== PROJET ===');
    const taches = await api(`/projets/${projet.id}/taches`);
    for (const t of taches) {
      const dec = '  '.repeat(t.niveau);
      const r = t.retard_jours != null ? `  ← RETARD ${t.retard_jours} j` : '';
      console.log(`  ${dec}${t.nom.padEnd(42 - dec.length)} ${String(t.avancement_calcule).padStart(3)}%${r}`);
    }
    const enRetard = taches.filter(t => t.retard_jours != null && !t.a_enfants);
    console.log(`\n  ${enRetard.length} activité(s) en retard à voir sur le Gantt.`);
    console.log('\nPour retirer la démo : node seed-demo-marches-projets.mjs ' + port + ' --purger');
  } catch (e) {
    console.error('\nÉCHEC : ' + e.message);
    process.exit(1);
  }
})();
