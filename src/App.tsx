import React, { useState } from 'react';

interface PreppedTicket {
  id: string;
  category: string;
  title: string;
  label: string;
  targetFiles: string[];
  evidence: string[];
  prompt: string;
}

const AUTO_CATCHER_WORKFLOW = `# From D:\\Projects\\project-b-client:
# 1. Pull latest auto-catcher & exit-snapshot updates:
git pull origin main

# 2. Build client once with hotpatch + dev diagnostics:
cargo build -p benilla --features hotpatch

# 3. Launch the game normally (or double-click "Launch RustWrath.cmd"):
powershell -ExecutionPolicy Bypass -File tools\\launch-client.ps1

# What happens automatically while you play & when you close the window:
# - In-game incidents auto-export every 10s to benilla-config\\Logs\\InteractionDiagnostics\\
# - Closing the window (Alt+F4 / Quit) flushes a synchronous final flight-recorder snapshot
# - A background watcher automatically runs tools\\collect-diagnostics.ps1 on exit and generates:
#     * benilla-config\\Logs\\Launcher\\latest-tickets.md      (Ready-to-resolve repair queue!)
#     * benilla-config\\Logs\\Launcher\\latest-diagnostics.md  (Full subsystem + flight recorder report)
#     * benilla-config\\Logs\\Launcher\\latest-diagnostics.json
# - Or double-click "Collect Diagnostics.cmd" at any time while the game is still open!`;

const TARGETED_TEST_COMMANDS = `# Run from D:\\Projects\\project-b-client (13 total test cases — strictly within 20-case ceiling)

# 1. G03: Build 12340 ItemStats_C count bounds (0, 10, >10 rejection without cursor desync, truncated) [1 test]
cargo test -p benilla-protocol-wrath --lib gameplay::tests::item_info_rejects_oversized_stat_count_without_cursor_desync

# 2. H00: Hotpatch Host::poll transient load failure retry without republishing ready.toml [3 tests]
cargo test -p benilla-hotpatch --lib

# 3. D01–D07 & Q00–Q04: Resident Flight Recorder, Objective Progress, Delayed Template & Turn-In Delta Correlation [9 tests]
cargo test -p benilla-app --lib quest_trace::tests`;

