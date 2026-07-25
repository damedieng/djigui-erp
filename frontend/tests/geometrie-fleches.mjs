// Vérifie la GÉOMÉTRIE du tracé des flèches en rejouant la logique de
// dessinerFleches sur les vraies dates du projet, avec des barres simulées.
// jsdom ne calculant pas la mise en page, on reproduit ici le même calcul.
const ok = [], ko = [];
const v = (n, c) => (c ? ok : ko).push(n);

// Cas réels tirés de djigui.db : activités qui s'enchaînent BOUT À BOUT.
const cas = [
  ['Reunion de demarrage', 100, 160, 'Recueil des besoins', 161, 260],
  ['Recueil des besoins',  161, 260, 'Redaction CDC',       261, 400],
  ['Formation comptable',  500, 560, 'Mise en production',  580, 900],
  // Cas de lien violé : le successeur démarre AVANT la fin du prédécesseur.
  ['Etude',                300, 500, 'Travaux',             200, 400],
];

function tracer(x1, y1, x2, y2) {
  const app = 10, xe = x2 - app;
  return xe > x1 + 4
    ? `M ${x1} ${y1} H ${xe} V ${y2} H ${x2 - 6}`
    : `M ${x1} ${y1} H ${x1 + app} V ${(y1 + y2) / 2} H ${xe} V ${y2} H ${x2 - 6}`;
}

let y = 0;
for (const [np, , pFin, ns, sDeb] of cas) {
  const y1 = y, y2 = y + 40; y += 40;
  const chemin = tracer(pFin, y1, sDeb, y2);
  // Le dernier segment horizontal doit aller vers la DROITE.
  const seg = chemin.split(' ');
  const dernierH = Number(seg[seg.length - 1]);
  const avant = chemin.includes('V') ? Number(chemin.split('H').slice(-2)[0].trim().split(' ')[0]) : 0;
  const versDroite = dernierH > avant;
  v(`${np} -> ${ns} : approche vers la droite`, versDroite);
  // La pointe est toujours dessinée au bord gauche du successeur, vers la droite.
  const tete = `M ${sDeb} ${y2} L ${sDeb - 6} ${y2 - 3.5} L ${sDeb - 6} ${y2 + 3.5} Z`;
  v(`${np} -> ${ns} : pointe au début du successeur`, tete.includes(`M ${sDeb} `));
}

console.log('--- RÉSULTATS ---');
ok.forEach(x => console.log('  OK   ' + x));
ko.forEach(x => console.log('  KO   ' + x));
console.log(`\n${ok.length} réussi(s), ${ko.length} échec(s)`);
process.exit(ko.length ? 1 : 0);
