"""Sleep/street-life report for a village JSONL log.

Usage: python tools/sleep_report.py sim_out/circ-a.jsonl
"""

import json
import sys
from collections import Counter, defaultdict


def main(path: str) -> None:
    bedtimes = Counter()  # hour -> count (sleep started, day >= 2)
    night_wakers = defaultdict(set)  # day -> set of npcs active 23..05
    day_sleeps = Counter()  # naps started 9..18
    strolls = 0
    actions = Counter()
    deaths = []
    hungry = 0
    days = 0

    with open(path) as f:
        for line in f:
            e = json.loads(line)
            kind = e.get("event")
            tick = e.get("tick", 0)
            day = tick // 1440 + 1
            hour = (tick % 1440) // 60
            days = max(days, day)
            if kind == "npc_died":
                deaths.append((day, e.get("npc")))
            if kind == "cannot_afford_meal":
                hungry += 1
            if kind != "action_started" or day < 2:
                continue
            action = e.get("action")
            actions[action] += 1
            if action == "sleep":
                bedtimes[hour] += 1
                if 9 <= hour <= 18:
                    day_sleeps[hour] += 1
            else:
                if hour >= 23 or hour < 5:
                    night_wakers[day].add(e.get("npc"))
            if action == "stroll":
                strolls += 1

    nights = max(days - 1, 1)
    print(f"days: {days}, deaths: {deaths}, hungry_broke episodes: {hungry}")
    print(f"actions/day: " + ", ".join(f"{a}={c / nights:.1f}" for a, c in actions.most_common()))
    print("bedtime histogram (sleep starts per hour, day>=2):")
    for hour in list(range(12, 24)) + list(range(0, 12)):
        if bedtimes[hour]:
            print(f"  {hour:02}:00  {'#' * bedtimes[hour]} {bedtimes[hour]}")
    naps = sum(day_sleeps.values())
    print(f"daytime naps (09-18): {naps} total = {naps / nights:.2f}/day")
    total_night_wakers = sum(len(v) for v in night_wakers.values())
    print(f"night-active villagers (23:00-05:00): {total_night_wakers / nights:.2f}/night avg")


if __name__ == "__main__":
    main(sys.argv[1])
