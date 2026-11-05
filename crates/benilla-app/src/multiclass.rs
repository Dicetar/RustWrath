//! Resident owner of the opt-in WM addon transport. Lua never owns a socket or a receipt.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bevy::prelude::*;
use benilla_ui::mlua;
use benilla_protocol_wrath::{multiclass as wire, session::Phase, world::WorldConnection};
use benilla_ui::script::{MulticlassHost, UiScript, VmKind};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const PUSH_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Closed,
    Selection,
    World(u64),
}

struct Pending {
    request: wire::Request,
    wire_id: u32,
    deadline: Instant,
    dispatched: bool,
}

struct State {
    connection: u64,
    live: bool,
    next_wire_id: u32,
    scope: Scope,
    vm: Option<(u64, VmKind)>,
    pending: Option<Pending>,
    preview: Option<(wire::PreviewToken, Instant)>,
    owned_classes: u16,
    reply: Option<Vec<u8>>,
    pushes: VecDeque<Vec<u8>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            connection: 0, live: false, next_wire_id: 1, scope: Scope::Closed, vm: None,
            pending: None, preview: None, owned_classes: 0, reply: None, pushes: VecDeque::new(),
        }
    }
}

impl State {
    fn retire(&mut self, reason: &str) {
        if let Some(pending) = self.pending.take() {
            warn!("multiclass: retired op={} wire={} dispatched={} reason={reason}; no automatic replay",
                pending.request.operation, pending.wire_id, pending.dispatched);
        }
        self.reply = None;
        self.preview = None;
        self.pushes.clear();
    }

    fn expire(&mut self) {
        if self.preview.as_ref().is_some_and(|(_, deadline)| Instant::now() >= *deadline) {
            self.preview = None;
        }
        if self.pending.as_ref().is_some_and(|pending| Instant::now() >= pending.deadline) {
            if let Some(pending) = self.pending.take() {
                warn!("multiclass: timeout op={} wire={} dispatched={}; outcome unknown if dispatched, no replay",
                    pending.request.operation, pending.wire_id, pending.dispatched);
            }
        }
    }

    fn matches_vm(&self, vm: u64) -> bool {
        matches!((self.vm, self.scope),
            (Some((id, VmKind::Glue)), Scope::Selection) if id == vm)
            || matches!((self.vm, self.scope),
                (Some((id, VmKind::InGame)), Scope::World(_)) if id == vm)
    }
}

#[derive(Resource, Clone, Default)]
pub(crate) struct MulticlassBridge(Arc<Mutex<State>>);

impl MulticlassBridge {
    pub(crate) fn owned_classes(&self) -> u16 {
        self.0.lock().ok().filter(|state| state.live && matches!(state.scope, Scope::World(_)))
            .map_or(0, |state| state.owned_classes)
    }

    pub(crate) fn attach(&self, script: &UiScript) -> mlua::Result<()> {
        script.set_multiclass_host(Arc::new(self.clone()))?;
        if let Ok(mut state) = self.0.lock() {
            // Login pushes may arrive while the Glue VM still exists. Carry those to the
            // first world VM, but never carry a request or receipt across VM replacement.
            let pushes = if matches!(state.scope, Scope::World(_))
                && state.vm.is_none_or(|(_, kind)| kind == VmKind::Glue)
                && script.vm_kind() == VmKind::InGame
            {
                std::mem::take(&mut state.pushes)
            } else {
                VecDeque::new()
            };
            state.retire("vm-replaced");
            state.pushes = pushes;
            state.vm = Some((script.session(), script.vm_kind()));
        }
        Ok(())
    }

    pub(crate) fn detach(&self, vm: u64) {
        if let Ok(mut state) = self.0.lock() {
            if state.vm.is_some_and(|(active, _)| active == vm) {
                state.retire("vm-closed");
                state.vm = None;
            }
        }
    }

