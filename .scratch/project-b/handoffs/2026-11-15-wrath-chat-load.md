# Measure FloatingChatFrame.xml during a normal Wrath launch

Status: ready to merge
Owner: Codex
Branch: diag/wrath-chat-load
Issue: none
Next: include in the owner's build batch and inspect the wrath chat load lines in launcher stderr

The November 14 launcher log records a 153-second gap before the chat
document finished. The next manual launch can collect phase timings using
the same install, font engine and Lua VM as the reported run.

The production FrameXML loader logs a start line and a summary for the
Wrath copy of `FloatingChatFrame.xml`. The summary gives the root file read,
decode and parse, frame build, and report times. Dependency reads are
included in the build time and reported separately. Top-level items and
XML OnLoad handlers that take at least 100 ms get a named timing line.
Frame timings include their handlers, so those lines must not be summed.

Source review confirmed that the stock FrameXML manifest resolves this
document to the instrumented path and that the launcher retains the
existing info log alongside the font load lines. No Cargo build, tests or
client launch ran. The launcher's next log is needed to locate the stall.
