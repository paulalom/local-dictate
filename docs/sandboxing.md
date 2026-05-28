# Sandboxing Notes

`local-dictate` should work with outbound network access blocked.

The privacy baseline is documented in `docs/privacy-principles.md`: store as
little as possible, for as little time as possible, and never send anything from
the running app over the network.

Runtime requirements for the first adapter:

- Read access to the bundled or selected `whisper-cli` binary.
- Read access to the bundled or selected `ggml` model file.
- Read access to the audio file being transcribed.
- No shell execution; process arguments are passed directly through `ProcessStartInfo.ArgumentList`.
- No runtime model downloads. Model and engine installation happens before runtime.

Future live dictation requirements:

- Microphone access for the recorder.
- Clipboard or synthetic keyboard access for text insertion.
- Optional GPU driver access if we run a GPU-enabled engine build.

Recommended privacy defaults:

- Keep bundled or downloaded engines in `engines/`.
- Keep bundled or downloaded models in `models/`.
- Keep recordings and transcripts out of Git.
- Disable cloud fallback, sync, telemetry, crash uploads, and auto-update checks unless the user explicitly opts in.
- Add outbound firewall deny rules for both the wrapper executable and `whisper-cli` when testing sensitive workflows.
