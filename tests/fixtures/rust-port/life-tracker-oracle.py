"""현재 발판 생사 추적의 개발용 상태 비교. 화면·입력 장치에 접근하지 않는다."""
import json
import sys
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner.life_vision import SummonerLifeTracker


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if sys.version.split()[0] != baseline["python"]:
        raise RuntimeError("life tracker oracle baseline changed")
    result = []
    for operations in json.load(sys.stdin):
        tracker = SummonerLifeTracker("flash", "room")
        rows = []
        for op in operations:
            if op.get("moved"):
                tracker.moved()
                rows.append(None)
                continue
            alive = None if op.get("alive") is None else SimpleNamespace(**op["alive"])
            corpse = None if op.get("corpse") is None else SimpleNamespace(**op["corpse"])
            platform = None if op.get("platform") is None else SimpleNamespace(**op["platform"])
            rows.append(tracker.observe(alive, corpse, platform, sequence=op["sequence"], acquired_at=op["at"], now=op["now"]))
        result.append(rows)
    json.dump(result, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
