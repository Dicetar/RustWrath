import React from 'react';

export default function App() {
  return (
    <main className="p-8 max-w-4xl mx-auto space-y-6">
      <header className="border-b border-stone-700 pb-4">
        <h1 className="text-3xl font-bold tracking-tight text-amber-400">RustWrath — Project B</h1>
        <p className="text-stone-400 text-sm mt-1">3.3.5a Wrath of the Lich King Client & Tooling</p>
      </header>

      <section className="bg-stone-800 rounded-lg p-6 space-y-3 border border-stone-700">
        <h2 className="text-xl font-semibold text-stone-200">Session Status</h2>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <span className="text-stone-400">Branch: </span>
            <span className="font-mono text-emerald-400">wotlk</span>
          </div>
          <div>
            <span className="text-stone-400">Build: </span>
            <span className="font-mono text-amber-300">12340</span>
          </div>
          <div>
            <span className="text-stone-400">Latest Commit: </span>
            <span className="font-mono text-stone-300">39780a3</span>
          </div>
          <div>
            <span className="text-stone-400">Agreements: </span>
            <span className="text-emerald-400 font-medium">TOS, EULA, Scanning, Contest, TerminationWithoutNotice</span>
          </div>
        </div>
      </section>

      <section className="bg-stone-800 rounded-lg p-6 space-y-3 border border-stone-700">
        <h2 className="text-xl font-semibold text-stone-200">3.3.5a Launch & Login Workflow Status</h2>
        <ul className="list-disc list-inside space-y-2 text-stone-300 text-sm">
          <li>
            <strong className="text-stone-100">showToolsUI:</strong> Registered CVar (default <code className="text-amber-300">-1</code>) preventing launcher dialog crashes on launch.
          </li>
          <li>
            <strong className="text-stone-100">Termination Without Notice:</strong> Registered <code className="text-amber-300">readTerminationWithoutNotice</code> CVar and exposed <code className="text-amber-300">ShowTerminationWithoutNoticeNotice</code>, <code className="text-amber-300">TerminationWithoutNoticeAccepted</code>, and <code className="text-amber-300">AcceptTerminationWithoutNotice</code> to the Glue Lua environment.
          </li>
          <li>
            <strong className="text-stone-100">Wrath Glue API Parity:</strong> Registered <code className="text-amber-300">IsShiftKeyDown</code>, <code className="text-amber-300">GetUsesToken</code>, <code className="text-amber-300">SetUsesToken</code>, <code className="text-amber-300">GetSavedAccountList</code>, <code className="text-amber-300">SetSavedAccountList</code>, expansion levels, and authentication stubs matching 12340 specifications.
          </li>
        </ul>
      </section>
    </main>
  );
}
