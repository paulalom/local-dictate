# local-dictate

Local-first dictation wrapper for `whisper.cpp`.

The project is intentionally separate from `whisper.cpp`. `whisper.cpp` is the inference engine; this repo owns the product layer around it: microphone capture, push-to-talk, local-only configuration, transcript handling, and text insertion.

## Current Shape

- `crates/local-dictate-core`: engine contracts and the first `whisper-cli` adapter.
- `crates/local-dictate-cli`: a small smoke-test CLI for transcribing an existing audio file.
- `crates/local-dictate-ui`: a native desktop settings app for Windows, macOS, and Linux.
- `engines/`: local engine binaries such as `whisper-cli.exe`; ignored by Git.
- `models/`: local `ggml` model files; ignored by Git.
- `recordings/` and `transcripts/`: local private artifacts; ignored by Git.

## Why Not Fork `whisper.cpp`?

Forking makes every app feature compete with upstream ASR engine maintenance. A separate wrapper gives us a cleaner privacy boundary:

- We can pin or swap the engine without rewriting dictation features.
- We can network-sandbox the wrapper and engine independently.
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
  --capture-hotkey Ctrl+Shift+D `
  --swap peers=PRs
```

`--swap <from=to>` can be repeated for post-processing keyword swaps. Swaps match whole keywords
case-insensitively, so `--swap peers=PRs` changes `peers` or `Peers` to `PRs` without changing
words like `appears`.

`--capture-hotkey` validates the push-to-talk hotkey setting that the live voice capture surface will
use. It defaults to `Ctrl+Alt+Space`.

No network access is required at runtime by this adapter.

## Settings App

Run the native settings UI:

```sh
cargo run -p local-dictate-ui
```

On this Windows development setup, the helper script also works:

```powershell
.\scripts\cargo-dev.cmd run -p local-dictate-ui
```

The app saves `settings.toml` in the operating system's config directory. It currently configures:

- Voice capture hotkey.
- Post-processing keyword swaps.

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

On Windows, if Cargo cannot find the MSVC linker environment, run through the helper:

```powershell
.\scripts\cargo-dev.cmd run -p local-dictate-cli -- --help
```
