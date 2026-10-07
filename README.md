# Giant's Flame — prototype de combat

Prototype de combat inspiré de *Lies of P* / *Dark Souls*, rendu façon PlayStation 1, en
Rust + [Bevy 0.19](https://bevy.org) (natif et navigateur via WASM).

Un joueur, deux armes (rapière et greatsword), un boss lent et télégraphié — l'Automate du
Carrousel — dans une arène circulaire, précédée d'un couloir avec un checkpoint.

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
| Sprint (un clic, tant que le stick est poussé) | L3 | — |
| Verrouillage | R3 | Tab ou clic molette |
| Changer d'arme | Y / △ | R |
| Utiliser l'objet sélectionné | X / □ | F |
| Objet suivant (emplacements rapides) | croix ↓ | C |
| Se reposer au checkpoint | A / ✕ | G |
| Déplacement / caméra | sticks | WASD / souris |
| Menu | Start | Échap |

Les touches sont lues par position physique (disposition QWERTY/QWERTZ) : sur un clavier
AZERTY, le déplacement est sur ZQSD et la garde sur A.

Clic dans la fenêtre pour capturer la souris, Échap pour la libérer. La page **Aide** du menu
pause rappelle les contrôles du dernier périphérique utilisé (manette, ou clavier et souris).

### Menus

- **Écran titre** : Nouvelle partie (demande confirmation si une sauvegarde existe), Charger,
  Options, Quitter.
- **Pause** (Échap / Start) : Reprendre, Équipement, Options, Aide, Retour à l'écran titre, Quitter.
- **Checkpoint** (en s'y reposant) : Partir (sélectionné par défaut), Voyager (liste des
  checkpoints avec une vue du lieu ; un seul pour l'instant), Monter de niveau (grisé, à venir),
  Équipement, Ranimer l'Automate (s'il a été vaincu).
- **Équipement** : 4 emplacements rapides où équiper les objets de l'inventaire (pour l'instant :
  la fiole de soin) ; en jeu, croix ↓ / C passe à l'emplacement équipé suivant.

Échap / (B) revient à la page précédente. Le jeu démarre en plein écran.

| Option | Valeurs |
|---|---|
| Affichage | Plein écran (sans bordure) · Plein écran exclusif (sauf Wayland et navigateur) · Fenêtré |
| Résolution | taille de la fenêtre, ou mode vidéo en exclusif (natif uniquement) |
| Fréquence | fréquences proposées par l'écran, en exclusif (natif uniquement) |
| Synchronisation verticale | activée / désactivée (natif uniquement) |
| Résolution interne | 240p (PS1), 360p, 480p |
| Volume général, volume des effets | 0 à 100 % |
| Sensibilité caméra, axe vertical inversé, tremblements de caméra | |

Sous **Wayland**, le plein écran exclusif n'existe pas (le protocole ne permet pas de changer de
mode vidéo ; winit l'ignore) : l'option n'est pas proposée. Pour l'avoir quand même, lancer le jeu
via XWayland avec `WAYLAND_DISPLAY= cargo run --release` (le compositeur émule alors le changement
de mode).

Langue : anglais ou français, demandée au premier lancement (anglais par défaut) et modifiable
dans les options.

Les options sont enregistrées dès qu'on les change et reprises au lancement suivant :
`~/.config/giants-flame/settings.ron` (Linux), `%APPDATA%\giants-flame\settings.ron` (Windows),
`~/Library/Application Support/giants-flame/settings.ron` (macOS), `localStorage` dans le navigateur.
Dans le navigateur, le plein écran s'active au premier clic ou à la première touche (le navigateur
l'exige) ; Échap en fait sortir, une action en jeu y fait revenir.

### Outils de réglage

