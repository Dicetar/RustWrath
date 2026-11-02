import React, { useState } from 'react';

const ASTRA_PROMPT = `You are working on RustWrath (branch \`wotlk\`, WoW 3.3.5a build 12340) in \`D:\\Projects\\project-b-client\`.
First read \`AGENTS.md\`, \`docs/METHOD.md\`, \`docs/agents/active-ownership.md\`, and \`docs/agents/12340-reference.md\`.

## What was just landed on \`origin/wotlk\`
1. **All 23 3.3.5a (12340) DBC Schemas** (\`crates/benilla-formats/src/\`):
   - \`TaxiNodes.dbc\` (24 fields), \`Exhaustion.dbc\` (23 fields), \`AreaPOI.dbc\` (54 fields), \`WorldStateUI.dbc\` (63 fields), \`LFGDungeons.dbc\` (49 fields), \`MailTemplate.dbc\` (35 fields), \`ItemSet.dbc\` (53 fields), \`ItemSubClass.dbc\` (44 fields) + \`ItemSubClassMask.dbc\` (19 fields), \`TalentTab.dbc\` (24 fields) + \`Talent.dbc\` (23 fields), \`CreatureFamily.dbc\` (28 fields) + \`ItemPetFood.dbc\` (18 fields), \`PetPersonality.dbc\` (24 fields) + \`PetLoyalty.dbc\` (18 fields), \`AuctionHouse.dbc\` (21 fields), \`ItemClass.dbc\` (20 fields), \`Package.dbc\` (20 fields), \`Resistances.dbc\` (20 fields), \`CreatureType.dbc\` (19 fields), \`Languages.dbc\` (18 fields), \`GameTips.dbc\` (18 fields), \`GMTicketCategory.dbc\` (18 fields), \`ServerMessages.dbc\` (18 fields), \`ItemBagFamily.dbc\` (18 fields), and \`EmotesTextData.dbc\` (18 fields).
2. **Wrath M2 \`.skin\` & Particle Runtime Support**:
   - \`parse_external_skin\` accepts 48-byte (\`0x30\`) \`M2SkinProfile\` headers (\`ui-autocastbutton00.skin\`, \`*_smoke00.skin\`) and \`load_m2_mesh_skinned\` loads companion \`<stem>00.skin\` for v264 M2s.
   - \`wrath_particles.rs\`, \`quads.rs\`, and \`sim.rs\` now support 12340 particle \`priority_plane\` sort bias (\`ui_mainmenu_northrend.m2\`) and \`initial_spin\` / \`initial_spin_variation\` / \`spin\` / \`spin_variation\` (\`ui_nightelf.m2\`).
3. **3.3.5a CVars & Natives**: registered \`showKeyring\` (\`GameUI_CVar.cpp:608\`), \`RequestRealmSplitInfo\` (\`0x004DE2E0\`), and 12340 title natives.
4. **Diagnostic Collector**: \`tools/collect-diagnostics.ps1\` scans the latest \`benilla-config/Logs/Launcher/<run>\` and writes \`benilla-config/Logs/Launcher/latest-diagnostics.md\`.

## Your Task
1. Run \`git pull origin wotlk\` (preserving local uncommitted edits).
2. Check for running \`cargo\`/\`rustc\`/\`benilla\` processes first, then rebuild and launch via \`Launch RustWrath.cmd\`.
3. Run \`powershell -ExecutionPolicy Bypass -File .\\tools\\collect-diagnostics.ps1\` on the new run directory to verify that all 23 DBC schema errors and the M2 skin/particle errors are resolved.
4. Triage any newly surfaced in-world issues in \`latest-diagnostics.md\` against \`D:/Projects/wowdecomp-reference\` (pin \`e67b6ffef23655dc7da061cb2d36b7963f6ef71a\`).`;

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
            <div className="font-mono text-amber-300 font-semibold">1. All 23 3.3.5a DBC Schemas</div>
            <p className="text-stone-300">
              Added 12340 field-count &amp; column detection for{' '}
              <code className="text-emerald-300">TaxiNodes</code> (24),{' '}
              <code className="text-emerald-300">Exhaustion</code> (23),{' '}
              <code className="text-emerald-300">AreaPOI</code> (54),{' '}
              <code className="text-emerald-300">WorldStateUI</code> (63),{' '}
              <code className="text-emerald-300">LFGDungeons</code> (49),{' '}
              <code className="text-emerald-300">MailTemplate</code> (35),{' '}
              <code className="text-emerald-300">ItemSet</code> (53),{' '}
              <code className="text-emerald-300">ItemSubClass</code> (44),{' '}
              <code className="text-emerald-300">TalentTab</code> (24),{' '}
              <code className="text-emerald-300">CreatureFamily</code> (28),{' '}
              <code className="text-emerald-300">PetPersonality</code> (24),{' '}
              <code className="text-emerald-300">AuctionHouse</code> (21),{' '}
              <code className="text-emerald-300">ItemClass</code> (20),{' '}
              <code className="text-emerald-300">Package</code> (20),{' '}
              <code className="text-emerald-300">Resistances</code> (20),{' '}
              <code className="text-emerald-300">CreatureType</code> (19),{' '}
              <code className="text-emerald-300">Languages</code> (18),{' '}
              <code className="text-emerald-300">GameTips</code> (18),{' '}
              <code className="text-emerald-300">GMTicketCategory</code> (18),{' '}
              <code className="text-emerald-300">ServerMessages</code> (18),{' '}
              <code className="text-emerald-300">ItemBagFamily</code> (18), and{' '}
              <code className="text-emerald-300">EmotesTextData</code> (18).
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">2. Wrath M2 .skin &amp; Particle Runtime</div>
            <p className="text-stone-300">
              <code className="text-emerald-300">parse_external_skin</code> accepts 48-byte (
              <code className="text-stone-400">0x30</code>) <code className="text-stone-400">M2SkinProfile</code>{' '}
              headers (<code className="text-stone-400">ui-autocastbutton00.skin</code>), and{' '}
              <code className="text-emerald-300">wrath_particles</code> +{' '}
              <code className="text-emerald-300">benilla-world</code> now support{' '}
              <code className="text-emerald-300">priority_plane</code> sort bias (
              <code className="text-stone-400">ui_mainmenu_northrend.m2</code>) and spin variation (
              <code className="text-stone-400">ui_nightelf.m2</code>).
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">3. PaperDoll Title Natives &amp; Glue Verbs</div>
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
