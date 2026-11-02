import React, { useState } from 'react';

const ASTRA_PROMPT = `You are working on RustWrath (branch \`wotlk\`, WoW 3.3.5a build 12340) in \`D:\\Projects\\project-b-client\`.
First read \`AGENTS.md\`, \`docs/METHOD.md\`, \`docs/agents/active-ownership.md\`, and \`docs/agents/12340-reference.md\`.

## What was just landed on \`origin/wotlk\`
1. **14 Missing 3.3.5a CVars** (\`crates/benilla-app/src/cvars/table.rs\` & \`WRATH_UI_CVARS\` in \`lifecycle.rs\`):
   - Registered \`alwaysShowActionBars\`, \`trackerSorting\`, \`stopAutoAttackOnTargetChange\`, \`mapQuestDifficulty\`, \`showPartyPets\`, \`lastTalkedToGM\`, \`guildRecruitmentChannel\`, \`showClock\`, \`UnitNameFriendlyPlayerName\`, \`showNewbieTips\`, \`locale\`, \`previewTalents\`, \`fctSpellMechanics\`, and \`showDispelDebuffs\`.
2. **22 Missing Wrath UI Lua Globals & Stubs** (\`crates/benilla-ui/src/script/wrath_natives.rs\`):
   - \`GetExpansionLevel\` (returns 2 for WotLK, fixes \`LFDFrame.lua:1\` load crash that cascade-failed all LFD XML handlers)
   - \`GetAccountExpansionLevel\` (returns 2 for WotLK, fixes \`ReputationFrame.lua:25\`)
   - \`SetChatWindowUninteractable\` (\`FloatingChatFrame.lua:982\`)
   - \`GetAttackPowerForStat\` (\`PaperDollFrame.lua:272\`)
   - \`GetMirrorTimerInfo\` (\`MirrorTimer.lua:82\`)
   - \`CalendarGetNumPendingInvites\` (\`GameTime.lua:157\`)
   - \`IsXPUserDisabled\` (\`MainMenuBar.lua:344\`)
   - \`IsPossessBarVisible\` (\`BonusActionBarFrame.lua:221\`)
   - \`BNGetNumBlocked\` (\`FriendsFrame.lua:452\`)
   - \`GetNumVoiceSessions\` & \`IsVoiceChatAllowedByServer\`
   - \`GetTotemInfo\` (\`TotemFrame.lua:34\`, \`MultiCastActionBarFrame.lua:313\`)
   - \`GetRuneType\` (\`RuneFrame.lua:71\`, DK Blood/Unholy/Frost rune types)
   - \`GetHonorCurrency\` (\`PVPFrame.lua:515\`)
   - \`GetLFGRoles\` & \`UnitGroupRolesAssigned\` (\`PlayerFrame.lua:250\`)
   - \`GetNumMapDebugObjects\` & \`GetSelectedDisplayChannel\`
   - \`IsLoggedIn\` (AceAddon-3.0.lua:626) & \`GetPlayerFacing\` (Astrolabe-0.4.lua:370)
   - \`QueryGuildEventLog\` & \`RefreshLFGList\` XML handlers
   - \`WorldMapBlobFrame\` fallback frame for \`WorldMapFrame.lua:186\`
   - Fixed \`GetNumTrackedAchievements\` argument signature & added \`GetTrackedAchievements\`
3. **OnMinMaxChanged StatusBar Script Support** (\`crates/benilla-ui/src/script/object/events_regions.rs\`):
   - Added \`OnMinMaxChanged\` to recognized script kinds for 3.3.5a (fixes \`PetPaperDollFrameExpBar\`).
4. **All 23 3.3.5a (12340) DBC Schemas & Wrath M2 Particles**:
   - Fixed field counts and column layouts for all 23 DBCs and M2 skin/particle priority/spin.

## How to Rebuild and Launch:
1. Run \`git pull origin wotlk\`
2. Terminate any running \`benilla.exe\` process
3. Build with: \`cargo build -p benilla --features hotpatch\`
4. Run: \`Launch RustWrath.cmd\` (or \`cargo run -p benilla --features hotpatch\`)
5. Run: \`powershell -ExecutionPolicy Bypass -File .\\tools\\collect-diagnostics.ps1\` to verify clean diagnostics!`;

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
            <div className="font-mono text-amber-300 font-semibold">1. 14 Missing 3.3.5a CVars</div>
            <p className="text-stone-300">
              Registered all 14 missing CVars in the host table and <code className="text-emerald-300">WRATH_UI_CVARS</code>:{' '}
              <code className="text-stone-400">alwaysShowActionBars</code>,{' '}
              <code className="text-stone-400">trackerSorting</code>,{' '}
              <code className="text-stone-400">stopAutoAttackOnTargetChange</code>,{' '}
              <code className="text-stone-400">mapQuestDifficulty</code>,{' '}
              <code className="text-stone-400">showPartyPets</code>,{' '}
              <code className="text-stone-400">lastTalkedToGM</code>,{' '}
              <code className="text-stone-400">guildRecruitmentChannel</code>,{' '}
              <code className="text-stone-400">showClock</code>,{' '}
              <code className="text-stone-400">UnitNameFriendlyPlayerName</code>,{' '}
              <code className="text-stone-400">showNewbieTips</code>,{' '}
              <code className="text-stone-400">locale</code>,{' '}
              <code className="text-stone-400">previewTalents</code>,{' '}
              <code className="text-stone-400">fctSpellMechanics</code>, and{' '}
              <code className="text-stone-400">showDispelDebuffs</code>.
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">2. 22 Missing UI Globals &amp; Stubs</div>
            <p className="text-stone-300">
              Registered <code className="text-emerald-300">GetExpansionLevel</code> and{' '}
              <code className="text-emerald-300">GetAccountExpansionLevel</code> (returns 2, fixing the{' '}
              <code className="text-stone-400">LFDFrame.lua:1</code> crash that broke all LFD XML handlers),{' '}
              <code className="text-emerald-300">SetChatWindowUninteractable</code>,{' '}
              <code className="text-emerald-300">GetAttackPowerForStat</code>,{' '}
              <code className="text-emerald-300">GetMirrorTimerInfo</code>,{' '}
              <code className="text-emerald-300">CalendarGetNumPendingInvites</code>,{' '}
              <code className="text-emerald-300">IsXPUserDisabled</code>,{' '}
              <code className="text-emerald-300">IsPossessBarVisible</code>,{' '}
              <code className="text-emerald-300">BNGetNumBlocked</code>,{' '}
              <code className="text-emerald-300">GetRuneType</code>,{' '}
              <code className="text-emerald-300">GetTotemInfo</code>,{' '}
              <code className="text-emerald-300">GetLFGRoles</code>,{' '}
              <code className="text-emerald-300">UnitGroupRolesAssigned</code>,{' '}
              <code className="text-emerald-300">WorldMapBlobFrame</code>, and more.
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">3. OnMinMaxChanged &amp; Achievements</div>
            <p className="text-stone-300">
              Added <code className="text-emerald-300">OnMinMaxChanged</code> script kind for StatusBars in 3.3.5a{' '}
              (fixing <code className="text-stone-400">PetPaperDollFrameExpBar</code>) and broadened{' '}
              <code className="text-emerald-300">GetNumTrackedAchievements</code> argument handling to allow{' '}
              optional boolean parameters from <code className="text-stone-400">WatchFrame.lua:1306</code>.
            </p>
          </div>
          <div className="bg-stone-900/70 p-4 rounded border border-stone-700/80 space-y-1.5">
            <div className="font-mono text-amber-300 font-semibold">4. All 23 3.3.5a DBC Schemas</div>
            <p className="text-stone-300">
              Aligned field counts and multi-locale structures across all 23 DBC tables for 12340 and supported Wrath M2{' '}
              priority plane sorting and particle spin variation.
            </p>
          </div>
        </div>
      </section>

      <section className="bg-stone-800 rounded-lg p-6 space-y-3 border border-stone-700">
        <h2 className="text-xl font-semibold text-stone-200">
          Rebuilding &amp; Running Diagnostics
        </h2>
        <div className="space-y-2 text-stone-300 text-sm">
          <p>1. Pull the latest commits on <code className="text-emerald-300">wotlk</code>:</p>
          <pre className="bg-stone-950 text-emerald-300 p-2.5 rounded border border-stone-700 font-mono text-xs overflow-x-auto">
            git pull origin wotlk
          </pre>
          <p>2. Rebuild with sccache enabled (fast incremental):</p>
          <pre className="bg-stone-950 text-emerald-300 p-2.5 rounded border border-stone-700 font-mono text-xs overflow-x-auto">
            cargo build -p benilla --features hotpatch
          </pre>
          <p>3. Launch client via CMD or cargo:</p>
          <pre className="bg-stone-950 text-emerald-300 p-2.5 rounded border border-stone-700 font-mono text-xs overflow-x-auto">
            .\&quot;Launch RustWrath.cmd\&quot;
          </pre>
          <p>4. Collect clean diagnostics while the game is running:</p>
          <pre className="bg-stone-950 text-emerald-300 p-2.5 rounded border border-stone-700 font-mono text-xs overflow-x-auto">
            powershell -ExecutionPolicy Bypass -File .\tools\collect-diagnostics.ps1
          </pre>
        </div>
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
