"""개발 비교용 Python 설정 oracle. 배포 앱에서는 실행하지 않는다."""

import json
import math
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.config import apply_character_selection, load_config


def normalize(value):
    if isinstance(value, float) and not math.isfinite(value):
        return {"$python_nonfinite": str(value)}
    if isinstance(value, dict):
        return {key: normalize(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [normalize(item) for item in value]
    return value


try:
    config = load_config(Path(sys.argv[1]))
    request = json.loads(sys.argv[2])
    if "override_active" in request:
        config.active_character = request["override_active"]
    for key in request.get("select", []):
        apply_character_selection(config, key)
    mutation = request.get("mutate_run")
    if mutation:
        setattr(config.run_profiles[mutation["key"]], mutation["field"], mutation["value"])
    result = {
        "snapshot": normalize(asdict(config)),
        "character_order": list(config.characters),
        "run_order": list(config.run_profiles),
        "farm_is_registered": any(config.farm is item for item in config.run_profiles.values()),
    }
except Exception as error:
    result = {"error_type": type(error).__name__, "error_message": str(error)}

print(json.dumps(result, ensure_ascii=False, allow_nan=False))
