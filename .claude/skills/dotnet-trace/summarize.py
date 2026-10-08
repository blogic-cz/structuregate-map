"""Where a .NET process spent its time, from a `dotnet-trace --format speedscope` file.

    python summarize.py trace.speedscope.json [--cpu] [--match Name ...] [--top 25] [--thread N | --threads]

Prints, per method, INCLUSIVE time (the method and everything it called) and SELF time (the method alone),
summed over every thread - so work run in parallel adds up to more than the wall clock, and a method that
waits shows up under UNMANAGED_CODE_TIME rather than under itself. `--match` keeps the frames whose name
contains any of the given words (case-insensitive), which is how a trace of a big framework is read for
one's own code.

Standard library only; plain substring matching.
"""
from __future__ import annotations

import argparse
import collections
import json
import sys


def load(path: str) -> dict:
    with open(path, encoding='utf-8') as handle:
        return json.load(handle)


# dotnet-trace ends every stack in one of these: WHAT the time was, not WHERE. Self time is charged to the
# method just under it, and the pseudo-frame itself is reported apart.
PSEUDO = ('CPU_TIME', 'UNMANAGED_CODE_TIME')


def aggregate(trace: dict, thread: int | None, cpu: bool = False) -> tuple[collections.Counter, collections.Counter, dict]:
    """Inclusive and self time per frame index, and each thread's span."""
    names = [frame['name'] for frame in trace['shared']['frames']]

    def counted(stack: list[int]) -> bool:
        # --cpu: only a sample where the thread RAN managed code - a wait is not work.
        return not cpu or (bool(stack) and names[stack[-1]] == 'CPU_TIME')

    def owner(stack: list[int]) -> int | None:
        for frame in reversed(stack):
            if names[frame] not in PSEUDO:
                return frame
        return None

    inclusive: collections.Counter = collections.Counter()
    alone: collections.Counter = collections.Counter()
    spans: dict = {}
    for number, profile in enumerate(trace['profiles']):
        if thread is not None and number != thread:
            continue
        spans[profile.get('name', str(number))] = profile.get('endValue', 0) - profile.get('startValue', 0)
        if profile.get('type') == 'evented':
            stack: list[int] = []
            last = None
            for event in profile['events']:
                at = event['at']
                if stack and last is not None and counted(stack):
                    for frame in set(stack):
                        inclusive[frame] += at - last
                    if (mine := owner(stack)) is not None:
                        alone[mine] += at - last
                last = at
                if event['type'] == 'O':
                    stack.append(event['frame'])
                elif stack:
                    stack.pop()
        else:
            for stack, weight in zip(profile['samples'], profile['weights']):
                if not counted(stack):
                    continue
                for frame in set(stack):
                    inclusive[frame] += weight
                if (mine := owner(stack)) is not None:
                    alone[mine] += weight
    return inclusive, alone, spans


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('trace')
    parser.add_argument('--match', nargs='*', default=[], help='keep frames whose name contains any of these')
    parser.add_argument('--top', type=int, default=25)
    parser.add_argument('--cpu', action='store_true', help='only time the threads spent RUNNING managed code, not waiting')
    parser.add_argument('--thread', type=int, help='one profile (thread) by its index, as --threads lists them')
    parser.add_argument('--threads', action='store_true', help='list the threads and their spans, then stop')
    args = parser.parse_args()

    trace = load(args.trace)
    frames = [frame['name'] for frame in trace['shared']['frames']]
    unit = trace['profiles'][0].get('unit', 'milliseconds') if trace['profiles'] else 'milliseconds'
    scale = 1000.0 if unit == 'milliseconds' else 1.0
    inclusive, alone, spans = aggregate(trace, args.thread, args.cpu)

    if args.threads:
        for number, (name, span) in enumerate(spans.items()):
            print(f'{number:3}  {span / scale:9.1f}s  {name}')
        return 0

    words = [word.lower() for word in args.match]

    def kept(name: str) -> bool:
        # The process and thread roots carry everything; they are not methods.
        if name in PSEUDO or name in ('Threads', '(Non-Activities)') or name.startswith('Process64 ') or name.startswith('Thread ('):
            return False
        return not words or any(word in name.lower() for word in words)

    busy = {name: inclusive[i] for i, name in enumerate(frames) if name in PSEUDO}
    print('== ' + ', '.join(f'{name} {weight / scale:.1f}s' for name, weight in busy.items()) + ' (summed over threads)')
    print()
    for title, counter in (('INCLUSIVE (the method and all it called)', inclusive), ('SELF (the method alone)', alone)):
        print(f'== {title}, seconds summed over threads')
        shown = 0
        for frame, weight in counter.most_common():
            name = frames[frame]
            if not kept(name):
                continue
            print(f'{weight / scale:9.1f}s  {name[:160]}')
            shown += 1
            if shown == args.top:
                break
        print()
    return 0


if __name__ == '__main__':
    sys.exit(main())
