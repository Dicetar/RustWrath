# OpenCode: four CVar defects

Implement these four fixes in D:/Projects/project-b-client. This supersedes your completed read-only contract review. First acknowledge the ownership below through AutoSwarm, then implement. User requested half the discovered bugs assigned to OpenCode; the other half is reserved in 2026-11-01-gemini-bug-fixes.md. Do not delegate.

Read AGENTS.md, docs/METHOD.md, docs/agents/12340-reference.md and active-ownership.md. Current HEAD at assignment: 8807107f; preserve all dirty edits. Manual session ses_e67532ec7fferWEL6rThWdiGOY ended idle after committing cursor work and merging web preview; do not resume its old task. Reference D:/Projects/wowdecomp-reference, pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a: behavioral evidence only, no copied implementation.

## Ownership and verification

Own crates/benilla-app/src/cvars/table.rs, crates/benilla-ui/src/script/cvars.rs, and existing CVar-focused tests only. Request parent approval before editing any additional consumer file. Gemini owns glue_select.rs, ui_script/glue_mgr.rs, tooltip_item/, wrath_natives.rs and their focused tests; do not touch them. Parent owns build/client coordination. No Cargo, client, visual automation, commits, pushes, install/server/DB/credential changes. Implement source and describe exact targeted checks to parent; parent schedules shared verification. Do not claim runtime acceptance from source inspection.

Latest successful build D3A611C044A4D601D07F92649EAFBC3CFE27968C845A3FCBD53146C7AAFCFBC6 already contains strict unknown-CVar behavior. Never restore blanket unknown-name tolerance or arbitrary 0 defaults.

## OC-1 [high, runtime confirmed]: Sound_EnableMusic is missing

Latest log benilla-config/Logs/Launcher/20261101-234026-676/stderr.log lines 32, 34, 175, 259, 400 reports OptionsPanelTemplates.lua:336/335 Couldn't find CVar named 'Sound_EnableMusic', in glue and world. table.rs has other Sound_Enable* rows but not this one.

Reference WoW/Source/SoundInterface/SoundInterface2_CVar.cpp:251 registers Sound_EnableMusic with default "1" and EnableMusicHandler. Add a real Wrath row before both GlueXML and FrameXML initialization using the existing registry. Trace the existing music-enable consumer: if a bounded existing wiring path lies outside ownership, report exact files and proposed delta before editing. A registered value alone must not be described as working mute control. Check adjacent stock sound settings against actual callers to avoid another blind modal/rebuild cycle; only add proven required rows, with cited defaults and honest backend limitations. Acceptance: both VM reads/default reads resolve to 1 on fresh settings; unknown names still error; eventual live music-toggle check must be reported separately.

## OC-2 [high, runtime confirmed]: textureFilteringMode is missing

Same log lines 31,33,173,258,398: OptionsPanelTemplates.lua:216/214 cannot find textureFilteringMode. Reference WoW/Source/Client.cpp:1108 registers default "1" with CWorldParam::TextureFilteringCallback. Add the real Wrath registry row and inspect the callback's semantics. Existing donor rows trilinear/anisotropic (table.rs around705-724) and TexFilterSetting are routing clues, not proof the enum can be copied directly. Keep donor defaults intact. If connecting Wrath's enum requires extra files, request that narrow ownership extension; do not fake successful renderer application. Acceptance: stock options initialize in both VMs, default 1, no blanket tolerance; renderer behavior is separately verified or explicitly outstanding.

## OC-3 [medium, source confirmed]: incorrect stock defaults

Correct the following current -> reference values in the added Wrath rows in table.rs: showKeyring 1->0; questFadingDisable 0->1; playerStatLeftDropdown "1"->""; playerStatRightDropdown "2"->""; xpBarText 1->0; gxStereoConvergence 0->1; gxStereoSeparation 0->25; autojoinPartyVoice 0->1; environmentDetail 3->1.0; groundEffectDensity 1->16; groundEffectDist 300->70.0.

Recheck register sites in Ui/GameUI_CVar.cpp (488,551,604,608,610-611), ConsoleDevice/ConsoleDevice_CVar.cpp (530-531), WorldClient/WorldParam_CVar.cpp (646-650). Preserve saved user overrides; do not rewrite config.toml. Missing alwaysCompareItems default0 (GameUI_CVar.cpp:467) is also needed by equipment-comparison consumers: add it if absent, without claiming OC implements Gemini's tooltip task. Keep any backend limitation documented rather than inventing functionality. Acceptance: fresh default/get-default values match reference; persisted values remain untouched; donor CVar catalog stays compatible.

## OC-4 [medium, source confirmed]: Wrath-only API leaked into donor VM

script/cvars.rs around635 installs GetCVarMin/GetCVarMax unconditionally. Restore their installation inside the existing Wrath335 target branch beside GetCVarBool. Wrath glue already uses glue_with_target(selected_ui_target()) in app lifecycle; glue does not imply vanilla. Remove/update the misleading comment that target gating broke glue. Preserve strict unknown-name errors and known limit behavior; do not invent extShadowQuality maximum support. Acceptance: Wrath glue and world expose getters, vanilla glue does not; existing the_glue_vm_answers_the_glue_surface remains valid. Reuse existing cvar_bindings tests with a small target matrix, no broad suite or new harness.

Return changed files, reference anchors, precise limitations, and proposed <=6 focused checks. Do not fix quest clicks, add unrelated Lua stubs, or touch Gemini's files.
