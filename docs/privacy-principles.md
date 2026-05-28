# Privacy Principles

`local-dictate` is built around a hard privacy constraint:

Store as little as possible, for as little time as possible, and never send
anything from the running app over the network.

That means:

- Microphone audio is captured only while the push-to-talk hotkey is held.
- Captured audio is written only to short-lived temporary storage so the local
  transcription engine can read it.
- Temporary audio is deleted as soon as transcription processing completes.
- Transcribed text stays in memory long enough to apply local keyword swaps and,
  when enabled, insert it into the focused field.
- The app does not save dictated text, transcripts, or captured audio.
- The app does not include cloud fallback, telemetry, analytics, crash uploads,
  sync, or remote transcription.
- The app does not make runtime network requests.
- Runtime code must work with outbound network access blocked.

The local settings file may store user configuration such as hotkeys, selected
engine/model identifiers, language, automatic insertion, and keyword swaps. It
must not store dictated audio or transcript content.

The asset installation script is separate from runtime behavior. It may download
public engine/model assets when a developer or packager explicitly runs it, but
it must not send or upload microphone audio, transcripts, settings, or other user
data.

## Verification Notes

Before changing capture, transcription, text insertion, settings, packaging, or
asset installation behavior, verify the privacy boundary:

- Search runtime Rust code for network APIs or client libraries.
- Search runtime Rust code for persistent writes beyond settings.
- Confirm audio capture still uses temporary storage whose lifetime is bound to
  the capture/transcription flow.
- Confirm transcription output is not written to disk or displayed in the app.
- Confirm the app still works without runtime model downloads.
