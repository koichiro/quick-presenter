#!/usr/bin/env python3
"""Return the current slide's speaker notes verbatim for autopresent.py."""
import json
import sys


def narration(context):
    notes = context['current']['notes']
    if not isinstance(notes, str) or not notes.strip():
        raise ValueError('Current slide has no speaker notes; add a narration before presenting')
    return notes


def main():
    try:
        print(json.dumps({'narration': narration(json.load(sys.stdin))}, ensure_ascii=False))
    except (ValueError, KeyError, TypeError) as exc:
        print(f'Notes narration failed: {exc}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
