#!/usr/bin/env python3
"""Render a complete narration with local Kokoro, then play it with macOS afplay."""
import argparse
import contextlib
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import sys
import tempfile
import wave


MODEL_ID = 'hexgrad/Kokoro-82M'
MODEL_REVISION = 'f3ff3571791e39611d31c381e3a41a3af07b4987'
SAMPLE_RATE = 24000
DEFAULT_CACHE = Path(__file__).resolve().parent / '.kokoro'


def audio_path(cache, text, voice, speed):
    signature = json.dumps([1, MODEL_REVISION, text, voice, speed], ensure_ascii=False)
    return cache / 'audio' / (hashlib.sha256(signature.encode('utf-8')).hexdigest() + '.wav')


def load_pipeline(cache, voice, download=False):
    # Keep third-party progress and model diagnostics off the adapter's stdout.
    with contextlib.redirect_stdout(sys.stderr):
        import spacy
        import torch
        from huggingface_hub import hf_hub_download
        from kokoro import KModel, KPipeline

        if sys.platform == 'darwin':
            # Misaki selects its bundled eSpeak library on import. Use the
            # Homebrew library/data together to avoid its wheel's build-time
            # data path, which can make native initialization exit the process.
            from phonemizer.backend.espeak.wrapper import EspeakWrapper
            executable = shutil.which('espeak-ng')
            if executable is None:
                raise RuntimeError('eSpeak NG is missing; run brew install espeak-ng')
            prefix = Path(executable).resolve().parent.parent
            library = prefix / 'lib' / 'libespeak-ng.dylib'
            data = prefix / 'share'
            if not library.is_file() or not (data / 'espeak-ng-data').is_dir():
                raise RuntimeError('Cannot locate the Homebrew eSpeak NG library and data')
            EspeakWrapper.set_library(str(library))
            EspeakWrapper.set_data_path(str(data))

        if importlib.util.find_spec('en_core_web_sm') is None:
            if not download:
                raise RuntimeError('English language data is missing; run this player with --prepare first')
            spacy.cli.download('en_core_web_sm')

        def fetch(filename):
            return hf_hub_download(MODEL_ID, filename, revision=MODEL_REVISION,
                                   cache_dir=str(cache / 'model'), local_files_only=not download)

        config = fetch('config.json')
        weights = fetch('kokoro-v1_0.pth')
        voice_file = fetch(f'voices/{voice}.pt')
        model = KModel(repo_id=MODEL_ID, config=config, model=weights).to('cpu').eval()
        pipeline = KPipeline(lang_code=voice[0], repo_id=MODEL_ID, model=model)
        return pipeline, torch.load(voice_file, map_location='cpu', weights_only=True)


def write_audio(chunks, destination):
    """Publish one WAV only after every generated chunk has been written."""
    import numpy as np
    import soundfile as sf

    destination.parent.mkdir(parents=True, exist_ok=True)
    frames = 0
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=destination.parent, suffix='.wav', delete=False) as file:
            temporary = Path(file.name)
        with sf.SoundFile(temporary, mode='w', samplerate=SAMPLE_RATE,
                          channels=1, subtype='PCM_16', format='WAV') as file:
            for _, _, audio in chunks:
                if audio is None:
                    raise RuntimeError('Kokoro returned a narration chunk without audio')
                if hasattr(audio, 'detach'):
                    audio = audio.detach().cpu().numpy()
                samples = np.asarray(audio)
                if samples.ndim != 1 or samples.size == 0 or not np.isfinite(samples).all():
                    raise RuntimeError('Kokoro returned an empty or invalid narration chunk')
                file.write(samples)
                frames += samples.size
        if not frames:
            raise RuntimeError('Kokoro produced no narration audio')
        os.replace(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def validate_audio(path):
    with wave.open(str(path), 'rb') as file:
        frames = file.getnframes()
        if (file.getframerate(), file.getnchannels(), file.getsampwidth()) != (SAMPLE_RATE, 1, 2):
            raise RuntimeError('Cached narration has an unexpected WAV format')
        if frames == 0 or len(file.readframes(frames)) != frames * 2:
            raise RuntimeError('Cached narration is empty or incomplete')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--voice', default='af_heart', help='Kokoro English voice, e.g. af_heart or bf_emma')
    parser.add_argument('--speed', type=float, default=1.0, help='Positive speech speed multiplier')
    parser.add_argument('--cache-dir', type=Path, default=DEFAULT_CACHE)
    parser.add_argument('--prepare', action='store_true', help='Download model, voice and English data without playback')
    args = parser.parse_args()
    if not math.isfinite(args.speed) or args.speed <= 0:
        parser.error('Speed must be finite and positive')
    if not args.voice.startswith(('af_', 'am_', 'bf_', 'bm_')) or not all(
            c.isascii() and (c.isalnum() or c == '_') for c in args.voice):
        parser.error('Choose a single American or British English Kokoro voice')
    if not args.prepare and sys.platform != 'darwin':
        parser.error('Playback requires macOS afplay')
    try:
        cache = args.cache_dir.resolve()
        if args.prepare:
            load_pipeline(cache, args.voice, download=True)
            print(f'Kokoro model and {args.voice} ready in {cache}', file=sys.stderr)
            return 0
        text = sys.stdin.read()
        if not text.strip():
            raise ValueError('Narration must not be empty')
        output = audio_path(cache, text, args.voice, args.speed)
        if not output.exists():
            pipeline, voice = load_pipeline(cache, args.voice)
            with contextlib.redirect_stdout(sys.stderr):
                write_audio(pipeline(text, voice=voice, speed=args.speed), output)
        validate_audio(output)
        # afplay owns this process until the whole file finishes. Adapter timeouts
        # kill playback directly, with no detached child to outlive the adapter.
        os.execv('/usr/bin/afplay', ['afplay', str(output)])
    except Exception as exc:
        print(f'Kokoro playback failed: {exc}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
