import React, { useState } from 'react';

const ASTRA_PROMPT = `You are working on RustWrath (branch \`wotlk\`, WoW 3.3.5a build 12340) in \`D:\\Projects\\project-b-client\`.
First read \`AGENTS.md\`, \`docs/METHOD.md\`, \`docs/agents/active-ownership.md\`, and \`docs/agents/12340-reference.md\`.

## What was just landed on \`origin/wotlk\`
1. \`TaxiNodes.dbc\` (\`crates/benilla-formats/src/taxi_nodes.rs\`): supports 24-field 3.3.5a schema (12340 \`TaxiNodesRec::Read\` \`0x008B9E40\`, 16 locale columns, mounts at cols 22/23) alongside 16-field 1.12.
2. \`Exhaustion.dbc\` (\`crates/benilla-formats/src/exhaustion.rs\`): supports 23-field 3.3.5a schema (12340 \`ExhaustionRec::Read\`, 16 locale columns) alongside 15-field 1.12.
3. Wrath \`.skin\` parser (\`crates/benilla-m2/src/skin.rs\` & \`crates/benilla-formats/src/models/m2_batches.rs\`): \`parse_external_skin\` now accepts 48-byte (\`0x30\`) \`M2SkinProfile\` headers (\`M2Data.h:378\`, \`LoadSkinProfile\` \`0x0083CB40\`) so particle-only models (\`ui-autocastbutton00.skin\`, \`*_smoke00.skin\`) parse cleanly, and \`load_m2_mesh_skinned\` loads companion \`<stem>00.skin\` for v264 M2s.
4. 3.3.5a CVars & Natives: registered \`showKeyring\` (\`GameUI_CVar.cpp:608\`), \`RequestRealmSplitInfo\` (\`0x004DE2E0\`), and 12340 title natives (\`GetNumTitles\`, \`GetCurrentTitle\` returning \`0\`, \`IsTitleKnown\`, \`GetTitleName\`, \`SetCurrentTitle\`).
5. Diagnostic collector: \`tools/collect-diagnostics.ps1\` scans the latest \`benilla-config/Logs/Launcher/<run>\` and writes \`benilla-config/Logs/Launcher/latest-diagnostics.md\`.

## Your Task
1. Run \`git pull origin wotlk\` (preserving any local untracked files).
2. Check for running \`cargo\`/\`rustc\`/\`benilla\` processes first, then build and launch via \`Launch RustWrath.cmd\` (or inspect the latest human session in \`benilla-config/Logs/Launcher\`).
3. Run \`powershell -ExecutionPolicy Bypass -File tools/collect-diagnostics.ps1\` to generate \`benilla-config/Logs/Launcher/latest-diagnostics.md\`.
4. Triage every remaining item in \`latest-diagnostics.md\` against the 12340 reference (\`D:/Projects/wowdecomp-reference\`, pin \`e67b6ffef23655dc7da061cb2d36b7963f6ef71a\`):
   - Missing Lua globals / widget methods / frame fields
   - Missing CVars
   - Remaining 3.3.5a DBC field-count mismatches
   - Remaining M2 / BLP / WMO asset warnings
   - Northshire in-world verification: action-bar cast/attack (\`SMSG_SPELL_GO\` cooldown clear), bag toggle (\`B\` / backpack), loot clicks, target frame & tooltips.
5. Fix each confirmed root cause behind the 3.3.5a version boundary, add focused unit tests (<=20 test cases), and record source vs. build vs. live-session evidence.`;

