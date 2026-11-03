import React, { useState } from 'react';

const TARGETED_TEST_COMMANDS = `# Run from D:\\Projects\\project-b-client (13 total test cases — strictly within 20-case ceiling)

# 1. G03: Build 12340 ItemStats_C count bounds (0, 10, >10 rejection without cursor desync, truncated) [1 test]
cargo test -p benilla-protocol-wrath --lib gameplay::tests::item_info_rejects_oversized_stat_count_without_cursor_desync

# 2. H00: Hotpatch Host::poll transient load failure retry without republishing ready.toml [3 tests]
cargo test -p benilla-hotpatch --lib

# 3. D01–D07 & Q00–Q04: Resident Flight Recorder, Objective Progress, Delayed Template & Turn-In Delta Correlation [9 tests]
cargo test -p benilla-app --lib quest_trace::tests`;

const H01_LIVE_RUNBOOK = `# H01 — Same-PID Live Pose Module Swap (A -> B -> A) Runbook
# Prerequisite: No competing cargo/benilla process; run from D:\\Projects\\project-b-client

# Step 1: Launch client with hotpatch enabled and record PID
$proc = Start-Process -FilePath "target\\debug\\benilla.exe" -PassThru
Write-Host "Launched benilla.exe with PID: $($proc.Id)"

# Step 2: Enter world with test character so an active animated rig is present

# Step 3: Publish Revision A (-LiftDemo) into the running process
powershell -ExecutionPolicy Bypass -File scripts\\publish-pose-module.ps1 -LiftDemo -Revision "h01-rev-a-lift"
Get-Content "$env:LOCALAPPDATA\\benilla\\hotpatch\\pose\\ready.toml"

# Step 4: Verify in client log & viewport that h01-rev-a-lift activated under the SAME PID ($proc.Id)

# Step 5: Publish Revision B (-PassThrough) into the same running process
powershell -ExecutionPolicy Bypass -File scripts\\publish-pose-module.ps1 -PassThrough -Revision "h01-rev-b-pass"
Get-Content "$env:LOCALAPPDATA\\benilla\\hotpatch\\pose\\ready.toml"

# Step 6: Re-publish Revision A (-LiftDemo) to complete A -> B -> A cycle under identical PID
powershell -ExecutionPolicy Bypass -File scripts\\publish-pose-module.ps1 -LiftDemo -Revision "h01-rev-a-return"
Get-Process -Id $proc.Id | Select-Object Id, ProcessName, StartTime`;

const Q05_LIVE_RUNBOOK = `# Q05 — Firna Northshire Connected Quest Route & Diagnostics Export Runbook
# Owner-driven live acceptance (Credentials & combat driven by owner)

# Step 1: Log in as Firna in Northshire Valley (Map 0, Elwynn/Northshire)
# Step 2: Interact with Deputy Willem / Marshal McBride:
#   - Right-click NPC -> verify Greeting/Detail panel opens (correlates Open -> GreetingPanel/DetailPanel)
#   - Click Accept -> verify QuestSlotAdded + JournalProjection + JournalRowInModel (or JournalTemplatePending -> JournalRowInModel)
# Step 3: Complete objective progress in Northshire:
#   - Verify QuestSlotProgressed counters advance in quest log and tracker
# Step 4: Return to questgiver -> Complete Quest -> Choose Reward:
#   - Verify RequestReward -> RewardPanel -> ChooseReward -> QuestCompletePacket
#   - Verify asynchronous turn-in deltas: QuestSlotRemoved + PlayerXpChanged + PlayerMoneyChanged + RewardItemPushed
# Step 5: Open Debug Overlay (Ctrl+Shift+D) -> Interaction Diagnostics -> Click "Save local report"
# Step 6: Inspect latest JSONL snapshot header & correlations:
Get-ChildItem "$env:LOCALAPPDATA\\benilla\\Logs\\InteractionDiagnostics\\interaction-*.jsonl" |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1 |
  ForEach-Object {
    Write-Host "Snapshot: $($_.FullName)"
    Get-Content $_.FullName -TotalCount 1 | ConvertFrom-Json |
      Select-Object schema, version, executable_file_sha1, received, dropped, evicted,
        action_summaries, panel_correlations, slot_correlations, turn_in_correlations
  }
`;

const WORKSTATION_SYNC_COMMAND = `# Option A: If downloaded to your Downloads folder:
git bundle verify "$HOME\\Downloads\\main-incremental.bundle"
git pull "$HOME\\Downloads\\main-incremental.bundle" main
git push origin main
Remove-Item "$HOME\\Downloads\\main-incremental.bundle"

# Option B: If saved directly in D:\\Projects\\project-b-client:
# git bundle verify .\\main-incremental.bundle
# git pull .\\main-incremental.bundle main
# git push origin main
# Remove-Item .\\main-incremental.bundle`;

