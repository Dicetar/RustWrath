//! The Souls server link's inbox and the muted swings: the half of `benilla-souls`'s `server`
//! the core's chat handler ([`tap_addon`]) and animation driver ([`SwingMute`]) reach.

use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::prelude::*;

/// The addon prefix of every message on the link.
pub const PREFIX: &str = "RWSOULS";
/// The inbox keeps at most this many unread messages.
const INBOX_CAP: usize = 256;

/// Inbound message bodies (the text after `RWSOULS\t`), filled by the chat handler.
static INBOX: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Take an inbound addon text if it is the link's: `true` when it was (the chat log skips it).
pub fn tap_addon(text: &str) -> bool {
    let Some(body) = text
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix('\t'))
    else {
        return false;
    };
    if let Ok(mut inbox) = INBOX.lock() {
        if inbox.len() >= INBOX_CAP {
            inbox.pop_front();
        }
        inbox.push_back(body.to_owned());
    }
    true
}

/// Every unread message body, oldest first, taken out of the inbox.
pub fn drain_inbox() -> Vec<String> {
    INBOX
        .lock()
        .map(|mut inbox| inbox.drain(..).collect())
        .unwrap_or_default()
}

/// The bodies whose network swing animation is muted: ours while Souls combat is on (its swings
/// are the kit's clips, and the server's damage report would replay a stock swing over them), and
/// other Souls players' while their relayed actions play. Read by the animation driver.
#[derive(Resource, Default, Debug)]
pub struct SwingMute(pub Vec<Entity>);

impl SwingMute {
    pub fn muted(&self, entity: Entity) -> bool {
        self.0.contains(&entity)
    }
}