export default function App() {
  const [copied, setCopied] = useState(false);

  const copyPrompt = () => {
    navigator.clipboard.writeText(ASTRA_PROMPT);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <main className="p-8 max-w-5xl mx-auto space-y-6">
      <header className="border-b border-stone-700 pb-4 flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight text-amber-400">RustWrath — Project B</h1>
          <p className="text-stone-400 text-sm mt-1">
            WoW 3.3.5a (Build 12340) Source Repairs & In-Game Diagnostic Collector
          </p>
        </div>
        <span className="px-3 py-1 rounded bg-emerald-950/70 border border-emerald-700/60 text-emerald-300 font-mono text-xs">
          branch: wotlk
        </span>
      </header>

      <section className="bg-stone-800 rounded-lg p-6 space-y-4 border border-stone-700">
        <h2 className="text-xl font-semibold text-stone-200">Implemented 12340 Source Repairs</h2>
        <div className="grid md:grid-cols-2 gap-4 text-sm">
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">1. 3.3.5a DBC Schemas</div>
            <p className="text-stone-300">
              <code className="text-emerald-300">TaxiNodes.dbc</code> now detects 24-field 12340 records
              (<code className="text-stone-400">0x008B9E40</code>, 16 locale columns, mounts at 22/23) and{' '}
              <code className="text-emerald-300">Exhaustion.dbc</code> detects 23-field 12340 records while
              preserving 1.12 schemas.
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">2. Wrath 48-Byte M2 .skin Profiles</div>
            <p className="text-stone-300">
              <code className="text-emerald-300">parse_external_skin</code> now accepts 48-byte (
              <code className="text-stone-400">0x30</code>) <code className="text-stone-400">M2SkinProfile</code>{' '}
              headers (<code className="text-stone-400">M2Data.h:378</code>) for particle-only models (
              <code className="text-stone-400">ui-autocastbutton00.skin</code>,{' '}
              <code className="text-stone-400">*_smoke00.skin</code>), and{' '}
              <code className="text-emerald-300">load_m2_mesh_skinned</code> loads companion{' '}
              <code className="text-stone-400">00.skin</code> for v264 M2s.
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">3. PaperDoll Title Natives & Glue Verbs</div>
            <p className="text-stone-300">
              Fixed <code className="text-emerald-300">GetCurrentTitle</code> (<code className="text-stone-400">0x0051B3B0</code>)
              to return <code className="text-amber-300">0</code> (not <code className="text-stone-400">nil</code>),
              and registered <code className="text-emerald-300">GetNumTitles</code>,{' '}
              <code className="text-emerald-300">IsTitleKnown</code>,{' '}
              <code className="text-emerald-300">GetTitleName</code>,{' '}
              <code className="text-emerald-300">SetCurrentTitle</code>, and Glue{' '}
              <code className="text-emerald-300">RequestRealmSplitInfo</code>.
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">4. Missing 3.3.5a CVars</div>
            <p className="text-stone-300">
              Registered <code className="text-emerald-300">showKeyring</code> (default{' '}
              <code className="text-amber-300">&quot;0&quot;</code>,{' '}
              <code className="text-stone-400">GameUI_CVar.cpp:608</code>) in both the host CVar table and{' '}
              <code className="text-stone-400">WRATH_UI_CVARS</code>.
            </p>
          </div>
        </div>
      </section>

      <section className="bg-stone-800 rounded-lg p-6 space-y-3 border border-stone-700">
        <h2 className="text-xl font-semibold text-stone-200">
          In-Game Error & Missing Element Collector (<code className="text-amber-300 text-base">tools/collect-diagnostics.ps1</code>)
        </h2>
        <p className="text-stone-300 text-sm">
          After launching the client via <code className="text-amber-300">Launch RustWrath.cmd</code>, run this
          PowerShell command to scan the latest launcher log and produce a categorized markdown report of missing Lua
          globals, widget methods, frame fields, CVars, DBC schema errors, and M2/BLP failures:
        </p>
        <pre className="bg-stone-950 text-emerald-300 p-3 rounded border border-stone-700 font-mono text-xs overflow-x-auto">
          powershell -ExecutionPolicy Bypass -File .\tools\collect-diagnostics.ps1
        </pre>
      </section>

      <section className="bg-stone-800 rounded-lg p-6 space-y-3 border border-stone-700">
        <div className="flex items-center justify-between">
          <h2 className="text-xl font-semibold text-stone-200">Ready-to-Paste Prompt for Astra / OpenCode</h2>
          <button
            onClick={copyPrompt}
            className="px-3 py-1.5 rounded bg-amber-500 hover:bg-amber-400 text-stone-950 font-medium text-xs transition cursor-pointer"
          >
            {copied ? 'Copied!' : 'Copy Prompt'}
          </button>
        </div>
        <pre className="bg-stone-950 text-stone-200 p-4 rounded border border-stone-700 font-mono text-xs whitespace-pre-wrap leading-relaxed">
          {ASTRA_PROMPT}
        </pre>
      </section>
    </main>
  );
}