- **F1** : overlay d'état (action en cours au tick près, fenêtre de garde parfaite, stagger…)
- **F2** : hitboxes (bleu = corps, orange = coup actif, rouge = furie, jaune = coup imminent)
- **F3** : ralenti ×0,25 (les timings en ticks restent exacts)
- **F4** : boss passif (il marche mais n'attaque plus)
- **F5** : recréer les combattants (progression gardée, le boss repart de zéro)

## Mécaniques

- **Garde parfaite** : appuyer sur garde au plus 8 ticks (~133 ms) avant l'impact. Aucun dégât,
  stagger infligé au boss, jauge spéciale. Spammer la garde réduit la fenêtre.
- **Garde normale** : 60 % des dégâts, convertis en **regain** (barre grise) qu'on récupère en
  frappant le boss dans les 6 secondes. Plus d'endurance → garde brisée.
- **Attaques furie** (le boss rougeoie) : la garde normale ne sert à rien, il faut une garde
  parfaite ou une esquive.
- **Attaques de zone** (cercle rouge au sol, qui se remplit jusqu'à l'impact) : ni garde ni
  garde parfaite. L'*onde de choc* dure plus longtemps que les i-frames d'une roulade : il faut
  sortir du cercle. Le *bond arrière* est souvent suivi du *saut écrasant*, qui retombe là où se
  trouvait le joueur au décollage.
- **Roulade** : ~3,5 m (pas en arrière : ~1,7 m).
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
- **Fiole de soin** (objet de l'inventaire) : 3 charges, rechargées au checkpoint et à la mort,
  +40 % des PV ; on peut marcher lentement pendant. Touché avant que le soin s'applique : la
  charge est perdue.
- **Jauge spéciale** (3 segments sous l'endurance) : se remplit en frappant et en garde parfaite ;
  chaque attaque spéciale consomme un segment. Les coups de la spéciale ne rechargent pas la jauge,
  et son coût d'endurance est fixe (pas proportionnel à ses gros dégâts).
- **Boss** : 9 attaques en phase 1 (dont une furie et deux attaques de zone), 2 de plus en
  phase 2 (sous 50 % de PV).

## Progression et sauvegarde

- On commence au **checkpoint** (lanterne au bout du couloir). S'y reposer rend PV, endurance et
  objets.
- Entrer dans l'arène réveille le boss et une **brume** ferme le couloir jusqu'à la fin du combat.
- Victoire : **+1000 braises** (`embers` dans `boss.ron`, icône de flamme), affichées en bas à droite au-dessus du
  compteur, qui les absorbe avec un petit son ; le boss reste mort, on peut le ranimer depuis
  le checkpoint.
- Mort : « VOUS ÊTES MORT » (~4 s, l'écran s'assombrit puis passe au noir), puis retour au
  checkpoint (braises conservées), objets rechargés, boss réinitialisé.
- **Sauvegarde automatique** façon Dark Souls (un seul emplacement) : à chaque événement important
  (repos, entrée dans l'arène, victoire, mort, objet utilisé, menu ouvert/fermé), toutes les
  5 secondes et en quittant ; le fichier n'est réécrit que s'il a changé. On reprend à l'endroit
  où on a quitté, sauf en plein combat : on revient devant la brume et le boss repart de zéro.
  Fichier `save.ron` à côté de `settings.ron` (`localStorage` dans le navigateur).

## Régler le feel

Toutes les valeurs de gameplay sont dans `assets/config/` et exprimées en **ticks** (60/s) :

- `player.ron` : PV, endurance, vitesses, esquive (i-frames), garde parfaite, regain…
- `weapons.ron` : chaque attaque (startup/actif/récupération, hitbox, dégâts, stagger, root motion)
- `boss.ron` : PV, phases, stagger, pauses entre les attaques, chaque attaque et ses portées
- `arena.ron` : taille de l'arène, piliers, couloir, checkpoint (nom, vue du menu de voyage),
  points d'apparition

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
  encounter.rs  checkpoint, brume, victoire, mort/réapparition, progression, commandes des menus
  items.rs      objets, inventaire, emplacements rapides
src/input.rs  clavier/souris/manette → PlayerInput (appuis verrouillés entre deux ticks)
src/render/   rendu PS1, caméra, modèles et pilotage des animations par l'état de la sim,
              aperçu des checkpoints (menu de voyage)
src/fx.rs     sons, étincelles, tremblements déclenchés par les événements de la sim
src/hud.rs    interface en jeu
src/menu.rs   écran titre, pause, checkpoint, équipement, options, aide
src/save.rs   sauvegarde automatique (storage.rs : fichiers / localStorage)
tools/blender assets générés par script (modèles low-poly, animations calées sur les timings)
tools/sfx.py  bruitages synthétisés
tools/pixel_font.py  police bitmap de l'interface
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
tools/build_assets.sh   # timings → Blender (joueur, boss, armes, arène) → bruitages → police
```

Les fichiers `.blend` sont écrits dans `tools/blender/blend/` pour retouche manuelle (non versionnés :
ils sont régénérés par `tools/build_assets.sh`). Les `.glb` produits, eux, sont versionnés.
`tools/blender/preview.py` rend des planches de poses pour vérifier une animation :

```sh
PREVIEW_WEAPON=rapier blender -b tools/blender/blend/player.blend -P tools/blender/preview.py -- out.png rapier_light1:0 rapier_light1:8
```

## Crédits

Tout est généré par les scripts du dépôt : modèles, textures, animations, sons, et la police
bitmap de l'interface (`tools/pixel_font.py`, cadratin de 12 pixels, accents français). L'interface
est dessinée sur une grille de gros pixels (≈ 360 lignes, un nombre entier de pixels de l'écran
par point) comme le jeu ; les icônes de touches (clavier, souris, manette Xbox) et d'objets sont
dessinées en pixel art par `src/ui.rs` et `src/hud.rs`.