const LEDGER_ROWS = [
  {
    pkg: 'D00–D07, Q00–Q02',
    title: 'Resident Interaction Flight Recorder, NPC Alias Redaction & Scope Retirement',
    status: 'S + B + T (7/7)',
    gap: 'R / V (Q05 Live Run)',
    files: 'quest_trace.rs, rfc-resident-quest-diagnostics.md',
  },
  {
    pkg: 'G03',
    title: 'Build 12340 ItemStats_C Count Bounds Rejection (<= 10) + Unit Fixture',
    status: 'S + B + T Ready (1 test)',
    gap: 'Run targeted G03 test command below',
    files: 'crates/benilla-protocol-wrath/src/gameplay.rs',
  },
  {
    pkg: 'G04–G05',
    title: 'Typed Outbound Command Admission & Refusal Receipts (incl. RequestReward & ChooseReward)',
    status: 'S + B + T Ready',
    gap: 'Covered by quest_trace & verbs tests',
    files: 'net.rs, net/io.rs, wrath.rs, ui_quest/verbs.rs',
  },
  {
    pkg: 'H00',
    title: 'Hotpatch Host::poll Failed-Manifest Retry State Machine + Unit Fixture',
    status: 'S + B + T Ready (3 tests)',
    gap: 'R / V: H01 Same-PID A->B->A swap',
    files: 'crates/benilla-hotpatch/src/lib.rs',
  },
  {
    pkg: 'Q03',
    title: 'Objective Progress (QuestSlotProgressed) & Delayed/Refused Template vs Journal Row Sync',
    status: 'S + T Ready (+1 test)',
    gap: 'R / V: Q05 Live Northshire route',
    files: 'wrath.rs, ui_quest_log.rs, quest_trace.rs',
  },
  {
    pkg: 'Q04',
    title: 'Turn-In & Asynchronous Reward Proof (QuestCompletePacket + Slot/XP/Money/Item Push Deltas)',
    status: 'S + T Ready (+1 test)',
    gap: 'R / V: Q05 Live Northshire route',
    files: 'ui_quest/verbs.rs, ui_quest/net.rs, wrath.rs, quest_trace.rs',
  },
];

