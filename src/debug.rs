use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};

use crate::stage::AssetHandles;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app
            // .add_systems(Update, print_position)
            // .add_systems(Update, check_asset_handles)
            .add_plugins((FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin::default()));
    }
}

fn print_position(query: Query<(Entity, &Transform)>) {
    // Log the entity ID and translation of each entity with a `Position` component.
    for (entity, transform) in query.iter() {
        info!(
            "Entity {:?} is at position {:?},",
            entity, transform.translation
        );
    }
}

fn check_asset_handles(asset_server: Res<AssetServer>, asset_handles: Res<AssetHandles>) {
    info!(
        "AssetStates: screen {:?}",
        asset_server.get_load_state(asset_handles.screen.id()),
    )
}
