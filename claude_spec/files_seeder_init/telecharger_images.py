#!/usr/bin/env python3
"""
Djigui - Telechargement des images du seeder de catalogues.

Interroge l'API Openverse, filtre strictement sur les licences sans obligation
d'attribution (CC0 / domaine public), normalise en WebP 256x256, et journalise
la provenance de chaque fichier dans SOURCES.md.

Le script est IDEMPOTENT : un fichier deja present n'est jamais retelecharge.
On peut donc le relancer autant de fois que necessaire, seuls les manquants
sont traites.

Il ne garantit PAS la pertinence ni l'absence de marque sur l'image : la revue
visuelle via planche.html reste obligatoire. Voir IMAGES-SOURCING.md §3.

    pip install requests pillow

    python telecharger_images.py --sortie assets/catalogue/images/articles
    python telecharger_images.py --manquants
    python telecharger_images.py --seulement mafe --requete "peanut stew bowl"
"""

from __future__ import annotations

import argparse
import io
import json
import sys
import time
from dataclasses import dataclass, asdict
from pathlib import Path

try:
    import requests
    from PIL import Image
except ImportError:
    sys.exit("Dependances manquantes. Lancer : pip install requests pillow")


API = "https://api.openverse.org/v1/images/"
LICENCES = "cc0,pdm"          # sans obligation d'attribution
TAILLE = 256                  # cote de la tuile, cf. SEEDER-CATALOGUES.md §7.2
QUALITE = 80
UA = "Djigui-SeederImages/1.0 (catalogue seeding)"
PAUSE = 1.2                   # respect des quotas anonymes Openverse


@dataclass
class Provenance:
    code: str
    titre: str
    auteur: str
    licence: str
    url_source: str
    page_origine: str
    requete: str


def chercher(session: requests.Session, requete: str, jeton: str | None) -> list[dict]:
    """Retourne les candidats Openverse pour une requete, licences filtrees."""
    entetes = {"User-Agent": UA}
    if jeton:
        entetes["Authorization"] = f"Bearer {jeton}"
    params = {
        "q": requete,
        "license": LICENCES,
        "page_size": 8,
        "mature": "false",
        "aspect_ratio": "square,wide",
    }
    try:
        r = session.get(API, params=params, headers=entetes, timeout=25)
        if r.status_code == 429:
            print("    ! quota atteint, pause 30 s")
            time.sleep(30)
            return chercher(session, requete, jeton)
        r.raise_for_status()
        return r.json().get("results", [])
    except requests.RequestException as e:
        print(f"    ! recherche echouee : {e}")
        return []


def normaliser(donnees: bytes) -> Image.Image:
    """Recadrage carre centre puis redimensionnement. Fond blanc si transparence."""
    img = Image.open(io.BytesIO(donnees))
    img.load()

    if img.mode in ("RGBA", "LA", "P"):
        img = img.convert("RGBA")
        fond = Image.new("RGB", img.size, (255, 255, 255))
        fond.paste(img, mask=img.split()[-1])
        img = fond
    else:
        img = img.convert("RGB")

    l, h = img.size
    cote = min(l, h)
    if cote < 200:
        raise ValueError(f"resolution insuffisante ({l}x{h})")

    gauche = (l - cote) // 2
    haut = (h - cote) // 2
    img = img.crop((gauche, haut, gauche + cote, haut + cote))
    return img.resize((TAILLE, TAILLE), Image.LANCZOS)


def telecharger(session: requests.Session, candidats: list[dict], code: str,
                requete: str, sortie: Path) -> Provenance | None:
    """Essaie les candidats dans l'ordre, retient le premier exploitable."""
    for c in candidats:
        url = c.get("url")
        if not url:
            continue
        try:
            r = session.get(url, headers={"User-Agent": UA}, timeout=30)
            r.raise_for_status()
            if not r.headers.get("content-type", "").startswith("image/"):
                continue
            img = normaliser(r.content)
        except (requests.RequestException, ValueError, OSError) as e:
            print(f"    - candidat ecarte : {e}")
            continue

        chemin = sortie / f"{code}.webp"
        img.save(chemin, "WEBP", quality=QUALITE, method=6)
        poids = chemin.stat().st_size // 1024
        print(f"    OK {chemin.name} ({poids} Ko) - {c.get('license', '?')}")

        return Provenance(
            code=code,
            titre=(c.get("title") or "").strip() or "(sans titre)",
            auteur=(c.get("creator") or "").strip() or "(inconnu)",
            licence=f"{c.get('license', '?')} {c.get('license_version', '')}".strip(),
            url_source=url,
            page_origine=c.get("foreign_landing_url", ""),
            requete=requete,
        )
    return None


def ecrire_sources(sortie: Path, provenances: list[Provenance]) -> None:
    """Fusionne avec l'existant : le fichier est la trace legale cumulee."""
    fichier = sortie / "SOURCES.md"
    connues: dict[str, dict] = {}

    json_cache = sortie / ".sources.json"
    if json_cache.exists():
        connues = json.loads(json_cache.read_text(encoding="utf-8"))

    for p in provenances:
        connues[p.code] = asdict(p)

    json_cache.write_text(json.dumps(connues, ensure_ascii=False, indent=2),
                          encoding="utf-8")

    lignes = [
        "# Provenance des images du catalogue",
        "",
        "Genere par `telecharger_images.py`. **Ne pas editer a la main.**",
        "",
        "Toutes les images sont sous licence CC0 ou domaine public : aucune",
        "obligation d'attribution. Ce fichier est conserve comme trace en cas",
        "de contestation, et doit etre versionne avec le code.",
        "",
        "| Code article | Titre | Auteur | Licence | Origine |",
        "|---|---|---|---|---|",
    ]
    for code in sorted(connues):
        p = connues[code]
        page = p.get("page_origine") or p.get("url_source", "")
        lien = f"[source]({page})" if page else "-"
        lignes.append(
            f"| `{code}` | {p['titre'][:60]} | {p['auteur'][:30]} | {p['licence']} | {lien} |"
        )
    lignes.append("")
    lignes.append(f"_{len(connues)} images referencees._")
    fichier.write_text("\n".join(lignes), encoding="utf-8")
    print(f"\n-> {fichier} ({len(connues)} entrees)")