export default function App() {
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const copyText = (id: string, text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  return (
    <main className="min-h-screen bg-stone-950 text-stone-100 p-6 md:p-10 max-w-6xl mx-auto space-y-8">
      <header className="border-b border-stone-800 pb-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2.5">
            <span className="px-2.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300 font-mono text-xs">
              WoW 3.3.5a (12340)
            </span>
            <span className="px-2.5 py-0.5 rounded bg-emerald-500/15 border border-emerald-500/40 text-emerald-300 font-mono text-xs">
              main@cbc8a717 (wotlk merged)
            </span>
          </div>
          <h1 className="text-2xl md:text-3xl font-bold tracking-tight text-stone-100 mt-2">
            RustWrath — Engineering Execution & Verification Console
          </h1>
          <p className="text-stone-400 text-sm mt-1">
            Active branch is now <code className="text-emerald-300">main</code> (tracking <code className="text-stone-300">origin/main</code>) · <code className="text-amber-300">wotlk</code> fully merged
          </p>
        </div>
        <div className="flex flex-col items-end gap-1">
          <span className="px-3 py-1.5 rounded bg-emerald-950/90 border border-emerald-700/60 text-emerald-300 font-mono text-xs">
            13 TARGETED TESTS (&lt; 20 CEILING)
          </span>
          <span className="text-xs text-stone-400 font-mono">
            Evidence Separation: S / B / T / R / V
          </span>
        </div>
      </header>

      {/* GitHub & Workstation Sync Banner */}
      <section className="bg-sky-950/40 border border-sky-600/40 rounded-lg p-5 space-y-3">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold text-sky-300 flex items-center gap-2">
              <span>⚡</span> Fast Workstation & GitHub Push
            </h2>
            <p className="text-stone-300 text-xs mt-0.5">
              Download the bundle via your browser (PowerShell <code className="text-amber-300">Invoke-WebRequest</code> gets blocked by Google Cloud Run login without cookies). Then run the pull command below:
            </p>
          </div>
          <div className="flex items-center gap-2.5 self-start sm:self-auto">
            <a
              href="/main-incremental.bundle"
              download="main-incremental.bundle"
              className="py-1.5 px-3.5 rounded bg-emerald-500 hover:bg-emerald-400 text-stone-950 font-semibold text-xs transition inline-flex items-center gap-1.5 shadow"
            >
              <span>📥</span> Download Bundle (1.4MB)
            </a>
            <button
              onClick={() => copyText('sync', WORKSTATION_SYNC_COMMAND)}
              className="py-1.5 px-3.5 rounded bg-sky-500 hover:bg-sky-400 text-stone-950 font-semibold text-xs transition cursor-pointer"
            >
              {copiedId === 'sync' ? '✓ Copied PowerShell Sync!' : 'Copy PowerShell Commands'}
            </button>
          </div>
        </div>
        <pre className="bg-stone-950/90 p-3 rounded border border-stone-800 text-sky-200 font-mono text-xs overflow-x-auto whitespace-pre-wrap leading-relaxed">
          {WORKSTATION_SYNC_COMMAND}
        </pre>
      </section>

      <section className="bg-stone-900/90 rounded-lg p-6 border border-stone-800 space-y-4">
        <h2 className="text-lg font-semibold text-amber-300">
          1. Current S/B/T/R/V Evidence Ledger
        </h2>
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse text-xs">
            <thead>
              <tr className="border-b border-stone-800 text-stone-400 font-mono">
                <th className="py-2.5 pr-4">Package</th>
                <th className="py-2.5 pr-4">Deliverable Summary</th>
                <th className="py-2.5 pr-4">Evidence State</th>
                <th className="py-2.5 pr-4">Remaining Gate</th>
                <th className="py-2.5">Owned Files</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-stone-800/70">
              {LEDGER_ROWS.map((row) => (
                <tr key={row.pkg} className="hover:bg-stone-800/40">
                  <td className="py-3 pr-4 font-mono font-semibold text-amber-300 whitespace-nowrap">
                    {row.pkg}
                  </td>
                  <td className="py-3 pr-4 text-stone-200">{row.title}</td>
                  <td className="py-3 pr-4 font-mono text-emerald-300 whitespace-nowrap">
                    {row.status}
                  </td>
                  <td className="py-3 pr-4 text-stone-300">{row.gap}</td>
                  <td className="py-3 font-mono text-stone-400">{row.files}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bg-stone-900/90 rounded-lg p-6 border border-emerald-500/30 space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-emerald-300">
              2. Close Out T-Level Gates (G03, H00, D01–D07, Q00–Q04)
            </h2>
            <p className="text-stone-400 text-xs mt-0.5">
              Executes 13 targeted unit tests total (1 in <code className="text-stone-200">benilla-protocol-wrath</code>, 3 in <code className="text-stone-200">benilla-hotpatch</code>, 9 in <code className="text-stone-200">benilla-app::quest_trace::tests</code>).
            </p>
          </div>
          <button
            onClick={() => copyText('tests', TARGETED_TEST_COMMANDS)}
            className="py-2 px-4 rounded bg-emerald-500 hover:bg-emerald-400 text-stone-950 font-semibold text-xs transition cursor-pointer"
          >
            {copiedId === 'tests' ? '✓ Copied Test Commands!' : 'Copy Targeted Test Commands'}
          </button>
        </div>
        <pre className="bg-stone-950 p-4 rounded border border-stone-800 text-stone-200 font-mono text-xs overflow-x-auto whitespace-pre-wrap leading-relaxed">
          {TARGETED_TEST_COMMANDS}
        </pre>
      </section>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <section className="bg-stone-900/90 rounded-lg p-6 border border-amber-500/30 space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-base font-semibold text-amber-300">
              3. H01 Runbook — Live Same-PID Pose Swap (A → B → A)
            </h2>
            <button
              onClick={() => copyText('h01', H01_LIVE_RUNBOOK)}
              className="py-1.5 px-3 rounded bg-amber-500 hover:bg-amber-400 text-stone-950 font-semibold text-xs transition cursor-pointer"
            >
              {copiedId === 'h01' ? '✓ Copied H01!' : 'Copy H01 Runbook'}
            </button>
          </div>
          <pre className="bg-stone-950 p-4 rounded border border-stone-800 text-stone-200 font-mono text-xs overflow-x-auto whitespace-pre-wrap leading-relaxed">
            {H01_LIVE_RUNBOOK}
          </pre>
        </section>

        <section className="bg-stone-900/90 rounded-lg p-6 border border-amber-500/30 space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-base font-semibold text-amber-300">
              4. Q05 Runbook — Firna Northshire Live Acceptance
            </h2>
            <button
              onClick={() => copyText('q05', Q05_LIVE_RUNBOOK)}
              className="py-1.5 px-3 rounded bg-amber-500 hover:bg-amber-400 text-stone-950 font-semibold text-xs transition cursor-pointer"
            >
              {copiedId === 'q05' ? '✓ Copied Q05!' : 'Copy Q05 Runbook'}
            </button>
          </div>
          <pre className="bg-stone-950 p-4 rounded border border-stone-800 text-stone-200 font-mono text-xs overflow-x-auto whitespace-pre-wrap leading-relaxed">
            {Q05_LIVE_RUNBOOK}
          </pre>
        </section>
      </div>
    </main>
  );
}

