import importlib.util
import io
from pathlib import Path
import struct
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, patch
import wave

import speak_kokoro as player


HAS_AUDIO_DEPS = all(importlib.util.find_spec(name) is not None for name in ('numpy', 'soundfile'))


class Kokoro(unittest.TestCase):
    def test_normal_model_loading_requires_local_files_at_the_pinned_revision(self):
        hub = types.ModuleType('huggingface_hub')
        hub.hf_hub_download = Mock(side_effect=lambda repo, filename, **kwargs: filename)
        kokoro = types.ModuleType('kokoro')
        kokoro.KModel, kokoro.KPipeline = Mock(), Mock()
        torch = types.ModuleType('torch')
        torch.load = Mock()
        with patch.dict(sys.modules, {'huggingface_hub': hub, 'kokoro': kokoro,
                                      'spacy': types.ModuleType('spacy'), 'torch': torch}), \
                patch.object(sys, 'platform', 'linux'), \
                patch.object(importlib.util, 'find_spec', return_value=object()):
            player.load_pipeline(Path('cache'), 'af_heart')
        self.assertEqual(hub.hf_hub_download.call_count, 3)
        for call in hub.hf_hub_download.call_args_list:
            self.assertEqual(call.args[0], player.MODEL_ID)
            self.assertTrue(call.kwargs['local_files_only'])
            self.assertEqual(call.kwargs['revision'], player.MODEL_REVISION)

    def test_audio_cache_changes_with_narration_voice_or_speed(self):
        cache = Path('cache')
        original = player.audio_path(cache, 'First note.', 'af_heart', 1.0)
        self.assertEqual(original, player.audio_path(cache, 'First note.', 'af_heart', 1.0))
        for text, voice, speed in [('Last note.', 'af_heart', 1.0),
                                   ('First note.', 'af_bella', 1.0),
                                   ('First note.', 'af_heart', 0.9)]:
            self.assertNotEqual(original, player.audio_path(cache, text, voice, speed))

    @unittest.skipUnless(HAS_AUDIO_DEPS, 'optional Kokoro audio dependencies')
    def test_every_chunk_is_in_the_final_wav_in_order(self):
        import numpy as np
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'whole.wav'
            chunks = [('First sentence.', '', np.full(240, 0.5)),
                      ('Last sentence.', '', np.full(480, -0.5))]
            player.write_audio(iter(chunks), output)
            player.validate_audio(output)
            with wave.open(str(output)) as file:
                self.assertEqual(file.getnframes(), 720)
                samples = struct.unpack('<720h', file.readframes(720))
                self.assertEqual(samples[:240], (16384,) * 240)
                self.assertEqual(samples[240:], (-16384,) * 480)

    @unittest.skipUnless(HAS_AUDIO_DEPS, 'optional Kokoro audio dependencies')
    def test_late_synthesis_failure_never_publishes_partial_audio(self):
        import numpy as np

        def failed_chunks():
            yield 'First sentence.', '', np.ones(240)
            raise RuntimeError('Last sentence failed')

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'whole.wav'
            with self.assertRaisesRegex(RuntimeError, 'Last sentence failed'):
                player.write_audio(failed_chunks(), output)
            self.assertEqual(list(Path(directory).iterdir()), [])

    @unittest.skipUnless(HAS_AUDIO_DEPS, 'optional Kokoro audio dependencies')
    def test_empty_or_missing_audio_stops(self):
        with tempfile.TemporaryDirectory() as directory:
            for chunks in ([], [('Unspoken sentence.', '', None)]):
                with self.assertRaises(RuntimeError):
                    player.write_audio(iter(chunks), Path(directory) / 'whole.wav')
                self.assertEqual(list(Path(directory).iterdir()), [])

    def test_truncated_cached_wav_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'whole.wav'
            with wave.open(str(output), 'wb') as file:
                file.setparams((1, 2, player.SAMPLE_RATE, 0, 'NONE', 'not compressed'))
                file.writeframes(b'\x00\x00' * 240)
            output.write_bytes(output.read_bytes()[:-10])
            with self.assertRaisesRegex(RuntimeError, 'incomplete'):
                player.validate_audio(output)

    def test_cached_playback_executes_afplay_without_loading_models(self):
        class Executed(BaseException):
            pass

        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory).resolve()
            output = player.audio_path(cache, 'Read this note.', 'af_heart', 1.0)
            output.parent.mkdir()
            with wave.open(str(output), 'wb') as file:
                file.setparams((1, 2, player.SAMPLE_RATE, 0, 'NONE', 'not compressed'))
                file.writeframes(b'\x00\x00' * 240)
            with patch.object(sys, 'argv', ['speak_kokoro.py', '--cache-dir', directory]), \
                    patch.object(sys, 'stdin', io.StringIO('Read this note.')), \
                    patch.object(sys, 'platform', 'darwin'), \
                    patch.object(player, 'load_pipeline') as load, \
                    patch.object(player.os, 'execv', side_effect=Executed) as execute:
                with self.assertRaises(Executed):
                    player.main()
                load.assert_not_called()
                execute.assert_called_once_with('/usr/bin/afplay', ['afplay', str(output)])


if __name__ == '__main__':
    unittest.main()
