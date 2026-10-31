"""고정 공개 참조의 MPEG-4 VLC 데이터만 Rust 표로 생성한다."""
import hashlib
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[3]
source = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(source).hexdigest() == "9cb73c2482d73d3cf11fce3be647d889d2380adb6d0db831f9917311351f30f2"
text = source.decode("utf-8")
plain = re.sub(r"/\*.*?\*/", "", text, flags=re.S)


def table(name):
    match = re.search(r"static const VLCtable " + re.escape(name) + r"\b[^=]*=\s*(.*?);", plain, re.S)
    assert match is not None, name
    return [(int(code, 0), int(bits)) for code, bits in re.findall(r"\{\s*(0x[0-9a-fA-F]+|[0-9]+)\s*,\s*([0-9]+)\s*\}", match[1])]


def coefficients(intra):
    rows = []

    def add(name, last, first_run, columns):
        for index, (code, bits) in enumerate(table(name)):
            if bits:
                rows.append((last, first_run + index // columns, 1 + index % columns, code, bits))

    if intra:
        add("coeff_tab4", False, 0, 27)
        add("coeff_tab5", False, 1, 10)
        add("coeff_tab6", False, 2, 5)
        add("coeff_tab7", False, 10, 1)
        add("coeff_tab8", True, 0, 8)
        add("coeff_tab9", True, 1, 3)
        add("coeff_tab10", True, 7, 1)
    else:
        add("coeff_tab0", False, 0, 12)
        add("coeff_tab1", False, 2, 4)
        add("coeff_tab2", True, 0, 3)
        add("coeff_tab3", True, 2, 1)
    assert len(rows) == 102
    return rows


lines = [
    "//! MPEG-4 VLC 표. 출처·라이선스는 THIRD-PARTY-NOTICES.md와 mpeg4-reference-notice.txt 참조.",
    "//! generate-mpeg4-vlc.py가 고정 해시의 공개 참조에서 생성한다.",
]
for rust_name, values in [
    ("DC_LUMA", table("DCtab_lum")),
    ("DC_CHROMA", table("DCtab_chrom")),
    ("MCBPC_I", [table("mcbpc_intra_tab")[index * 4 + 1] for index in range(4)]),
    ("MCBPC_P", [table("mcbpc_inter_tab")[index * 8] for index in range(4)]),
    ("CBPY", table("cbpy_tab")),
    ("MV", table("mvtab")),
]:
    lines.append("pub(super) const " + rust_name + ": [(u16,u8);" + str(len(values)) + "] = [")
    lines.extend("(" + str(code) + "," + str(bits) + ")," for code, bits in values)
    lines.append("];")
for name, intra in [("AC_INTRA", True), ("AC_INTER", False)]:
    rows = coefficients(intra)
    lines.append("pub(super) const " + name + ": [(bool,u8,u8,u16,u8);102] = [")
    for last, run, level, code, bits in rows:
        lines.append("(" + str(last).lower() + "," + ",".join(str(n) for n in (run, level, code, bits)) + "),")
    lines.append("];")
destination = ROOT / "src/common/recording/mpeg4/tables.rs"
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
notice = text.split("/***********************************************************HeaderBegin", 1)[0]
(ROOT / "licenses/mpeg4-reference-notice.txt").write_text(
    "Scope: MPEG-4 VLC data in src/common/recording/mpeg4/tables.rs.\n"
    "Source: Android platform/frameworks/av, commit 1e6a675fccdc37afcfb337f8fa97d1ddab90adf0,\n"
    "media/codecs/m4v_h263/enc/src/vlc_enc_tab.h\n"
    "SHA256: 9cb73c2482d73d3cf11fce3be647d889d2380adb6d0db831f9917311351f30f2\n"
    "The source notices below are retained for this data adaptation.\n\n" + notice,
    encoding="utf-8",
)
