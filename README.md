# Souls PS1 — prototype de combat

Prototype de combat inspiré de *Lies of P* / *Dark Souls*, rendu façon PlayStation 1, en
Rust + [Bevy 0.19](https://bevy.org) (natif et navigateur via WASM).

Un joueur, deux armes (rapière et greatsword), un boss lent et télégraphié — l'Automate du
Carrousel — dans une arène circulaire.

## Lancer

```sh
# Natif (le plus rapide pour itérer)
cargo run --release

# Natif avec rechargement à chaud des fichiers de tuning (assets/config/*.ron)
cargo run --features dev

# Navigateur : build dans dist/ (~5 min la première fois), puis http://127.0.0.1:8080
tools/build_web.sh
tools/serve_web.sh
```

Prérequis web : la cible `wasm32-unknown-unknown` et `wasm-bindgen-cli` **de la même version
que `wasm-bindgen` dans `Cargo.lock`** (actuellement `cargo install wasm-bindgen-cli --version 0.2.129`).
`wasm-opt` (binaryen) est utilisé s'il est installé. WASM : ~31 Mo, ~8 Mo une fois compressé.
Blender 5.x et Python 3 ne servent qu'à régénérer les assets.

## Contrôles

| Action | Manette | Clavier / souris |
|---|---|---|
| Attaque légère | RB / R1 | Clic gauche |
| Attaque lourde (maintenir = charge) | RT / R2 | Clic droit |
| Garde (garde parfaite = au bon moment) | LB / L1 | Q ou Maj gauche |
| Attaque spéciale de l'arme | LT / L2 | E |
| Esquive (maintenir = sprint) | B / ○ | Espace |
| Verrouillage | R3 | Tab ou clic molette |
| Changer d'arme | Y / △ | R |
| Soin (3 charges) | X / □ | F |
| Déplacement / caméra | sticks | WASD / souris |
| Recommencer | A / Start (après la fin) | Entrée ou F5 |

Les touches sont lues par position physique (disposition QWERTY/QWERTZ) : sur un clavier
AZERTY, le déplacement est sur ZQSD et la garde sur A.

Clic dans la fenêtre pour capturer la souris, Échap pour la libérer. H affiche ou masque l'aide.

### Menu et options

**Échap** (clavier) ou **Start** (manette) ouvre le menu pause. Le jeu démarre en plein écran.

| Option | Valeurs |
|---|---|
| Affichage | Plein écran (sans bordure) · Plein écran exclusif · Fenêtré |
| Résolution | taille de la fenêtre, ou mode vidéo en exclusif (natif uniquement) |
| Fréquence | fréquences proposées par l'écran, en exclusif (natif uniquement) |
| Synchronisation verticale | activée / désactivée (natif uniquement) |
| Résolution interne | 240p (PS1), 360p, 480p |
| Volume général, volume des effets | 0 à 100 % |
| Sensibilité caméra, axe vertical inversé, tremblements de caméra | |

Les options sont enregistrées dès qu'on les change et reprises au lancement suivant :
`~/.config/souls-ps1/settings.ron` (Linux), `%APPDATA%\souls-ps1\settings.ron` (Windows),
`~/Library/Application Support/souls-ps1/settings.ron` (macOS), `localStorage` dans le navigateur.
Dans le navigateur, le plein écran s'active au premier clic ou à la première touche (le navigateur
l'exige) ; Échap en fait sortir, une action en jeu y fait revenir.

### Outils de réglage

- **F1** : overlay d'état (action en cours au tick près, fenêtre de garde parfaite, stagger…)
- **F2** : hitboxes (bleu = corps, orange = coup actif, rouge = furie, jaune = coup imminent)
- **F3** : ralenti ×0,25 (les timings en ticks restent exacts)
- **F4** : boss passif (il marche mais n'attaque plus)
- **F5** : recommencer le combat

## Mécaniques

- **Garde parfaite** : appuyer sur garde au plus 8 ticks (~133 ms) avant l'impact. Aucun dégât,
  stagger infligé au boss, jauge spéciale. Spammer la garde réduit la fenêtre.
- **Garde normale** : 60 % des dégâts, convertis en **regain** (barre grise) qu'on récupère en
  frappant le boss dans les 6 secondes. Plus d'endurance → garde brisée.
- **Attaques furie** (le boss rougeoie) : la garde normale ne sert à rien, il faut une garde
  parfaite ou une esquive.
- **Groggy** : jauge de stagger du boss pleine (gardes parfaites, attaques lourdes chargées) →
  il tombe à genoux ; attaque légère de face pour le **coup fatal**.
- **Rapière** : combo de 4 estocs rapides ; lourde chargée en fente ; spéciale *Fente éclair*
  (dash + rafale).
- **Greatsword** : lente et lourde, grande allonge, hyperarmure pendant les coups ; 3 tailles
  larges ; lourde verticale ; spéciale *Posture* (garde haute avec hyperarmure : un coup reçu
  pendant la posture déclenche une riposte).
- **Endurance** : il faut au moins 1 point pour attaquer, esquiver ou lancer la spéciale ; une
  action peut faire passer l'endurance en négatif (jusqu'à -60), il faut alors attendre. Le coût
  d'une attaque = ses dégâts × `stamina_per_damage` : à dégâts égaux, même coût pour les deux armes.
- **Soins** : 3 charges (rechargées aux checkpoints, à venir), +40 % des PV ; on peut marcher
  lentement pendant. Touché avant que le soin s'applique : la charge est perdue.
- **Jauge spéciale** (3 segments sous l'endurance) : se remplit en frappant et en garde parfaite ;
  chaque attaque spéciale consomme un segment.
- **Boss** : 6 attaques en phase 1, 2 de plus en phase 2 (sous 50 % de PV), dont une furie.

## Régler le feel

Toutes les valeurs de gameplay sont dans `assets/config/` et exprimées en **ticks** (60/s) :

- `player.ron` : PV, endurance, vitesses, esquive (i-frames), garde parfaite, regain…
- `weapons.ron` : chaque attaque (startup/actif/récupération, hitbox, dégâts, stagger, root motion)
- `boss.ron` : PV, phases, stagger, pauses entre les attaques, chaque attaque et ses portées
- `arena.ron` : taille de l'arène, piliers, points d'apparition

Avec `cargo run --features dev`, sauvegarder un fichier relance le combat avec les nouvelles
valeurs. Les animations se recalent automatiquement si on modifie les fenêtres de frappe.

## Architecture

```
src/sim/      simulation déterministe à 60 Hz (aucune dépendance au rendu)
  data.rs       types du tuning (frame data)
  input.rs      PlayerInput : 7 octets par joueur et par tick (ce qui transitera sur le réseau)
  player.rs     machine à états du joueur (combos, charge, garde, esquive, sprint…)
  boss.rs       IA du boss (aggro multi-joueurs, choix pondéré, cooldowns, phases)
  combat.rs     collisions, hitbox (capsules, balayages en arc), garde / parfaite / regain
src/input.rs  clavier/souris/manette → PlayerInput (appuis verrouillés entre deux ticks)
src/render/   rendu PS1, caméra, modèles et pilotage des animations par l'état de la sim
src/fx.rs     sons, étincelles, tremblements déclenchés par les événements de la sim
src/hud.rs    interface
tools/blender assets générés par script (modèles low-poly, animations calées sur les timings)
tools/sfx.py  bruitages synthétisés
tests/sim.rs  tests de la simulation, dont le déterminisme
```

**Pensé pour le multijoueur en ligne (coop contre le boss).** La simulation ne lit que des
`PlayerInput` et un compteur de ticks, utilise des maths `libm` et une RNG interne : rejouer
les mêmes inputs donne exactement le même état (vérifié par `cargo test`). C'est la condition
pour du rollback avec `bevy_ggrs` + `matchbox` (WebRTC pair-à-pair dans le navigateur) — la
version de Bevy a été choisie pour être compatible avec ces deux crates. Le boss choisit déjà sa
cible entre plusieurs joueurs.

## Régénérer les assets

```sh
tools/build_assets.sh   # timings → Blender (joueur, boss, armes, arène) → bruitages
```

Les fichiers `.blend` sont écrits dans `tools/blender/blend/` pour retouche manuelle (non versionnés :
ils sont régénérés par `tools/build_assets.sh`). Les `.glb` produits, eux, sont versionnés.
`tools/blender/preview.py` rend des planches de poses pour vérifier une animation :

```sh
PREVIEW_WEAPON=rapier blender -b tools/blender/blend/player.blend -P tools/blender/preview.py -- out.png rapier_light1:0 rapier_light1:8
```

## Crédits

Polices DejaVu (licence libre, voir `assets/fonts/LICENSE-DejaVu.txt`). Tout le reste (modèles,
textures, animations, sons) est généré par les scripts du dépôt.
