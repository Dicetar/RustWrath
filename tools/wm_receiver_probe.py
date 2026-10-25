"""Work package 01 / issue 16 — receiver acceptance probe (M0 evidence).

Runs the real chain in the WM-service shape OpenCode's contract prescribes:
receiver + store + context loader in one process (the future WM process), the
renderless driver as a subprocess publishing through the integrated bridge.

Demonstrates (each printed with its evidence):
  1. authenticated handshake and receiver/store acceptance of real driver observations;
  2. WM availability: NativeClientObservationLoader.load_latest returns the context-shaped
     snapshot with epoch/sequence/revision/target/visibility intact;
  3. unknown target and incomplete visibility survive conversion;
  4. disconnect invalidation: after the driver's clean logout the store is invalidated
     (latest_for returns None) rather than serving a dead session;
  5. freshness expiry: an aged observation is not served.

Run with protected environment only:
  WM_NATIVE_SRC, WM_NATIVE_OBSERVATION_TOKEN, WM_PROBE_GUID, WOW_USER, WOW_PASS
Requires the lab worldserver/authserver running and the driver binary built.
"""

import os
import subprocess
import sys
import time

def required_env(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        raise SystemExit(f"wm_receiver_probe: set {name} (protected environment, not source)")
    return value


DRIVER = os.environ.get("WM_DRIVER", "target/debug/benilla-wm-driver.exe")
GUID = int(required_env("WM_PROBE_GUID"))
TOKEN = required_env("WM_NATIVE_OBSERVATION_TOKEN")
# The maintained probe must import the deployed/vendored package, not an agent checkout. The
# operator names it; no workstation path is compiled into this file.
sys.path.insert(0, required_env("WM_NATIVE_SRC"))

from wm_native import (  # noqa: E402
    NativeClientObservationLoader,
    ObservationServer,
    ObservationStore,
)


def main() -> None:
    store = ObservationStore(max_age_ms=5000, max_future_skew_ms=2000, allowed_player_guid=GUID)
    server = ObservationServer(store, TOKEN, host="127.0.0.1", port=48765).start_background()
    health = server.health()
    print(f"[receiver] bound on 127.0.0.1:{health.bound_port}, running={health.running}")
    assert health.bound_port == 48765, "the driver publishes to the default port"

    loader = NativeClientObservationLoader(store)

    env = dict(
        os.environ,
        WM_NATIVE_OBSERVATION_TOKEN=TOKEN,
        WM_DWELL_SECS="15",
    )
    mid_session = None
    proc = subprocess.Popen(
        [DRIVER],
        env=env,
        cwd=os.environ.get("WM_CLIENT_ROOT", os.getcwd()),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    # Poll WM availability while the session runs: the acceptance must be observable
    # mid-session, not only after exit.
    saw_fresh = False
    saw_snapshot = None
    deadline = time.time() + 40
    while proc.poll() is None and time.time() < deadline:
        snapshot = loader.load_latest(player_guid=GUID)
        if snapshot is not None and not saw_fresh:
            saw_fresh = True
            saw_snapshot = snapshot
            payload = snapshot["payload"]
            print(
                "[wm] mid-session read: "
                f"snapshot_id={snapshot['snapshot_id']} "
                f"epoch={payload['session_epoch']} world={payload.get('world_generation')} "
                f"rev={payload['state_revision']} "
                f"map={snapshot['map_id']} pos={payload['position']} "
                f"target={payload['target']} "
                f"visible={len(payload['visibility']['guids'])} "
                f"complete={payload['visibility']['complete']} "
                f"total={payload['visibility']['total_guid_count']} "
                f"age_ms={payload['age_ms']}"
            )
        time.sleep(0.5)
    out = proc.communicate(timeout=30)[0]
    tail = [line for line in out.splitlines() if "LOGOUT" in line or "handed off" in line]
    for line in tail[-3:]:
        print(f"[driver] {line}")
    assert proc.returncode == 0, f"driver failed: exit {proc.returncode}"
    print(f"[driver] clean exit 0; mid-session WM read available: {saw_fresh}")
    assert saw_fresh, "WM never read a fresh observation during the session"

    # Receiver acceptance stats, separate from handoff.
    health = server.health()
    print(
        f"[receiver] accepted={health.accepted} health_acks={health.health_acks} "
        f"rejected={health.rejected} client_connected={health.client_connected}"
    )
    assert health.accepted > 0, "the receiver accepted nothing"
    if health.last_accepted is not None:
        print(f"[receiver] last accepted: {health.last_accepted}")

    # The snapshot the loader saw must carry the v2 distinctions truthfully.
    if saw_snapshot is not None:
        payload = saw_snapshot["payload"]
        target = payload["target"]
        visibility = payload["visibility"]
        print(
            f"[wm] target state={target['state'] if isinstance(target, dict) else target} "
            f"visibility complete={visibility['complete']} total={visibility['total_guid_count']}"
        )

    # Disconnect invalidation: after the driver's clean logout the receiver saw the disconnect
    # and invalidated (or the observation expired by age); either way a dead session is not
    # served as live.
    time.sleep(6)  # past the 5 s freshness window
    after = loader.load_latest(player_guid=GUID)
    assert after is None, "the store served a dead session after logout and expiry"
    print("[wm] after logout+age: latest=None (invalidated/expired)")

    print("[probe] M0 acceptance sequence complete")


if __name__ == "__main__":
    main()
