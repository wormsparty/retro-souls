# Giant's Flame — prototype de combat

Prototype de combat inspiré de *Lies of P* / *Dark Souls*, rendu façon PlayStation 1, en
Rust + [Bevy 0.19](https://bevy.org) (natif et navigateur via WASM).

Un joueur, deux armes (rapière et greatsword), un boss lent et télégraphié — l'Automate du
Carrousel — dans une arène circulaire. En sortant de l'arène, un escalier descend vers la place
des Allumeurs (premier checkpoint), suspendue au-dessus du vide ; de là, un long chemin de ponts
et de plates-formes, gardé par des chiens et des pantins, mène au second checkpoint.

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
| Changer de cible (verrouillé) : point suivant à gauche / droite, plus haut / plus bas (la tête d'un grand boss) | stick droit ← → ↑ ↓ | souris ← → ↑ ↓ |
| Changer d'arme | croix ↑ | R |
| Attaque sautée (plus de dégâts, l'endurance du saut suffit presque) | RB / RT en l'air | clic en l'air |
| Utiliser l'objet sélectionné | X / □ | F |
| Objet suivant (emplacements rapides) | croix ↓ | C |
| Interagir (ramasser, se reposer) ; sinon sauter | A / ✕ | G |
| Déplacement / caméra | sticks | WASD / souris |
| Menu | Start | Échap |

Les touches sont lues par position physique (disposition QWERTY/QWERTZ) : sur un clavier
AZERTY, le déplacement est sur ZQSD et la garde sur A.

Clic dans la fenêtre pour capturer la souris, Échap pour la libérer. La page **Aide** du menu
pause rappelle les contrôles du dernier périphérique utilisé (manette, ou clavier et souris).

### Menus

- **Écran titre** : le titre centré, sur un fond où montent des braises et des cendres
  au-dessus d'une lueur de brasier. Nouvelle partie (demande confirmation si une sauvegarde
  existe), Charger, Options, *Fork me* (ouvre la page du projet sur GitHub), Quitter.
- **Pause** (Échap / Start) : deux icônes, Équipement (heaume) et Système (roue crantée :
  Options, Aide, *Fork me*, Retour à l'écran titre, Quitter). Échap / (B) referme le menu. En
  pause, les personnages et les sorts sont figés ; les particules continuent.
- **Checkpoint** (en s'y reposant) : Partir (sélectionné par défaut), Voyager (vers une autre
  brasier déjà ranimé, avec une vue du lieu), Choisir le boss, Monter de niveau (grisé, à venir),
  Équipement, Ranimer le boss (s'il a été vaincu).
- **Choisir le boss** : qui attend dans l'arène (il y apparaît aussitôt, en pleine forme), en
  grille de portraits (`assets/ui/boss_<n>.png`, rendus par `tools/blender/boss_icons.py`). Les
  rencontres sont dans `assets/config/bosses.ron` (en debug : `SOULS_BOSS=n`). Chaque boss a sa
  couleur (`color`), celle de ses cercles au sol :
  - l'Automate du Carrousel (`boss.ron`) ;
  - la Wyverne de Cendre : immense, on verrouille sa tête (stick droit ↑), ses pattes ou sa
    queue. Elle ne vise pas : elle avance à peu près vers sa proie, et finit par lui faire face.
    Devant elle, coup de tête jusqu'au sol ou plaquage (elle se laisse tomber) ; sur son flanc,
    elle pivote d'un quart de tour en fouettant de la queue ; derrière, la queue balaie ; de
    loin, un jet de feu jusqu'au sol ; elle s'envole et retombe sur sa cible, ou s'élève hors
    d'atteinte et crache trois boules de feu ; pluie de cendre en phase 2 ;
  - le Boucher Cornu et ses deux chiens : couperets en diagonale (gauche puis droite),
    tourbillon penché à hauteur d'homme, charge tête baissée, fendoir (onde de choc), couperet
    lancé ; il bondit loin en arrière puis charge ou lance un couperet ; frénésie en phase 2.
    Les chiens n'ont qu'une petite barre de vie et s'effondrent avec leur maître ;
  - l'Allumeur et l'Enclume : l'allumeur est un magicien qui garde ses distances et ne reste
    pas en place : il file régulièrement de côté (glissade), puis tire souvent de là (lueurs
    qui suivent leur cible, salve de trois lueurs l'une après l'autre, fusées qui jaillissent
    du sol, bond en arrière) ; le forgeron frappe lourd (marteau, revers, charge, séisme). Quand
    l'un tombe, l'autre passe en phase 2 (anneaux de lumière, pluie de fusées ; traînée de braises) ;
  - la Grande Marionnette (violette) : elle se sait vulnérable et bouge sans cesse : bonds de
    côté ou en arrière, en jetant deux aiguilles (bras levés au-dessus d'elle : elles partent
    de très haut, on les voit venir) ; gifle, chute sur sa cible,
    pirouette, danse ; hissée à la verticale, elle retombe sur place (grande onde de choc) ;
    envolée : hors d'atteinte, elle jette trois aiguilles puis se laisse tomber sur sa cible ;
    fils qui s'abattent en phase 2 ;
  - la Bête à l'Échine Creuse (le lézard, ventre à terre) : on verrouille sa tête ou son
    arrière-train ; morsure, bras dorsaux, pivot et coup de queue, bond ; elle se dresse et
    retombe : la glace jaillit devant elle ; frénésie et hurlement (cercles de glace) en phase 2 ;
  - le Géant Porte-Flamme : colonne qui fend le sol en lançant une traînée de feu, piliers de
    feu sous sa cible (qui ne tombent pas les uns sur les autres), trois boules de feu l'une
    après l'autre, anneaux de flammes dont le premier à ses pieds ; collé à lui, on prend un
    coup de pied qui fend le sol tout autour, ou la colonne qui fauche à hauteur de genou ;
    trop près, il bondit aussi en arrière pour tirer de loin ; brasier, météores et vague de
    feu en phase 2.

  Modèles et animations : `tools/blender/trial_bosses.py`. Les sorts (projectiles et éruptions
  annoncées au sol) d'une même attaque ne touchent qu'une fois. Un projectile qui s'écrase au
  sol y brûle encore un moment (petite zone, moitié des dégâts, ni garde ni parade). Sorts,
  ondes de choc et cercles sont tous à la couleur du boss (sauf le couperet, en fer).
- **Équipement** (façon Dark Souls) : en haut les armes (montrées, on en change en jeu) et le
  talisman, en dessous les cases des consommables, avec l'icône de l'objet équipé — 4
  emplacements rapides pour les consommables (en jeu, croix ↓ / C passe à l'emplacement équipé
  suivant) et un emplacement de talisman (le premier talisman ramassé est porté d'office).
  Choisir une case ouvre la liste de ce qu'on possède (avec icônes et quantités) ; depuis le
  menu pause ou un brasier.

Échap / (B) revient à la page précédente. Le jeu démarre en plein écran.

| Option | Valeurs |
|---|---|
| Affichage | Plein écran (sans bordure) · Plein écran exclusif (sauf Wayland et navigateur) · Fenêtré |
| Résolution | taille de la fenêtre, ou mode vidéo en exclusif (natif uniquement) |
| Fréquence | fréquences proposées par l'écran, en exclusif (natif uniquement) |
| Synchronisation verticale | activée / désactivée (natif uniquement) |
| Résolution interne | 240p (PS1), 480p (moderne) |
| Volume général, volume des effets | 0 à 100 % |
| Sensibilité caméra, axe vertical inversé, tremblements de caméra | |

Sous **Wayland**, le plein écran exclusif n'existe pas (le protocole ne permet pas de changer de
mode vidéo ; winit l'ignore) : l'option n'est pas proposée. Pour l'avoir quand même, lancer le jeu
via XWayland avec `WAYLAND_DISPLAY= cargo run --release` (le compositeur émule alors le changement
de mode).

Langue : anglais ou français, demandée au premier lancement (anglais par défaut) et modifiable
dans les options.

Style graphique : PS1 (résolution interne 240p) ou moderne (480p), demandé au premier lancement
juste après la langue, avec une capture de chaque (`assets/ui/style_*.png`). Modifiable dans les
options (« Résolution interne »).

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
  parfaite ou une esquive. Le boss rougeoie aussi avant un coup imparable qu'aucun cercle
  n'annonce (le jet de feu de la wyverne) : il faut fuir.
- **Attaques de zone** (cercle au sol, à la couleur du boss, qui se remplit jusqu'à l'impact) :
  ni garde ni garde parfaite. Le cercle apparaît au moins 40 ticks (⅔ s) avant l'impact et ne
  bouge plus : dès qu'il s'affiche, le boss cesse de suivre sa cible (un saut qui retombe sur
  elle la suit jusqu'au décollage, son vol sert d'alerte). L'*onde de choc* dure plus longtemps que les i-frames d'une roulade : il faut
  sortir du cercle. Le *bond arrière* est souvent suivi du *saut écrasant*, qui retombe là où se
  trouvait le joueur au décollage.
- **Roulade** : ~3,5 m (pas en arrière : ~1,7 m).
- **Groggy** : jauge de stagger du boss pleine (gardes parfaites, attaques lourdes chargées) →
  il tombe à genoux ; attaque légère de face pour le **coup fatal**. La jauge n'est pas
  affichée (F1 la montre).
- **Attaque sautée** : attaquer (légère ou lourde) pendant un saut : estoc plongeant (rapière)
  ou taille abattue (épée longue), plus de dégâts et de stagger qu'une attaque normale pour
  très peu d'endurance (le saut l'a déjà payée) ; en l'air, on atteint aussi plus haut.
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
- **Jauge spéciale** (barre dorée sous l'endurance) : se remplit en frappant et en garde
  parfaite ; chaque attaque spéciale en consomme un tiers. Les coups de la spéciale ne rechargent pas la jauge,
  et son coût d'endurance est fixe (pas proportionnel à ses gros dégâts).
- **HUD** : en bas à gauche, l'arme équipée (icône ; croix ↑ / R pour changer) au-dessus de
  l'objet rapide sélectionné.
- **Boss** : 9 attaques en phase 1 (dont une furie et deux attaques de zone), 2 de plus en
  phase 2 (sous 50 % de PV).

## Le chemin

```
              arène (boss)
                   │ escalier
        place des Allumeurs ◆ checkpoint ── pont ── jardin de la fontaine
                   │ pont
          kiosque à musique : 2 chiens endormis
                   │ rampe
   guichets : 2 pantins, 1 chien ── passerelle étroite ── corniche : chien ⤳ saut : éclat de fiole
                   │ long pont : un pantin en travers
   piste du colosse (unique) ── planche étroite ── corniche : plume de cimier
                   │ escalier
        belvédère brisé ◆ checkpoint ── pont effondré…
                   ⤳ saut en courant : rocher isolé, broche de fer
```

- **Saut** (A / G quand il n'y a rien à ramasser ni de brasier à portée) : ~0,8 m de haut,
  ~3,5 m franchis en courant (`jump` dans `player.ron`), 12 d'endurance. Retomber au-dessus
  du vide, c'est la chute.
- **Réapparition au brasier** : à côté du feu, tourné vers la suite du chemin (`look` dans
  `level.ron`) ; une partie quittée au pied d'un brasier reprend de la même façon.
- **Le vide** : hors de l'arène et de l'escalier, aucun garde-fou. Marcher, rouler ou être
  repoussé au-delà d'un bord, c'est la chute, et la mort. En contrebas, il n'y a que le noir et,
  au loin, quelques réverbères perdus sur des rochers flottants.
- **Checkpoints** (brasiers) : une vasque de fer sur un socle, une vieille épée plantée dans
  les braises. Éteint, il ne laisse échapper qu'un filet d'escarbilles ; s'y reposer le ranime
  (« BRASIER RANIMÉ ») : une colonne de braises et de cendres monte alors au-dessus, visible de
  loin. Il devient le point de réapparition et une destination de voyage. Se reposer rend PV,
  endurance et fioles, mais **fait revenir tous les ennemis** ; impossible tant qu'un ennemi
  est à vos trousses.
- **Mort** : on réapparaît au dernier brasier **sans ses braises** : elles restent sur place,
  avec votre cadavre nimbé de vert d'où montent des lueurs vertes, visibles de loin (au bord d'où l'on est tombé, après
  une chute). Interagir près du cadavre les récupère ; mourir avant de l'avoir atteint les fait
  perdre pour de bon.
- **Ennemis** : endormis (on peut les approcher, mais ils sentent tout autour d'eux) ou aux
  aguets (ils voient loin, devant eux). Un cri d'alerte réveille tout leur groupe. Trop loin
  de leur poste, ils abandonnent, y retournent et se soignent. Vaincus, ils donnent des
  braises ; ils reviennent au repos et à la mort, sauf le colosse.
  - *Chien errant* : rapide, morsures et bond ; interrompu par chaque coup.
  - *Pantin de foire* : maillet de « tête de Turc » (coup vertical en hyperarmure, revers,
    estoc en avançant) ; il faut deux coups de rapière pour l'interrompre.
  - *Colosse de la piste* : un pantin géant, unique, qui ne bronche presque jamais.
  Les coups du joueur s'abaissent jusqu'aux adversaires plus petits qu'eux (un estoc porté à
  hauteur de poitrine touche un chien).
- **Objets au sol** : des lueurs blanches entourées d'étincelles qui tournoient en montant
  (visibles à travers le brouillard) ; on ne sait ce que c'est qu'en les ramassant
  (« Ramasser », fenêtre de l'objet obtenu). Chacun ne se ramasse qu'une fois.
  - consommables (emplacements rapides, ne se rechargent pas) : *braise ternie* / *braise vive*
    (à écraser pour gagner des braises), *mousse dorée* (régénère des PV), *résine ardente*
    (+20 % de dégâts pendant une minute, la lame rougeoie) ;
  - talismans : *broche de fer* (dégâts subis −15 %), *plume de cimier* (esquives −30 %
    d'endurance) ;
  - *éclat de fiole* : une charge de soin de plus, définitivement.

## Progression et sauvegarde

- On commence au **checkpoint** de la place, au pied de l'escalier de l'arène. S'y reposer
  rend PV, endurance et objets.
- Entrer dans l'arène réveille le boss et une **brume** ferme l'escalier jusqu'à la fin du combat.
- Victoire : **+1000 braises** (`embers` dans `boss.ron`, icône de flamme), affichées en bas à droite au-dessus du
  compteur, qui les absorbe avec un petit son ; le boss reste mort, on peut le ranimer depuis
  le checkpoint.
- Mort : « VOUS ÊTES MORT » (~4 s, l'écran s'assombrit puis passe au noir), puis retour au
  dernier checkpoint où l'on s'est reposé (braises conservées), objets rechargés, boss et
  ennemis réinitialisés.
- **Sauvegarde automatique** façon Dark Souls (un seul emplacement) : à chaque événement important
  (repos, entrée dans l'arène, victoire, ennemi vaincu, objet ramassé ou utilisé, chute, mort,
  menu ouvert/fermé), toutes les 5 secondes et en quittant ; le fichier n'est réécrit que s'il a
  changé. On reprend à l'endroit où on a quitté, sauf en plein combat de boss (on revient devant
  la brume, le boss repart de zéro) ou en pleine chute (au dernier checkpoint). Les objets
  ramassés, brasiers ranimés, ennemis uniques vaincus et braises laissées à la mort sont sauvegardés.
  Fichier `save.ron` à côté de `settings.ron` (`localStorage` dans le navigateur).

## Régler le feel

Toutes les valeurs de gameplay sont dans `assets/config/` et exprimées en **ticks** (60/s) :

- `player.ron` : PV, endurance, vitesses, esquive (i-frames), garde parfaite, regain…
- `weapons.ron` : chaque attaque (startup/actif/récupération, hitbox, dégâts, stagger, root motion)
- `boss.ron` : PV, phases, stagger, pauses entre les attaques, chaque attaque et ses portées
- `bosses.ron` : les autres boss (même format, plus modèle et échelle, parties verrouillables,
  sorts) et les rencontres proposées au checkpoint
- `arena.ron` : taille de l'arène, piliers, ouverture, apparition du boss
- `level.ron` : le reste du niveau — sols (place, ponts, rampes, escaliers ; bords murés ou
  ouverts sur le vide), checkpoints (nom, vue du menu de voyage), décor, ennemis, objets
- `enemies.ron` : types d'ennemis (vision, poursuite, équilibre, braises, attaques)

Avec `cargo run --features dev`, sauvegarder un fichier relance le combat avec les nouvelles
valeurs. Les animations se recalent automatiquement si on modifie les fenêtres de frappe. Le
décor est généré à partir de `arena.ron` et `level.ron` : après une modification de la
géométrie, relancer `tools/build_assets.sh` pour que l'image corresponde aux collisions.

## Architecture

```
src/sim/      simulation déterministe à 60 Hz (aucune dépendance au rendu)
  data.rs       types du tuning (frame data)
  input.rs      PlayerInput : 7 octets par joueur et par tick (ce qui transitera sur le réseau)
  player.rs     machine à états du joueur (combos, charge, garde, esquive, sprint…)
  boss.rs       IA des boss (aggro multi-joueurs, choix pondéré, cooldowns, phases, duos,
                points verrouillables des grands boss)
  spell.rs      sorts des boss : projectiles, éruptions annoncées au sol
  enemy.rs      IA des ennemis du chemin (poste, alerte de groupe, poursuite, abandon)
  world.rs      sols praticables et hauteurs, bords murés ou ouverts (chute), obstacles
  combat.rs     collisions, hitbox (capsules, balayages en arc), garde / parfaite / regain
  encounter.rs  checkpoints, voyage, brume, victoire, mort/réapparition, progression, menus
  items.rs      objets (consommables, talismans, objets clés), inventaire, emplacements rapides
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
tools/build_assets.sh   # timings et niveau → Blender (joueur, boss, armes, chien, pantin, décor) → bruitages → police
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
