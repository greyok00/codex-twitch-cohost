#!/usr/bin/env python3
import argparse
import json
import sys
import wave

from vosk import KaldiRecognizer, Model, SetLogLevel


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--audio", required=True)
    args = parser.parse_args()

    SetLogLevel(-1)

    try:
        wf = wave.open(args.audio, "rb")
    except Exception as exc:
      print(f"failed opening wav: {exc}", file=sys.stderr)
      return 2

    if wf.getnchannels() != 1:
        print("wav must be mono", file=sys.stderr)
        return 2
    if wf.getsampwidth() != 2:
        print("wav must be 16-bit PCM", file=sys.stderr)
        return 2

    try:
        model = Model(args.model_dir)
        rec = KaldiRecognizer(model, float(wf.getframerate()))
        rec.SetWords(True)
        rec.SetPartialWords(False)
    except Exception as exc:
        print(f"failed loading vosk model: {exc}", file=sys.stderr)
        return 2

    texts = []
    while True:
        data = wf.readframes(4000)
        if not data:
            break
        if rec.AcceptWaveform(data):
            try:
                result = json.loads(rec.Result())
            except Exception:
                result = {}
            text = str(result.get("text") or "").strip()
            if text:
                texts.append(text)

    try:
        final_result = json.loads(rec.FinalResult())
    except Exception:
        final_result = {}
    final_text = str(final_result.get("text") or "").strip()
    if final_text:
        texts.append(final_text)

    print(" ".join(part for part in texts if part).strip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
