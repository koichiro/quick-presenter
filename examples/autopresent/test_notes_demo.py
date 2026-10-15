import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from autopresent import Presenter, command_adapter
from test_autopresent import Deck


ROOT = Path(__file__).resolve().parent


class NotesDemo(unittest.TestCase):
    def generator(self):
        return command_adapter([sys.executable, str(ROOT / 'notes_narration.py')], 5, True)

    def test_notes_are_read_verbatim_through_the_subprocess_adapter(self):
        notes = '  日本語のノート\nSecond paragraph.\n'
        self.assertEqual(self.generator()({'current': {'notes': notes, 'text': ['Do not read']}}), notes)

    def test_missing_notes_stop_before_playback_or_navigation(self):
        deck = Deck()
        original = deck.call

        def without_notes(*args):
            value = original(*args)
            if args[0] == 'context':
                value['current']['notes'] = ' \n'
            return value

        deck.call = without_notes
        spoken = []
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            Presenter(deck, self.generator(), spoken.append).run('demo.pdf')
        self.assertIn('no speaker notes', raised.exception.stderr)
        self.assertEqual(spoken, [])
        self.assertNotIn('next', deck.log)

    def test_notes_generator_integrates_with_all_pages_and_final_noop(self):
        deck = Deck()
        spoken = []
        Presenter(deck, self.generator(), spoken.append).run('demo.pdf')
        self.assertEqual(spoken, ['Note 1', 'Note 2'])
        self.assertEqual(deck.log.count('next'), 2)

    @unittest.skipUnless(sys.platform == 'darwin', 'macOS player')
    def test_player_passes_option_like_narration_via_stdin(self):
        with tempfile.TemporaryDirectory() as directory:
            say = Path(directory) / 'say'
            say.write_text(f'#!{sys.executable}\nimport json, sys\n'
                           'print(json.dumps({"argv":sys.argv[1:],"text":sys.stdin.read()}))\n')
            say.chmod(0o755)
            result = subprocess.run(
                [sys.executable, str(ROOT / 'speak_macos.py'), '--voice', 'Test Voice', '--rate', '170'],
                input='-v This is narration, not an option.\n日本語', text=True,
                capture_output=True, check=True, env={**os.environ, 'PATH': directory})
            self.assertEqual(json.loads(result.stdout), {
                'argv': ['-v', 'Test Voice', '-r', '170', '-f', '-'],
                'text': '-v This is narration, not an option.\n日本語'})


if __name__ == '__main__':
    unittest.main()
