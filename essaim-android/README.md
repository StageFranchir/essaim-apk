# Essaim — version Android (Rust)

Portage en Rust du jeu web *Essaim* : un essaim de bestioles bioluminescentes, piloté par
l'algorithme des **boids de Craig W. Reynolds** (séparation, alignement, cohésion), traque la
proie, c'est-à-dire ton doigt. Trace des murs de lumière qu'elles ne traversent pas et referme
des formes autour d'elles pour les piéger.

Moteur : [macroquad](https://macroquad.rs) (OpenGL ES, un seul binaire pour Android, Linux,
Windows et macOS).

## Règles (identiques au jeu web)

| | |
|---|---|
| Proie | Ton doigt. L'essaim se dirige en nuée vers lui ; chaque contact retire de la vie (5, ou 13 pour un colosse). |
| Murs | Infranchissables pour l'essaim, ils consomment de l'encre et disparaissent au bout de 7,5 s. |
| Encre | 100 points, se recharge quand tu ne traces pas. |
| Piège | Refermer une forme dissout toutes les bestioles à l'intérieur : 25 points chacune, bonus de combo, un peu de vie rendue. |
| Vagues | Toutes les 22 s, l'essaim grossit et accélère. Colosses violets à partir de la vague 3. |
| Score | 10 points par seconde de survie + les captures. Le record est enregistré sur le téléphone. |

## Commandes

- **Android** : un doigt déplace la proie. **Pose un deuxième doigt n'importe où** pour tracer
  avec le premier ; lève-le pour arrêter. Bouton Retour : pause.
- **Ordinateur** : la souris est la proie, clic gauche maintenu pour tracer, `P` ou `Échap` pour
  la pause, `Entrée` pour lancer une partie.

Sur téléphone, poser le doigt sert déjà à déplacer la proie. Le second doigt permet donc de
choisir quand tracer, sans gaspiller d'encre à chaque déplacement.

## Structure

```
Cargo.toml                    dépendances + configuration Android (nom, icône, orientation)
src/main.rs                   tout le jeu
assets/fonts/                 Tilt Warp + Chakra Petch (licence SIL OFL, fichiers inclus)
android_res/mipmap-*/         icône de l'application
.github/workflows/android.yml compilation automatique de l'APK sur GitHub
```

## Essayer sur ordinateur

```bash
cargo run --release
```

## Obtenir l'APK

### Option A — GitHub, sans rien installer (recommandé)

1. Crée un dépôt GitHub et pousses-y ce dossier.
2. L'onglet **Actions** lance le workflow « APK Android ».
3. À la fin, télécharge l'artefact **essaim-apk** : il contient `essaim.apk`.

### Option B — Docker

Depuis ce dossier :

```bash
docker run --rm -v "$(pwd)":/root/src -w /root/src notfl3/cargo-apk \
  sh -c "rustup default stable && rustup target add armv7-linux-androideabi aarch64-linux-android x86_64-linux-android && cargo quad-apk build --release"
```

L'APK est écrit dans `target/android-artifacts/release/apk/`.

### Option C — installation locale

1. Installer Android Studio (ou les outils en ligne de commande), puis le SDK 33 et le NDK.
2. Définir `ANDROID_HOME` et `NDK_HOME`.
3. ```bash
   rustup target add armv7-linux-androideabi aarch64-linux-android x86_64-linux-android
   cargo install cargo-quad-apk
   cargo quad-apk build --release
   ```

### Installer sur le téléphone

- Par câble : `adb install -r target/android-artifacts/release/apk/essaim.apk`
- Ou copie l'APK sur le téléphone et ouvre-le (autoriser l'installation d'applications de sources
  inconnues).

Pour publier sur le Play Store, il faudra signer l'APK avec ta propre clé (`apksigner`) et
changer `package_name` dans `Cargo.toml` si besoin.

## Vérifier le rendu sans écran

Un pilote automatique joue une courte partie (trace une boucle, un mur, met en pause, perd) et
enregistre des captures :

```bash
mkdir -p captures && ESSAIM_AUTOTEST=captures cargo run
```