    pub(crate) fn suspend(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.retire("session-stopping");
            state.scope = Scope::Closed;
            state.live = false;
            state.owned_classes = 0;
        }
    }

    pub(crate) fn connect(&self) -> Connection {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.retire("connection-replaced");
        state.scope = Scope::Closed;
        state.live = true;
        state.owned_classes = 0;
        state.connection = state.connection.checked_add(1).expect("connection identity exhausted");
        info!("multiclass: opened connection={}", state.connection);
        Connection { bridge: self.clone(), generation: state.connection }
    }
}

impl MulticlassHost for MulticlassBridge {
    fn request(&self, vm: u64, arguments: [u32; 6]) -> bool {
        let Some(request) = wire::Request::from_arguments(arguments) else { return false };
        let Ok(mut state) = self.0.lock() else { return false };
        state.expire();
        // Both installed Lua consumers allow one request at a time. Reserve its receipt slot
        // at admission, so unsolicited state cannot displace a mutation result.
        if !state.live || !state.matches_vm(vm) || state.pending.is_some() || state.reply.is_some()
            || request.in_world() != matches!(state.scope, Scope::World(_))
        {
            debug!("multiclass: admission refused op={} vm={vm} scope={:?}", request.operation, state.scope);
            return false;
        }
        if request.operation == 3
            && !state.preview.as_ref().is_some_and(|(token, _)| token.permits(request))
        {
            debug!("multiclass: commit refused without current preview vm={vm}");
            return false;
        }
        let wire_id = state.next_wire_id;
        let Some(next) = wire_id.checked_add(1) else { return false };
        state.next_wire_id = next;
        if request.operation == 1 || request.operation == 2 {
            state.preview = None;
        }
        state.pending = Some(Pending {
            request, wire_id, deadline: Instant::now() + REQUEST_TIMEOUT, dispatched: false,
        });
        debug!("multiclass: admitted op={} wire={wire_id} vm={vm}", request.operation);
        true
    }

    fn poll(&self, vm: u64) -> Option<String> {
        let mut state = self.0.lock().ok()?;
        state.expire();
        if !state.matches_vm(vm) { return None; }
        state.reply.take().or_else(|| state.pushes.pop_front()).map(|bytes| wire::hex_reply(&bytes))
    }
}

/// Only the existing Wrath worker holds this lease. Drop covers every return, cancellation
/// and unwind; an old worker cannot close a replacement connection's bridge.
pub(crate) struct Connection {
    bridge: MulticlassBridge,
    generation: u64,
}

impl Connection {
    fn scope(&self, scope: Scope) {
        if let Ok(mut state) = self.bridge.0.lock() {
            if state.live && state.connection == self.generation && state.scope != scope {
                state.retire("scope-changed");
                state.scope = scope;
                debug!("multiclass: connection={} scope={scope:?}", self.generation);
            }
        }
    }

    pub(crate) fn selection(&self, available: bool) {
        self.scope(if available { Scope::Selection } else { Scope::Closed });
    }

    pub(crate) fn world(&self, phase: Phase, generation: u64) {
        self.scope(if phase == Phase::InWorld { Scope::World(generation) } else { Scope::Closed });
    }

    pub(crate) fn dispatch(&self, socket: &mut WorldConnection) -> anyhow::Result<()> {
        let packet = {
            let mut state = self.bridge.0.lock().map_err(|_| anyhow::anyhow!("multiclass bridge poisoned"))?;
            state.expire();
            if !state.live || state.connection != self.generation || state.scope == Scope::Closed {
                return Ok(());
            }
            let Some(pending) = state.pending.as_mut() else { return Ok(()) };
            if pending.dispatched { return Ok(()); }
            // This is the dispatch linearization point. A subsequent VM retirement treats it
            // as in flight, never as safely unsent. No mutex is held across socket IO.
            pending.dispatched = true;
            debug!("multiclass: dispatch op={} wire={}", pending.request.operation, pending.wire_id);
            pending.request.encode(pending.wire_id)
        };
        socket.send(wire::CLIENT_OPCODE, &packet)
    }

