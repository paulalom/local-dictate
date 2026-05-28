# Runtime Network Notes

`local-dictate` is designed to run after setup without runtime network access.

The app does not currently apply an OS-level network block. The wrapper launches
the selected `whisper-cli` executable as a normal child process, so a custom or
compromised engine binary would still have the normal access granted to that
process by the operating system.

Runtime requirements for the first adapter:

- Read access to the bundled or selected `whisper-cli` binary.
- Read access to the bundled or selected `ggml` model file.
- Read access to the audio file being transcribed.
- No shell execution; process arguments are passed directly through Rust
  `Command` arguments.
- No runtime engine or model downloads. Engine and model installation happens
  before runtime.

The repository-owned setup-time downloader is
`scripts/install-whisper-assets.ps1`. It fetches public `whisper.cpp` engine
assets and selected `ggml` models when explicitly run by a developer or
packager. On Windows, it installs `whisper-cli.exe` and DLL runtime files from
the upstream archive; it does not install the other helper executables.

The current pinned Windows `whisper.cpp` CLI runtime is `v1.8.4`. The checked
runtime files used by the app, `whisper-cli.exe`, `whisper.dll`, `ggml.dll`,
`ggml-base.dll`, and `ggml-cpu.dll`, do not import Windows networking libraries.
For the current repo-owned setup and runtime path, network access is limited to
the explicit setup-time asset downloads.

Recommended privacy defaults:

- Keep bundled or downloaded engines in `engines/`.
- Keep bundled or downloaded models in `models/`.
- Keep recordings and transcripts out of Git.
- Disable cloud fallback, sync, telemetry, crash uploads, and auto-update checks
  unless the user explicitly opts in.
- If a release needs enforced network isolation, configure that outside the app
  and document it as a platform-specific release step.
