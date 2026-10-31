"""기준 Python의 GUI 숫자 입력 문자 집합을 재생성한다."""
import unicodedata
from pathlib import Path


def main():
    if unicodedata.unidata_version != "11.0.0":
        raise RuntimeError("Use the baseline Python 3.7 Unicode 11.0.0 environment")
    decimals = [code for code in range(0x110000) if unicodedata.decimal(chr(code), None) == 0]
    ranges = []
    for code in range(0x110000):
        if chr(code).isdigit():
            if ranges and ranges[-1][1] + 1 == code:
                ranges[-1][1] = code
            else:
                ranges.append([code, code])
    text = (
        "// generate-unicode-numbers.py가 고정한 Unicode 11 숫자 입력 계약.\n"
        "pub(crate) const DECIMAL_STARTS: &[u32] = &[\n"
        + "".join("    0x{:x},\n".format(code) for code in decimals)
        + "];\n"
        + "pub(crate) const DIGIT_RANGES: &[(u32,u32)] = &[\n"
        + "".join("    (0x{:x},0x{:x}),\n".format(*pair) for pair in ranges)
        + "];\n"
    )
    root = Path(__file__).resolve().parents[3]
    (root / "src/common/numeric_input/unicode_numbers.rs").write_text(text, encoding="utf-8")
    print("Generated {} decimal starts and {} digit ranges".format(len(decimals), len(ranges)))


if __name__ == "__main__":
    main()