    pub(crate) fn receive(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let header = wire::ReplyHeader::decode(bytes)
            .ok_or_else(|| anyhow::anyhow!("invalid WMC1 v3 reply envelope"))?;
        let mut state = self.bridge.0.lock().map_err(|_| anyhow::anyhow!("multiclass bridge poisoned"))?;
        state.expire();
        if !state.live || state.connection != self.generation || state.scope == Scope::Closed { return Ok(()); }
        if header.operation >= 6 {
            if !matches!(state.scope, Scope::World(_)) { return Ok(()); }
            // BonusXpState includes a one-shot jackpot notification: it is not safe to
            // coalesce blindly. Overload ends this session through its normal recovery path.
            anyhow::ensure!(state.pushes.len() < PUSH_CAPACITY, "multiclass push queue overloaded");
            state.pushes.push_back(bytes.to_vec());
            return Ok(());
        }
        let matches = state.pending.as_ref().is_some_and(|pending| pending.dispatched
            && pending.wire_id == header.wire_id && pending.request.operation == header.operation);
        if !matches {
            debug!("multiclass: stale reply op={} wire={} discarded", header.operation, header.wire_id);
            return Ok(());
        }
        let pending = state.pending.take().expect("matched pending request");
        if matches!(header.operation, 4 | 5) && header.result == 0 {
            state.owned_classes = header.owned_classes(bytes)
                .ok_or_else(|| anyhow::anyhow!("invalid multiclass talent snapshot"))?;
        }
        if header.operation == 2 && header.result == 0 {
            let token = pending.request.preview_reply(bytes)
                .ok_or_else(|| anyhow::anyhow!("invalid multiclass preview receipt"))?;
            // Start before the round trip, conservatively within the server's 120-second TTL.
            state.preview = Some((token, pending.deadline - REQUEST_TIMEOUT + Duration::from_secs(120)));
        } else if header.operation == 3 && (header.result == 0 || header.result == 12) {
            state.preview = None;
        }
        let mut reply = bytes.to_vec();
        reply[8..12].copy_from_slice(&pending.request.serial.to_le_bytes());
        state.reply = Some(reply);
        if matches!(header.operation, 3 | 5) {
            info!("multiclass: mutation receipt op={} wire={} result={}", header.operation, header.wire_id, header.result);
        } else {
            debug!("multiclass: receipt op={} wire={} result={}", header.operation, header.wire_id, header.result);
        }
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Ok(mut state) = self.bridge.0.lock() {
            if state.connection == self.generation {
                state.retire("connection-closed");
                state.scope = Scope::Closed;
                state.live = false;
                state.owned_classes = 0;
                info!("multiclass: closed connection={}", self.generation);
            }
        }
    }
}

/// The WM DLL injects this owner-maintained script after stock GlueXML. RustWrath loads an
/// explicitly configured local file instead of copying it into game data or this repository.
pub(crate) fn load_glue(script: &UiScript) {
    let Some(path) = std::env::var_os("WOW_MULTICLASS_GLUE") else {
        warn!("multiclass: WOW_MULTICLASS_GLUE missing; character-selection controls unavailable");
        return;
    };
    let result = (|| -> anyhow::Result<()> {
        use std::io::Read;
        let file = std::fs::File::open(&path)?;
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
        anyhow::ensure!(!bytes.is_empty() && bytes.len() <= 256 * 1024, "Glue script size outside 1..256 KiB");
        script.run_chunk_named(&bytes, "@WM_MulticlassGlue")
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        anyhow::ensure!(script.eval::<bool>("return type(WM_MulticlassBoot) == 'table'")
            .map_err(|error| anyhow::anyhow!("{error}"))?, "Glue script did not create WM_MulticlassBoot");
        Ok(())
    })();
    match result {
        Ok(()) => info!("multiclass: character-selection script executed and boot frame exists"),
        Err(error) => {
            let message = format!("multiclass: Glue load failed: {error}");
            script.report_load_failure(&message);
            error!("{message}");
        }
    }
}
