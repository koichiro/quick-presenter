import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from autopresent import Presenter, Qp, QpError, Stop, command_adapter, dry_generate


class Deck:
    def __init__(self):
        self.state = dict(session_id='test', document_revision=1,
                          document=str(Path('demo.pdf').resolve()), page=1, pages=2,
                          opening=False, blackout=False, render_state='ready', notes_state='ready')
        self.log = []
        self.loading = 0
        self.failure = None

    def call(self, *args):
        command = args[0]
        self.log.append(command)
        if command == 'open':
            return {'state': copy.deepcopy(self.state)}
        if command == 'context':
            if self.loading:
                self.loading -= 1
                raise QpError('NOTES_LOADING', 'pending')
            page = self.state['page']
            return {'presentation': copy.deepcopy(self.state),
                    'current': dict(page=page, notes=f'Note {page}', text=['Text'], truncated=False),
                    'next': None if page == 2 else dict(page=2, notes='Note 2', text=['Text'], truncated=False)}
        if command == 'status':
            return copy.deepcopy(self.state)
        if command == 'next':
            changed = self.state['page'] < 2
            self.state['page'] = min(2, self.state['page'] + 1)
            if self.failure:
                raise QpError(self.failure, 'response lost after commit')
            return {'changed': changed, 'state': copy.deepcopy(self.state)}
        raise AssertionError(args)


class Integration(unittest.TestCase):
    def run_deck(self, deck, generate=dry_generate, play=None):
        Presenter(deck, generate, play or (lambda text: deck.log.append('audio_done')), wait=.05, interval=0).run('demo.pdf')

    def test_narration_finishes_before_next_and_final_noop(self):
        deck = Deck()
        spoken = []
        def play(text):
            spoken.append(text)
            deck.log.append('audio_done')
        self.run_deck(deck, play=play)
        self.assertEqual(spoken, ['Note 1', 'Note 2'])
        self.assertEqual(deck.log.count('next'), 2)
        for index, command in enumerate(deck.log):
            if command == 'next':
                self.assertEqual(deck.log[index - 2:index], ['audio_done', 'status'])

    def test_notes_loading_and_read_disconnect_are_retried(self):
        deck = Deck()
        deck.loading = 2
        original = deck.call
        disconnected = [True]
        def call(*args):
            if args[0] == 'context' and disconnected[0]:
                disconnected[0] = False
                raise QpError('IPC_FAILURE', 'disconnected')
            return original(*args)
        deck.call = call
        self.run_deck(deck)
        self.assertEqual(deck.log.count('next'), 2)
        self.assertEqual(deck.log.count('context'), 4)

    def test_uncertain_next_is_never_replayed(self):
        for failure in ('TIMEOUT', 'IPC_FAILURE', 'NOT_RUNNING'):
            with self.subTest(failure=failure):
                deck = Deck()
                deck.failure = failure
                with self.assertRaises(Stop):
                    self.run_deck(deck)
                self.assertEqual(deck.state['page'], 2)
                self.assertEqual(deck.log.count('next'), 1)

    def test_state_change_during_generation_or_audio_stops(self):
        for field, value in [('page', 2), ('session_id', 'new'), ('document_revision', 2), ('blackout', True), ('opening', True)]:
            for phase in ('generate', 'audio'):
                with self.subTest(field=field, phase=phase):
                    deck = Deck()
                    def generate(context):
                        if phase == 'generate':
                            deck.state[field] = value
                        return 'Narration'
                    def play(text):
                        if phase == 'audio':
                            deck.state[field] = value
                    with self.assertRaises(Stop):
                        self.run_deck(deck, generate, play)
                    self.assertNotIn('next', deck.log)

    def test_failed_audio_does_not_advance(self):
        deck = Deck()
        def play(text):
            raise Stop('audio failed')
        with self.assertRaises(Stop):
            self.run_deck(deck, play=play)
        self.assertNotIn('next', deck.log)

    def test_loading_deadline(self):
        deck = Deck()
        deck.loading = 100000
        with self.assertRaisesRegex(Stop, 'deadline'):
            self.run_deck(deck)
        self.assertNotIn('next', deck.log)

    def test_terminal_context_failure(self):
        deck = Deck()
        original = deck.call
        def call(*args):
            if args[0] == 'context':
                raise QpError('NOTES_FAILED', 'failed')
            return original(*args)
        deck.call = call
        with self.assertRaises(QpError):
            self.run_deck(deck)
        self.assertNotIn('next', deck.log)

    def test_command_adapters_use_real_stdin_stdout(self):
        generator = command_adapter([sys.executable, '-c',
            'import sys,json; c=json.load(sys.stdin); print(json.dumps({"narration":c["current"]["notes"]}))'], 5, True)
        player = command_adapter([sys.executable, '-c',
            'import sys; assert sys.stdin.read()=="spoken"'], 5, False)
        self.assertEqual(generator({'current': {'notes': 'spoken'}}), 'spoken')
        player('spoken')

    def test_real_qp_subprocess_boundary(self):
        # The executable fixture only supplies source/state; orchestration is real.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = root / 'fixture.py'
            state = root / 'state.json'
            state.write_text(json.dumps(Deck().state))
            fixture.write_text("""import json,sys
from pathlib import Path
path=Path(sys.argv[1]); state=json.loads(path.read_text()); command=sys.argv[2]
result={'protocol_version':1}
if command=='open': result.update(state=state)
elif command=='status': result.update(state)
elif command=='context':
    page=state['page']
    result.update(presentation=state,current=dict(page=page,text=['Text'],notes='Note '+str(page),truncated=False),next=None)
elif command=='next':
    changed=state['page']<state['pages']; state['page']=min(state['pages'],state['page']+1)
    path.write_text(json.dumps(state)); result.update(changed=changed,state=state)
print(json.dumps(result))
""")
            class FixtureQp(Qp):
                def call(self, *args):
                    original = subprocess.run
                    with patch('autopresent.subprocess.run', wraps=original) as run:
                        # Keep Qp's real JSON/error handling and launch a portable fixture.
                        run.side_effect = lambda argv, **kwargs: original(
                            [sys.executable, str(fixture), str(state), *argv[1:]], **kwargs)
                        return super().call(*args)
            spoken = []
            Presenter(FixtureQp(), dry_generate, spoken.append).run('demo.pdf')
            self.assertEqual(spoken, ['Note 1', 'Note 2'])
            self.assertEqual(json.loads(state.read_text())['page'], 2)

    def test_qp_process_timeout_is_uncertain(self):
        with patch('autopresent.subprocess.run', side_effect=subprocess.TimeoutExpired('qp', 1)):
            with self.assertRaises(QpError) as caught:
                Qp().call('next')
        self.assertEqual(caught.exception.code, 'TIMEOUT')


if __name__ == '__main__':
    unittest.main()