function parseRawLogsOrJsonToTickets(raw: string): PreppedTicket[] {
  const trimmed = raw.trim();
  if (!trimmed) return [];

  // Check if user pasted latest-diagnostics.json directly
  if (trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed.readyTickets)) {
        return parsed.readyTickets;
      }
      if (parsed.schema === 'rustwrath.interaction-diagnostics.v1') {
        const tickets: PreppedTicket[] = [];
        let idx = 1;
        const incLines: string[] = [];
        if (Array.isArray(parsed.incidents)) {
          for (const item of parsed.incidents) {
            const rec = item.record || item;
            incLines.push(
              `outcome=${rec.outcome} confidence=${rec.confidence} stage=${JSON.stringify(rec.terminal_stage)} subject=${JSON.stringify(rec.subject)}`
            );
          }
        }
        if (incLines.length > 0) {
          tickets.push({
            id: `AUTO-0${idx++}`,
            category: 'Flight Recorder / Incidents',
            title: `Triage and resolve ${incLines.length} recorded interaction incident(s)`,
            label: 'ready-for-agent',
            targetFiles: [
              'crates/benilla-app/src/quest_trace.rs',
              'crates/benilla-app/src/wrath.rs',
              'crates/benilla-app/src/ui_quest/',
            ],
            evidence: incLines.slice(0, 15),
            prompt: `Inspect the ${incLines.length} flight-recorder incidents (${incLines.slice(0, 3).join('; ')}), trace the terminal_stage and action chain, and fix the underlying client refusal or decode rejection.`,
          });
        }
        const questAnomalies: string[] = [];
        for (const key of [
          'slot_correlations',
          'turn_in_correlations',
          'panel_correlations',
          'quest_ui_correlations',
        ]) {
          if (Array.isArray(parsed[key])) {
            for (const line of parsed[key]) {
              if (
                /mismatch|unverified|incomplete|ambiguous|refused|failed|without a retained/i.test(
                  line
                )
              ) {
                questAnomalies.push(`[${key}] ${line}`);
              }
            }
          }
        }
        if (questAnomalies.length > 0) {
          tickets.push({
            id: `AUTO-0${idx++}`,
            category: 'Quests / Correlations',
            title: `Resolve ${questAnomalies.length} quest/journal/turn-in correlation anomaly(ies)`,
            label: 'ready-for-agent',
            targetFiles: [
              'crates/benilla-app/src/ui_quest/',
              'crates/benilla-app/src/ui_quest_log.rs',
              'crates/benilla-protocol-wrath/src/gameplay.rs',
            ],
            evidence: questAnomalies.slice(0, 15),
            prompt: `Fix the quest/journal/turn-in correlation anomalies captured by the flight recorder: ${questAnomalies.slice(0, 4).join(' | ')}`,
          });
        }
        return tickets;
      }
    } catch {
      // Fall through to line-by-line log + JSONL parser
    }
  }

  const lines = trimmed.split(/\r?\n/);
  const missingGlobals = new Map<string, number>();
  const missingMethods = new Map<string, number>();
  const missingFields = new Map<string, number>();
  const missingCvars = new Map<string, number>();
  const luaErrors = new Map<string, number>();
  const dbcFailures = new Map<string, number>();
  const assetFailures = new Map<string, number>();
  const diagProbes = new Map<string, Map<string, number>>();
  const flightIncidents: string[] = [];

  for (const rawLine of lines) {
    const line = rawLine.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '').trim();
    if (!line) continue;

    const diagMatch = line.match(/diag\[([A-Z_]+)\]\s*(.*)/);
    if (diagMatch) {
      const tag = diagMatch[1];
      const detail = diagMatch[2].trim();
      if (!diagProbes.has(tag)) diagProbes.set(tag, new Map());
      const m = diagProbes.get(tag)!;
      m.set(detail, (m.get(detail) || 0) + 1);
    }

    if (line.startsWith('{"schema":"rustwrath.interaction-diagnostics.v1"')) {
      try {
        const header = JSON.parse(line);
        if (Array.isArray(header.incidents)) {
          for (const item of header.incidents) {
            const rec = item.record || item;
            flightIncidents.push(
              `outcome=${rec.outcome} confidence=${rec.confidence} stage=${JSON.stringify(rec.terminal_stage)}`
            );
          }
        }
      } catch {
        // ignore malformed jsonl header
      }
      continue;
    }

    const gMatch = line.match(
      /attempt to call global ['`"]([^'`"]+)['`"] \(a nil value\)/
    );
    if (gMatch) {
      missingGlobals.set(gMatch[1], (missingGlobals.get(gMatch[1]) || 0) + 1);
    }

    const mMatch = line.match(
      /attempt to call method ['`"]([^'`"]+)['`"] \(a nil value\)/
    );
    if (mMatch) {
      missingMethods.set(mMatch[1], (missingMethods.get(mMatch[1]) || 0) + 1);
    }

    const fMatch = line.match(
      /attempt to index (?:field|global) ['`"]([^'`"]+)['`"] \(a nil value\)/
    );
    if (fMatch) {
      missingFields.set(fMatch[1], (missingFields.get(fMatch[1]) || 0) + 1);
    }

    const cMatch = line.match(/Couldn't find CVar named '([^']+)'/);
    if (cMatch) {
      missingCvars.set(cMatch[1], (missingCvars.get(cMatch[1]) || 0) + 1);
    }

    const stripped = line.replace(
      /^\d{4}-\d{2}-\d{2}T\S+\s+(TRACE|DEBUG|INFO|WARN|ERROR)\s+/,
      ''
    );

    if (
      /(DBC|\.dbc|schema field count|record size|Spell\.dbc|ChrClasses|ChrRaces)/i.test(
        line
      ) &&
      /\b(ERROR|WARN)\b/.test(line)
    ) {
      dbcFailures.set(stripped, (dbcFailures.get(stripped) || 0) + 1);
    }

    if (
      /(Failed to load asset|parsing skin|companion skin|not a SKIN|\.skin|\.m2|\.blp|\.wmo|\.adt)/i.test(
        line
      ) &&
      /\b(ERROR|WARN)\b/.test(line)
    ) {
      assetFailures.set(stripped, (assetFailures.get(stripped) || 0) + 1);
    }

    if (
      /(Interface\\|FrameXML|GlueXML|runtime error:|Lua error|ui_script:)/i.test(
        line
      ) &&
      (/\b(ERROR|WARN)\b/.test(line) || /runtime error:/.test(line))
    ) {
      luaErrors.set(stripped, (luaErrors.get(stripped) || 0) + 1);
    }
  }

  const tickets: PreppedTicket[] = [];
  let idx = 1;
  const formatMap = (m: Map<string, number>) =>
    Array.from(m.entries())
      .sort((a, b) => b[1] - a[1])
      .map(([k, v]) => `${k} (x${v})`);

  if (missingGlobals.size > 0) {
    const names = Array.from(missingGlobals.keys()).slice(0, 10).join(', ');
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'UI / Lua Globals',
      title: `Implement missing Wrath Lua global functions (${missingGlobals.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: [
        'crates/benilla-ui/src/script/',
        'crates/benilla-app/src/ui_script/',
      ],
      evidence: formatMap(missingGlobals).slice(0, 15),
      prompt: `Implement the missing 3.3.5a Lua global functions observed at runtime (${names}) in their owning script module under crates/benilla-ui/src/script/ or crates/benilla-app/src/, following the 12340 reference contract.`,
    });
  }

  if (missingMethods.size > 0) {
    const names = Array.from(missingMethods.keys()).slice(0, 10).join(', ');
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'UI / Widget Methods',
      title: `Implement missing FrameXML widget methods (${missingMethods.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: ['crates/benilla-ui/src/script/widget/'],
      evidence: formatMap(missingMethods).slice(0, 15),
      prompt: `Implement the missing FrameXML widget methods (${names}) on the appropriate widget tables in crates/benilla-ui/src/script/widget/ against the 12340 reference.`,
    });
  }

  if (missingFields.size > 0) {
    const names = Array.from(missingFields.keys()).slice(0, 10).join(', ');
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'UI / Frame & Table Fields',
      title: `Resolve nil field/global indexing (${missingFields.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: [
        'crates/benilla-ui/src/script/',
        'crates/benilla-app/src/ui_script/',
      ],
      evidence: formatMap(missingFields).slice(0, 15),
      prompt: `Trace and fix the nil field/global accesses (${names}) in crates/benilla-ui/src/script/ or crates/benilla-app/src/ui_script/ using the logged Lua stack lines.`,
    });
  }

  if (missingCvars.size > 0) {
    const names = Array.from(missingCvars.keys()).slice(0, 10).join(', ');
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'CVars',
      title: `Register missing 3.3.5a CVars (${missingCvars.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: ['crates/benilla-app/src/cvars/'],
      evidence: formatMap(missingCvars).slice(0, 15),
      prompt: `Register the missing 3.3.5a CVars (${names}) with their stock 12340 default values in crates/benilla-app/src/cvars/.`,
    });
  }

  if (luaErrors.size > 0) {
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'UI / Lua Runtime',
      title: `Fix Lua / FrameXML runtime errors (${luaErrors.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: [
        'crates/benilla-ui/src/script/',
        'crates/benilla-app/src/ui_script/',
      ],
      evidence: formatMap(luaErrors).slice(0, 15),
      prompt: `Fix the logged Lua/FrameXML runtime errors in crates/benilla-ui or crates/benilla-app: ${formatMap(luaErrors).slice(0, 3).join(' | ')}`,
    });
  }

  if (dbcFailures.size > 0) {
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'DBC / Formats',
      title: `Fix DBC schema / record layout mismatches (${dbcFailures.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: [
        'crates/benilla-formats/src/dbc.rs',
        'crates/benilla-app/src/',
      ],
      evidence: formatMap(dbcFailures).slice(0, 15),
      prompt: `Repair the 3.3.5a (12340) DBC record reader / field count mismatches: ${formatMap(dbcFailures).slice(0, 3).join(' | ')}`,
    });
  }

  if (assetFailures.size > 0) {
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'Assets / M2 / BLP',
      title: `Fix M2 / Skin / BLP / WMO asset load failures (${assetFailures.size} unique)`,
      label: 'ready-for-agent',
      targetFiles: ['crates/benilla-formats/src/', 'crates/benilla-assets/src/'],
      evidence: formatMap(assetFailures).slice(0, 15),
      prompt: `Investigate and fix the M2/Skin/BLP/WMO asset load failures: ${formatMap(assetFailures).slice(0, 3).join(' | ')}`,
    });
  }

  if (flightIncidents.length > 0) {
    tickets.push({
      id: `AUTO-0${idx++}`,
      category: 'Flight Recorder / Incidents',
      title: `Resolve ${flightIncidents.length} in-game flight-recorder incident(s)`,
      label: 'ready-for-agent',
      targetFiles: [
        'crates/benilla-app/src/quest_trace.rs',
        'crates/benilla-app/src/wrath.rs',
      ],
      evidence: flightIncidents.slice(0, 15),
      prompt: `Inspect and resolve the flight-recorder incidents: ${flightIncidents.slice(0, 3).join(' | ')}`,
    });
  }

  const diagMeta: Record<string, { category: string; title: string; files: string[]; prompt: string }> = {
    ENTITY_MODEL: {
      category: '3D Models & Player Visuals',
      title: 'Fix 3D character/entity model attachment failure or settle timeout',
      files: ['crates/benilla-app/src/entities/attach/', 'crates/benilla-app/src/entities/equipment/'],
      prompt: 'Repair the character/entity 3D model attachment failure or equipment settlement timeout recorded by diag[ENTITY_MODEL].',
    },
    PORTRAIT: {
      category: 'Portraits & Stand-ins',
      title: 'Fix unit portrait fallback or missing race mapping',
      files: ['crates/benilla-app/src/portrait/'],
      prompt: 'Repair the unit portrait 2D/3D stand-in fallback or unmapped race texture recorded by diag[PORTRAIT].',
    },
    UI_UNITFRAME: {
      category: 'Unit Frames',
      title: 'Reconcile or hide unheld unit frames (TargetFrame, FocusFrame, Boss1..4)',
      files: ['crates/benilla-app/src/ui_unit.rs', 'crates/benilla-app/src/ui_script/lifecycle.rs'],
      prompt: 'Hide or properly bind unit frames that are shown without an existing unit as recorded by diag[UI_UNITFRAME].',
    },
    ADDON: {
      category: 'AddOns & Third-Party UI',
      title: 'Resolve third-party AddOn loading, version, or dependency errors',
      files: ['crates/benilla-app/src/ui_script/addons.rs', 'crates/benilla-ui/src/script/addon.rs'],
      prompt: 'Fix the AddOn load refusal, dependency missing, or version mismatch recorded by diag[ADDON].',
    },
    UI_WINDOW: {
      category: 'UI Windows & Frames',
      title: 'Fix disabled or broken UI windows (Talents, SpellBook, CharacterFrame)',
      files: ['crates/benilla-ui/src/script/talent.rs', 'crates/benilla-app/src/ui_script/'],
      prompt: 'Repair the in-game UI window failure or disabled state reported by diag[UI_WINDOW].',
    },
    HOTKEY: {
      category: 'Input & Keybinds',
      title: 'Fix unresponsive hotkey bindings or missing action bar handlers',
      files: ['crates/benilla-app/src/bindings/', 'crates/benilla-ui/src/script/keybind/'],
      prompt: 'Fix the unhandled or unresponsive hotkey / action bar keybind reported by diag[HOTKEY].',
    },
    TEXTURE: {
      category: 'Textures & BLP',
      title: 'Fix missing or corrupted BLP textures',
      files: ['crates/benilla-assets/src/', 'crates/benilla-formats/src/blp.rs'],
      prompt: 'Resolve the missing or failed BLP texture decode reported by diag[TEXTURE].',
    },
    MODEL_MESH: {
      category: '3D Mesh & Geometry',
      title: 'Fix broken M2/WMO mesh, skeleton, or animations',
      files: ['crates/benilla-formats/src/m2/', 'crates/benilla-world/src/model_render.rs'],
      prompt: 'Fix the M2/WMO geometry, skinning, or animation corruption recorded by diag[MODEL_MESH].',
    },
    LOGIN_ANIM: {
      category: 'Login & Character Screen',
      title: 'Fix title/login screen animations and model rigs',
      files: ['crates/benilla-app/src/glue_session/', 'crates/benilla-app/src/portrait/'],
      prompt: 'Fix the login screen or character select animation/model issue reported by diag[LOGIN_ANIM].',
    },
    ITEM_TOOLTIP: {
      category: 'Items & Tooltips',
      title: 'Fix item tooltip description, stats, or template query failures',
      files: ['crates/benilla-app/src/ui_tooltip/', 'crates/benilla-app/src/items/'],
      prompt: 'Fix the item description, tooltip, or template query failure reported by diag[ITEM_TOOLTIP].',
    },
    AUDIO_SFX: {
      category: 'Audio & Sound Effects',
      title: 'Fix missing or unplayed sound effects',
      files: ['crates/benilla-app/src/sound/'],
      prompt: 'Fix the missing or failing sound effect playback reported by diag[AUDIO_SFX].',
    },
    AUDIO_MUSIC: {
      category: 'Audio & Music',
      title: 'Fix missing background music track or zero music volume',
      files: ['crates/benilla-app/src/sound/'],
      prompt: 'Fix the missing background music track or audio volume issue reported by diag[AUDIO_MUSIC].',
    },
  };

  for (const [tag, dict] of diagProbes.entries()) {
    if (dict.size > 0) {
      const meta = diagMeta[tag] || {
        category: `Diagnostic / ${tag}`,
        title: `Resolve in-game defects tagged with diag[${tag}]`,
        files: ['crates/benilla-app/src/'],
        prompt: `Resolve the in-game defects recorded by diag[${tag}].`,
      };
      tickets.push({
        id: `AUTO-0${idx++}`,
        category: meta.category,
        title: `${meta.title} (${dict.size} unique)`,
        label: 'ready-for-agent',
        targetFiles: meta.files,
        evidence: formatMap(dict).slice(0, 15),
        prompt: meta.prompt,
      });
    }
  }

  return tickets;
}

export default function App() {
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [rawInput, setRawInput] = useState('');
  const [parsedTickets, setParsedTickets] = useState<PreppedTicket[]>([]);

  const copyText = (id: string, text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const handleAnalyze = () => {
    setParsedTickets(parseRawLogsOrJsonToTickets(rawInput));
  };

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const content = String(ev.target?.result || '');
      setRawInput(content);
      setParsedTickets(parseRawLogsOrJsonToTickets(content));
    };
    reader.readAsText(file);
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
              Auto-Catcher &amp; Triage Pipeline Active
            </span>
          </div>
          <h1 className="text-2xl md:text-3xl font-bold tracking-tight text-stone-100 mt-2">
            RustWrath — Automated Issue Catcher &amp; Resolution Prep Console
          </h1>
          <p className="text-stone-400 text-sm mt-1">
            Zero manual bug writing: play the game, close the window, and let the resident flight recorder + log triager prep every issue into ready-to-fix tickets.
          </p>
        </div>
        <div className="flex flex-col items-end gap-1">
          <span className="px-3 py-1.5 rounded bg-emerald-950/90 border border-emerald-700/60 text-emerald-300 font-mono text-xs">
            AUTO-EXPORT ON INCIDENT + EXIT
          </span>
          <span className="text-xs text-stone-400 font-mono">
            Logs/Launcher/latest-tickets.md
          </span>
        </div>
      </header>

      {/* Automated Workstation Workflow */}
      <section className="bg-sky-950/40 border border-sky-600/40 rounded-lg p-5 space-y-3">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold text-sky-300">
              1. Zero-Touch Workstation Issue Catcher
            </h2>
            <p className="text-stone-300 text-xs mt-0.5">
              Every play session now automatically exports <code className="text-emerald-300">InteractionDiagnostics</code> snapshots on incidents &amp; exit, and auto-generates <code className="text-amber-300">latest-tickets.md</code> when the game closes:
            </p>
          </div>
          <button
            onClick={() => copyText('workflow', AUTO_CATCHER_WORKFLOW)}
            className="py-1.5 px-3.5 rounded bg-sky-500 hover:bg-sky-400 text-stone-950 font-semibold text-xs transition cursor-pointer self-start sm:self-auto"
          >
            {copiedId === 'workflow' ? '✓ Copied Commands!' : 'Copy Workstation Commands'}
          </button>
        </div>
        <pre className="bg-stone-950/90 p-3 rounded border border-stone-800 text-sky-200 font-mono text-xs overflow-x-auto whitespace-pre-wrap leading-relaxed">
          {AUTO_CATCHER_WORKFLOW}
        </pre>
      </section>

      {/* Interactive Log / Snapshot Triage Dropzone */}
      <section className="bg-stone-900/90 rounded-lg p-6 border border-amber-500/40 space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-amber-300">
              2. Instant Log &amp; Flight-Recorder Ticket Generator
            </h2>
            <p className="text-stone-400 text-xs mt-0.5">
              Upload or paste <code className="text-stone-200">stderr.log</code>, <code className="text-stone-200">interaction-*.jsonl</code>, or <code className="text-stone-200">latest-diagnostics.json</code> to immediately extract deduplicated repair tickets and agent prompts.
            </p>
          </div>
          <label className="py-2 px-4 rounded bg-stone-800 hover:bg-stone-700 border border-stone-700 text-stone-200 font-semibold text-xs transition cursor-pointer self-start sm:self-auto">
            Upload Log / JSONL File
            <input
              type="file"
              accept=".log,.txt,.json,.jsonl,.md"
              onChange={handleFileUpload}
              className="hidden"
            />
          </label>
        </div>

        <textarea
          value={rawInput}
          onChange={(e) => setRawInput(e.target.value)}
          placeholder="Paste lines from stderr.log, stdout.log, interaction-*.jsonl, or latest-diagnostics.json here..."
          rows={5}
          className="w-full bg-stone-950 border border-stone-800 rounded p-3 text-xs font-mono text-stone-200 focus:outline-none focus:border-amber-500"
        />

        <div className="flex items-center gap-3">
          <button
            onClick={handleAnalyze}
            className="py-2 px-4 rounded bg-amber-500 hover:bg-amber-400 text-stone-950 font-semibold text-xs transition cursor-pointer"
          >
            Extract &amp; Prep Resolution Tickets
          </button>
          {parsedTickets.length > 0 && (
            <button
              onClick={() =>
                copyText(
                  'all-prompts',
                  parsedTickets
                    .map(
                      (t) =>
                        `[${t.id}] ${t.title}\nTarget: ${t.targetFiles.join(', ')}\nPrompt: ${t.prompt}\nEvidence:\n${t.evidence.map((e) => '  - ' + e).join('\n')}`
                    )
                    .join('\n\n')
                )
              }
              className="py-2 px-4 rounded bg-emerald-500 hover:bg-emerald-400 text-stone-950 font-semibold text-xs transition cursor-pointer"
            >
              {copiedId === 'all-prompts'
                ? '✓ Copied All Tickets!'
                : `Copy All ${parsedTickets.length} Prepped Tickets`}
            </button>
          )}
        </div>

        {parsedTickets.length > 0 && (
          <div className="space-y-3 pt-2">
            {parsedTickets.map((ticket) => (
              <div
                key={ticket.id}
                className="bg-stone-950 border border-stone-800 rounded p-4 space-y-2"
              >
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <span className="px-2 py-0.5 rounded bg-amber-500/20 text-amber-300 font-mono text-xs font-semibold">
                      {ticket.id}
                    </span>
                    <span className="px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-300 font-mono text-xs">
                      {ticket.category}
                    </span>
                    <h3 className="font-semibold text-sm text-stone-100">
                      {ticket.title}
                    </h3>
                  </div>
                  <button
                    onClick={() => copyText(ticket.id, ticket.prompt)}
                    className="py-1 px-3 rounded bg-stone-800 hover:bg-stone-700 text-amber-300 font-mono text-xs transition cursor-pointer self-start sm:self-auto"
                  >
                    {copiedId === ticket.id
                      ? '✓ Copied Prompt!'
                      : 'Copy Agent Prompt'}
                  </button>
                </div>
                <div className="text-xs text-stone-400 font-mono">
                  Target Files: {ticket.targetFiles.join(', ')}
                </div>
                <div className="text-xs text-stone-300 bg-stone-900/80 p-2.5 rounded border border-stone-800/80">
                  <span className="text-amber-300 font-semibold">Prompt: </span>
                  {ticket.prompt}
                </div>
                <ul className="text-xs font-mono text-stone-400 space-y-1 pl-4 list-disc">
                  {ticket.evidence.map((ev, i) => (
                    <li key={i}>{ev}</li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="bg-stone-900/90 rounded-lg p-6 border border-emerald-500/30 space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-emerald-300">
              3. Targeted Verification Suite (13 Tests &lt; 20 Ceiling)
            </h2>
            <p className="text-stone-400 text-xs mt-0.5">
              Validates G03 item stats bounds, H00 hotpatch retry state, and D01–D07 / Q00–Q04 flight-recorder correlations.
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
    </main>
  );
}