def ecrire_planche(sortie: Path) -> None:
    """Planche-contact HTML pour la revue visuelle obligatoire."""
    fichiers = sorted(sortie.glob("*.webp"))
    cartes = "\n".join(
        f'<figure><img src="{f.name}" alt="{f.stem}" loading="lazy">'
        f'<figcaption>{f.stem}</figcaption></figure>'
        for f in fichiers
    )
    html = f"""<!DOCTYPE html>
<html lang="fr"><head><meta charset="utf-8">
<title>Djigui - revue des images ({len(fichiers)})</title>
<style>
 body {{ font-family: system-ui, sans-serif; background:#f6f6f4; margin:24px; }}
 h1 {{ font-size:18px; }}
 p.aide {{ color:#666; font-size:13px; max-width:70ch; line-height:1.5; }}
 .grille {{ display:grid; grid-template-columns:repeat(auto-fill,minmax(140px,1fr)); gap:14px; }}
 figure {{ margin:0; background:#fff; border:1px solid #e3e3e0; border-radius:10px;
           padding:8px; text-align:center; }}
 img {{ width:100%; aspect-ratio:1; object-fit:cover; border-radius:6px;
        background:#eee; }}
 figcaption {{ font-size:11px; color:#444; margin-top:6px; word-break:break-all; }}
</style></head><body>
<h1>Revue des images du catalogue &mdash; {len(fichiers)} fichiers</h1>
<p class="aide">Rejeter et supprimer tout fichier presentant : un logo ou emballage
de marque visible, un visage identifiable, un sujet hors propos, ou une image
illisible a cette taille. Relancer ensuite le script : seuls les fichiers
supprimes seront retelecharges.</p>
<div class="grille">
{cartes}
</div></body></html>"""
    (sortie / "planche.html").write_text(html, encoding="utf-8")
    print(f"-> {sortie / 'planche.html'} : ouvrir pour la revue visuelle")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mapping", default="requetes_images.json")
    ap.add_argument("--sortie", default="images/articles")
    ap.add_argument("--seulement", help="ne traiter qu'un seul code article")
    ap.add_argument("--requete", help="requete manuelle, avec --seulement")
    ap.add_argument("--manquants", action="store_true",
                    help="lister les codes sans image, sans rien telecharger")
    ap.add_argument("--jeton", help="jeton Openverse (quotas superieurs)")
    args = ap.parse_args()

    mapping = Path(args.mapping)
    if not mapping.exists():
        sys.exit(f"Mapping introuvable : {mapping}")
    articles = json.loads(mapping.read_text(encoding="utf-8"))["articles"]

    sortie = Path(args.sortie)
    sortie.mkdir(parents=True, exist_ok=True)

    if args.seulement:
        if args.requete:
            articles = {args.seulement: {"q": args.requete, "alt": args.requete}}
        elif args.seulement in articles:
            articles = {args.seulement: articles[args.seulement]}
        else:
            sys.exit(f"Code inconnu : {args.seulement}. Utiliser --requete.")

    manquants = [c for c in articles if not (sortie / f"{c}.webp").exists()]

    if args.manquants:
        print(f"{len(manquants)} image(s) manquante(s) sur {len(articles)} :")
        for c in manquants:
            print(f"  - {c}  ({articles[c]['q']})")
        return 0

    if not manquants:
        print("Toutes les images sont deja presentes. Rien a faire.")
        ecrire_planche(sortie)
        return 0

    print(f"{len(manquants)} image(s) a recuperer.\n")
    session = requests.Session()
    provenances: list[Provenance] = []
    echecs: list[str] = []

    for i, code in enumerate(manquants, 1):
        entree = articles[code]
        print(f"[{i}/{len(manquants)}] {code}")
        resultat = None

        for requete in (entree["q"], entree.get("alt")):
            if not requete:
                continue
            print(f"    ? \"{requete}\"")
            candidats = chercher(session, requete, args.jeton)
            if candidats:
                resultat = telecharger(session, candidats, code, requete, sortie)
                if resultat:
                    break
            time.sleep(PAUSE)

        if resultat:
            provenances.append(resultat)
        else:
            print("    ECHEC - aucun candidat exploitable")
            echecs.append(code)
        time.sleep(PAUSE)

    if provenances:
        ecrire_sources(sortie, provenances)
    ecrire_planche(sortie)

    print(f"\nRecuperees : {len(provenances)}  |  Echecs : {len(echecs)}")
    if echecs:
        print("A reprendre manuellement avec --seulement / --requete :")
        for c in echecs:
            print(f"  python telecharger_images.py --seulement {c} --requete \"...\"")
    print("\nEtape suivante OBLIGATOIRE : ouvrir planche.html et rejeter les images"
          "\ncomportant une marque, un visage, ou hors sujet (cf. IMAGES-SOURCING.md §3).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
