//! Weather sound: the weather state's ambience loop is an input to the one zone-ambience channel,
//! not a loop of its own. `SMSG_WEATHER` names a `Weather.dbc` row, and the row's `m_ambienceID`
//! is the `SoundEntries` kit (reference: the `SMSG_WEATHER` handler reads `m_ambienceID` and calls
//! `SetWeatherKitID`, which fills zone-sound slot 8). A change to clear skies sets -1, silence.
//!
//! The selector (`GetDesiredAmbienceKitID`) returns the weather kit while the player is outside
//! (slot 8 is skipped when `sm_bPlayerInside` is set by the WMO audio update) and the area's
//! `AmbienceID` while inside, each swap a 5.0 s crossfade: rain is replaced indoors, not ducked.
//! The packet's grade drives rendering only. The channel itself is `super::zone`'s.

use bevy::prelude::*;

use benilla_world::schedule::WorldStage;
use benilla_world::weather::{WeatherMessage, WeatherTable};

/// The weather loop's SoundEntries kit from the last `SMSG_WEATHER`'s `Weather.dbc` row, 0 for
/// clear skies or an unknown state.
#[derive(Resource, Default)]
pub(super) struct WeatherAmbience(pub(super) u32);

/// Track the wire's weather state and resolve its ambience kit through `Weather.dbc`.
fn track_weather_kit(
    mut kit: ResMut<WeatherAmbience>,
    // Absent in an app without the world crate's weather plugin: no kit is claimed.
    table: Option<Res<WeatherTable>>,
    mut msgs: MessageReader<WeatherMessage>,
) {
    for m in msgs.read() {
        let sound = table.as_deref().map_or(0, |t| t.ambience(m.weather_id));
        if kit.0 != sound {
            info!(
                "weather sound kit {sound} (state {}, grade {:.2})",
                m.weather_id, m.grade
            );
            kit.0 = sound;
        }
    }
}

/// Registration hook for [`super::SoundPlugin`].
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<WeatherAmbience>()
        .add_systems(Update, track_weather_kit.in_set(WorldStage::Present));
}
