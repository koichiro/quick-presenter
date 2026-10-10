#!/usr/bin/env python3
"""Read narration from stdin with macOS say, waiting until speech finishes."""
import argparse
import os
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--voice', help='An installed macOS voice (default: system voice)')
    parser.add_argument('--rate', type=int, help='Speaking rate in words per minute')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This player requires macOS; use your own player on other platforms')
    if args.rate is not None and args.rate <= 0:
        parser.error('Speaking rate must be positive')
    command = ['say']
    if args.voice:
        command += ['-v', args.voice]
    if args.rate is not None:
        command += ['-r', str(args.rate)]
    # Read stdin as a file, so narration is never interpreted as command options.
    # Replace this process: the orchestrator's timeout also terminates say.
    try:
        os.execvp('say', command + ['-f', '-'])
    except OSError as exc:
        print(f'Speech playback failed: {exc}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
