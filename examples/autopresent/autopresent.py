#!/usr/bin/env python3
"""External narration orchestrator for qp's stable v1 JSON interface."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time


class Stop(RuntimeError):
    pass


class QpError(Stop):
    def __init__(self, code, message):
        super().__init__(f'{code}: {message}')
        self.code = code


class Qp:
    def __init__(self, executable='qp', timeout=50):
        self.executable, self.timeout = executable, timeout

    def call(self, *args):
        try:
            result = subprocess.run([self.executable, *args, '--json'],
                                    capture_output=True, text=True, timeout=self.timeout)
        except subprocess.TimeoutExpired as exc:
            raise QpError('TIMEOUT', 'qp process deadline exceeded') from exc
        except OSError as exc:
            raise QpError('IPC_FAILURE', str(exc)) from exc
        try:
            value = json.loads(result.stderr if result.returncode else result.stdout)
        except ValueError as exc:
            raise Stop('Invalid qp JSON; mutation outcome may be unknown') from exc
        if result.returncode:
            raise QpError(value['code'], value.get('message', 'qp failed'))
        if value.get('protocol_version') != 1:
            raise Stop('Control Protocol v1 is required')
        return value


def identity(state):
    return tuple(state[k] for k in ('session_id', 'document_revision', 'document', 'page', 'pages'))


def guard(state, expected):
    if identity(state) != expected or state['opening'] or state['blackout']:
        raise Stop('Presentation changed, is opening, or is blacked out; restart explicitly')
    if state['render_state'] == 'failed' or state['notes_state'] == 'failed':
        raise Stop('Rendering or speaker notes failed')


class Presenter:
    def __init__(self, qp, generate, play, wait=60, interval=.1):
        self.qp, self.generate, self.play = qp, generate, play
        self.wait, self.interval = wait, interval

    def read(self, command, expected):
        """Reconnect read-only commands under a bounded deadline; never replay writes."""
        deadline = time.monotonic() + self.wait
        while True:
            try:
                value = self.qp.call(*command)
                state = value['presentation'] if command[0] == 'context' else value
                guard(state, expected)
                if state['render_state'] == 'ready' and state['notes_state'] == 'ready':
                    return value
            except QpError as exc:
                if exc.code not in {'NOTES_LOADING', 'BUSY', 'CANCELLED', 'TIMEOUT', 'IPC_FAILURE', 'NOT_RUNNING'}:
                    raise
                # Check identity on each successful status after reconnect/query cancellation.
                try:
                    guard(self.qp.call('status'), expected)
                except QpError as status_error:
                    if status_error.code not in {'TIMEOUT', 'IPC_FAILURE', 'NOT_RUNNING', 'BUSY'}:
                        raise
            if time.monotonic() >= deadline:
                raise Stop('Readiness/reconnection deadline exceeded')
            time.sleep(self.interval)

    def run(self, pdf):
        opened = self.qp.call('open', str(Path(pdf).resolve()))
        state = opened['state']
        if state['document'] != str(Path(pdf).resolve()) or state['page'] != 1:
            raise Stop('Open did not commit the requested PDF at page one')
        expected = identity(state)
        while True:
            context = self.read(('context', '--full'), expected)
            if context['current']['page'] != expected[3] or context['current']['truncated']:
                raise Stop('Inconsistent or truncated slide context')
            narration = self.generate(context)
            self.read(('status',), expected)
            self.play(narration)  # Must return only after all audio finishes; failures stop navigation.
            self.read(('status',), expected)
            try:
                moved = self.qp.call('next')  # Exactly once, including the final-page no-op.
            except QpError as exc:
                if exc.code in {'TIMEOUT', 'IPC_FAILURE', 'NOT_RUNNING'}:
                    # Reconcile for diagnosis, but never infer permission to retry an uncertain write.
                    observed = None
                    if observed is None:
                        try:
                            observed = self.qp.call('status')
                        except Stop:
                            pass
                    raise Stop(f'Uncertain next outcome; no replay. Observed state: {observed}') from exc
                raise
            new = moved['state']
            target = expected[:3] + (min(expected[3] + 1, expected[4]), expected[4])
            guard(new, target)
            if not moved['changed']:
                if expected[3] != expected[4]:
                    raise Stop('Unexpected no-op before the final page')
                return
            if expected[3] == expected[4]:
                raise Stop('Unexpected mutation at the final page')
            expected = target


def dry_generate(context):
    current = context['current']
    return current['notes'] or '\n'.join(current['text']) or '[Image-only slide; no source narration]'


def command_adapter(argv, timeout, decode):
    def invoke(value):
        payload = json.dumps(value, ensure_ascii=False) if decode else value
        result = subprocess.run(argv, input=payload, text=True, capture_output=True,
                                timeout=timeout, check=True)
        if decode:
            response = json.loads(result.stdout)
            if not isinstance(response.get('narration'), str) or not response['narration'].strip():
                raise Stop('Generator must return nonempty narration')
            return response['narration']
        return None
    return invoke


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pdf')
    parser.add_argument('--qp', default='qp')
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--generate-command', help='JSON argv array; context JSON in, {"narration":"..."} out')
    parser.add_argument('--play-command', help='JSON argv array; narration on stdin; wait for audio completion')
    parser.add_argument('--timeout', type=float, default=50, help='Per qp process deadline in seconds')
    parser.add_argument('--wait', type=float, default=60, help='Readiness/reconnect deadline in seconds')
    parser.add_argument('--adapter-timeout', type=float, default=300)
    args = parser.parse_args()
    if min(args.timeout, args.wait, args.adapter_timeout) <= 0:
        parser.error('Timeouts must be positive')
    if not args.dry_run and not (args.generate_command and args.play_command):
        parser.error('Use --dry-run or provide both adapter commands')
    try:
        if args.dry_run:
            generate = dry_generate
            play = lambda narration: print(json.dumps({'narration': narration}, ensure_ascii=False), flush=True)
        else:
            def argv(raw):
                value = json.loads(raw)
                if not isinstance(value, list) or not value or not all(isinstance(x, str) for x in value):
                    raise Stop('Adapter commands must be nonempty JSON string arrays')
                return value
            generate = command_adapter(argv(args.generate_command), args.adapter_timeout, True)
            play = command_adapter(argv(args.play_command), args.adapter_timeout, False)
        Presenter(Qp(args.qp, args.timeout), generate, play, args.wait).run(args.pdf)
    except (Stop, OSError, ValueError, subprocess.SubprocessError) as exc:
        print(f'Autopresentation stopped: {exc}', file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    return 0


if __name__ == '__main__':
    sys.exit(main())
