"""고정 Python parser의 실제 인자 변환만 실행하며 게임 효과는 호출하지 않는다."""
import contextlib
import io
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.app import build_parser


def inspect(arguments):
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        try:
            parsed = build_parser().parse_args(arguments)
        except SystemExit as error:
            return {"exit": error.code}
    return {"exit": 0, "arguments": vars(parsed)}


if __name__ == "__main__":
    request = json.load(sys.stdin)
    print(json.dumps([inspect(arguments) for arguments in request], ensure_ascii=False, default=str))
