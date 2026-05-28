# ADR 0001: Keep the Dictation Wrapper Separate from `whisper.cpp`

## Status

Accepted.

## Context

`whisper.cpp` is a focused speech-to-text inference engine. `local-dictate` needs application behavior around that engine: recording, push-to-talk, voice activity detection policy, text insertion, local artifact handling, runtime privacy boundaries, and eventually a tray/settings UI.

Combining those responsibilities in a fork would make upstream updates harder and blur the security review surface.

## Decision

Build `local-dictate` as a separate wrapper. Integrate `whisper.cpp` through a narrow engine boundary.

The first implementation invokes a local `whisper-cli` executable as a child process with explicit arguments and no shell. A later implementation may link against the `whisper.cpp` C API for lower latency while preserving the same app-level contract.

The product default is the bundled `whisper.cpp` CLI engine with a bundled or
preinstalled `ggml-base.en.bin` model. Users can still provide custom
`whisper-cli` and `ggml` model paths for development, GPU builds, or model
experiments.

## Consequences

- The wrapper can run fully offline after setup.
- `whisper.cpp` can be pinned, replaced, or upgraded without changing dictation workflows.
- The first milestone can validate privacy and UX before we commit to a deeper native integration.
- Release packages must include third-party notices for `whisper.cpp` and OpenAI Whisper model weights.
- Process startup overhead is acceptable for push-to-talk MVPs but may be too slow for streaming dictation; that is a known migration point.
