# local-dictate

Local, no-network voice dictation that saves nothing.

The project is intentionally separate from `whisper.cpp`. `whisper.cpp` is the inference engine; this repo owns the product layer around it: microphone capture, push-to-talk, local-only configuration, transcript handling, and text insertion.

One of the core guiding principles is to store as little as possible, for as
little time as possible, and never send anything from the running app over the
network. See `docs/privacy-principles.md` for the project privacy boundary.

## Current Shape

- `crates/local-dictate-core`: engine contracts and the first `whisper-cli` adapter.
- `crates/local-dictate-cli`: a small smoke-test CLI for transcribing an existing audio file.
- `crates/local-dictate-ui`: a native desktop settings app for Windows, macOS, and Linux.
- `engines/`: bundled or locally installed `whisper.cpp` engine binaries such as `whisper-cli.exe`; ignored by Git.
- `models/`: bundled or locally installed `ggml` model files; ignored by Git.
- `recordings/` and `transcripts/`: local private artifacts; ignored by Git.

## Why Not Fork `whisper.cpp`?

Forking makes every app feature compete with upstream ASR engine maintenance. A separate wrapper gives us a cleaner privacy boundary:

- We can pin or swap the engine without rewriting dictation features.
- We can review the app-level privacy boundary separately from upstream engine maintenance.
- We can start with the `whisper-cli` executable, then move to the C API later if latency demands it.
- Upstream security and performance fixes remain easy to pull.

## First Smoke Test

After placing a local `whisper-cli` binary and a local `ggml` model file on disk:

```powershell
cargo run -p local-dictate-cli -- `
  --engine .\engines\whisper-cli.exe `
  --model .\models\ggml-base.en.bin `
  --audio .\recordings\sample.wav `
  --language en `
  --capture-hotkey Ctrl+Win `
  --cleanup-disfluencies `
  --cleanup-revisions `
  --swap peers=PRs
```

`--swap <from=to>` can be repeated for post-processing keyword swaps. Swaps match whole keywords
case-insensitively, so `--swap peers=PRs` changes `peers` or `Peers` to `PRs` without changing
words like `appears`.

`--cleanup-disfluencies` enables local post-processing for common dictation cleanup, including
filler words, immediate repeated words or phrases, and simple hyphenated stutter starts.
`--cleanup-revisions` separately removes short abandoned revision fragments when they strongly
overlap with a fuller final clause.

`--capture-hotkey` validates the push-to-talk hotkey setting that the live voice capture surface will
use. It defaults to `Ctrl+Win` on Windows, `Ctrl+Cmd` on macOS, and `Ctrl+Super` on Linux and
other Unix-like systems.

The Rust adapter does not make network requests at runtime; it invokes a local
engine binary with local model and audio paths. See `docs/runtime-network.md`
for the current network boundary and the checked `whisper.cpp` engine runtime.

## Bundled Defaults

The settings app defaults to the bundled `whisper.cpp` engine and the `base.en`
Whisper model preset. Custom engine and model paths still work as advanced
overrides, so developers can test GPU builds, newer upstream builds, or custom
`ggml` models without changing the app contract.

For Windows development and release prep, install the default local assets with:

```powershell
.\scripts\install-whisper-assets.ps1
```

If local PowerShell script execution is disabled, run it as:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install-whisper-assets.ps1
```

That script downloads the pinned `whisper.cpp` Windows binary release, installs
`whisper-cli.exe` and its DLL runtime files into `engines/`, and installs the
`ggml-base.en.bin` model into `models/`. The model download is validated against
the upstream SHA-1 listed by `whisper.cpp`.

For local macOS and Linux release prep, build the pinned `whisper.cpp` engine
from source and install the default model with:

```powershell
pwsh ./scripts/build-whisper-cpp.ps1 -Platform macos-universal
pwsh ./scripts/build-whisper-cpp.ps1 -Platform linux-x64
pwsh ./scripts/install-whisper-model.ps1 -Model base.en
```

The engine build script installs the compatible `whisper-cli` binary to
`engines/`. The model script installs `models/ggml-base.en.bin` and validates it
against the upstream SHA-1 listed by `whisper.cpp`.

Fully bundled app builds should include:

- `engines/whisper-cli.exe` on Windows, or `engines/whisper-cli` on macOS/Linux.
- `models/ggml-base.en.bin` for the default English model.
- `THIRD_PARTY_NOTICES.md` with the app distribution.

The GitHub release workflow publishes app packages for Windows, macOS, and
Linux. Those packages include the Local Dictate UI and CLI binaries, a bundled
`whisper.cpp` `whisper-cli` engine for the target operating system, the default
`ggml-base.en.bin` model, README, and third-party notices. The default
configuration is runnable after extracting the package; custom model paths are
only needed for non-default models.

## Settings App

Run the native settings UI:

```sh
cargo run -p local-dictate-ui
```

On this Windows development setup, the helper script also works:

```powershell
.\scripts\cargo-dev.cmd run -p local-dictate-ui
```

The app automatically saves `settings.toml` in the operating system's config directory. It currently configures:

- Voice capture hotkey.
- Whisper engine, model preset, optional advanced paths, and language.
- Automatic insertion and optional trailing spaces after inserted dictation.
- Optional local cleanup for stutters, fillers, and immediate repeats.
- Optional local cleanup for short abandoned revisions.
- Post-processing keyword swaps.

When the hotkey is held, the app captures microphone audio and shows a small always-on-top overlay.
When the hotkey is released, it transcribes the capture, applies local post-processing, and inserts
the processed text into the focused text field. Captured audio is written only to short-lived temporary
storage for `whisper-cli` processing and is deleted immediately after transcription completes. The
app does not save transcript files or display dictated text in the UI.

On Windows, the UI app runs without a console window. Closing or minimizing the
settings window keeps dictation running from the system tray; use the tray menu
to show settings again or exit completely. On macOS, ship the UI binary in an
`.app` bundle so it opens as a normal app with a menu bar status item instead of
a Terminal session. On Linux, launch from a `.desktop` entry with
`Terminal=false`; tray support uses the standard GTK/AppIndicator stack and may
require the matching system packages for the desktop environment. Starter
packaging templates live in `packaging/`.

Build a standalone binary for the current OS:

```sh
cargo build -p local-dictate-ui --release
```

Or with the Windows helper:

```powershell
.\scripts\cargo-dev.cmd build -p local-dictate-ui --release
```

The binary will be in `target\release\local-dictate-ui.exe` on Windows, and
`target/release/local-dictate-ui` on macOS or Linux. Build from each target OS to produce that
platform's native app binary.

## GitHub Releases

Pushing a version tag creates a GitHub release with one package per supported
operating system:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds and tests the workspace, builds or installs the
pinned `whisper.cpp` CLI engine, then uploads:

- `local-dictate-<version>-windows-x64.zip`
- `local-dictate-<version>-macos-universal.zip`
- `local-dictate-<version>-linux-x64.tar.gz`

You can also run the `Release` workflow manually from GitHub Actions and provide
the release tag, such as `v0.1.0`. Manual runs default to draft releases.

On Windows, if Cargo cannot find the MSVC linker environment, run through the helper:

```powershell
.\scripts\cargo-dev.cmd run -p local-dictate-cli -- --help
```

## Third-Party Credits

`local-dictate` uses `whisper.cpp` as the local inference engine and OpenAI
Whisper model weights converted to `ggml` format. See `THIRD_PARTY_NOTICES.md`
for license notices and attribution.
