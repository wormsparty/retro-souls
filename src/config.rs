//! Rechargement à chaud du tuning (`assets/config/*.ron`) : sauvegarder un fichier RON
//! pendant que le jeu tourne relance le combat avec les nouvelles valeurs.

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;

use crate::sim::ResetFight;
use crate::sim::data::{Tuning, TuningSources};

#[derive(Asset, TypePath, Debug)]
pub struct RonText(pub String);

#[derive(Default, TypePath)]
struct RonTextLoader;

impl AssetLoader for RonTextLoader {
    type Asset = RonText;
    type Settings = ();
    type Error = std::io::Error;
    async fn load(&self, reader: &mut dyn Reader, _: &(), _: &mut LoadContext<'_>) -> Result<RonText, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(RonText(String::from_utf8_lossy(&bytes).into_owned()))
    }
    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

#[derive(Resource)]
struct TuningFiles([Handle<RonText>; 6]);

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<RonText>()
            .register_asset_loader(RonTextLoader)
            .add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>| {
                commands.insert_resource(TuningFiles(
                    ["player", "weapons", "boss", "arena", "level", "enemies"].map(|n| server.load(format!("config/{n}.ron"))),
                ));
            })
            .add_systems(Update, reload);
    }
}

fn reload(
    mut events: MessageReader<AssetEvent<RonText>>,
    files: Res<TuningFiles>,
    texts: Res<Assets<RonText>>,
    mut tuning: ResMut<Tuning>,
    mut reset: ResMut<ResetFight>,
    mut last_hash: Local<Option<u64>>,
) {
    let changed = events.read().any(|e| matches!(e, AssetEvent::Modified { .. } | AssetEvent::LoadedWithDependencies { .. }));
    if !changed {
        return;
    }
    let Some(t) = files.0.iter().map(|h| texts.get(h).map(|t| t.0.as_str())).collect::<Option<Vec<_>>>() else {
        return;
    };
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    };
    if *last_hash == Some(hash) {
        return;
    }
    let src = TuningSources { player: t[0], weapons: t[1], boss: t[2], arena: t[3], level: t[4], enemies: t[5] };
    match Tuning::parse(&src) {
        Ok(new) => {
            // Le premier chargement est identique au tuning intégré : pas de reset.
            if last_hash.is_some() {
                info!("tuning rechargé");
                reset.requested = true;
            }
            *last_hash = Some(hash);
            *tuning = new;
        }
        Err(e) => error!("tuning invalide, conservé tel quel : {e}"),
    }
}
