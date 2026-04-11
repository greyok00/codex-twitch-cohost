Place bundled local STT assets here when packaging:

- whisper-cli binary (or whisper-cli.exe on Windows)
- ggml-base.en.bin model file (recommended fast default)

Optional per-platform folders are also scanned:
- assets/whisper-linux/
- assets/whisper-macos/
- assets/whisper-win/

If no bundled model is found, Auto Setup STT downloads ggml-base.en.bin into app data.
