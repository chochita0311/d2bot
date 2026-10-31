"""고정 OpenCV 그리기 테이블과 원문 고지를 재현한다. C 코드를 실행하지 않는다."""
import hashlib
import json
import re
import sys
from pathlib import Path


def generate(directory, output, notice):
    drawing = (directory / "drawing.cpp").read_bytes()
    fonts = (directory / "hershey_fonts.cpp").read_bytes()
    assert hashlib.sha256(drawing).hexdigest() == "feb12ebea7b08419289a46d043afeee665b77217d1faed89d86ad30d2d8af0e0"
    assert hashlib.sha256(fonts).hexdigest() == "5fa0d65264f72e5b166d07973aabd49f27cc54d0074062a8efc312bf9ab95432"
    source = drawing.decode("utf-8")
    font_source = fonts.decode("utf-8")
    body = re.search(r"HersheySimplex\[\]\s*=\s*\{(.*?)\};", source, re.S).group(1)
    indices = [int(n) for n in re.findall(r"\d+", body.split(",", 1)[1])]
    glyphs = [json.loads(s) for s in re.findall(r'"(?:\\.|[^"\\])*"', font_source.split("g_HersheyGlyphs[] =", 1)[1])]
    assert len(indices) == 95
    result = ["// OpenCV b4c5ec4에서 생성. 원문 라이선스: licenses/opencv-drawing-bsd.txt."]
    result.append("pub const GLYPHS: [&str; 95] = [")
    result.extend("    " + json.dumps(glyphs[i]) + "," for i in indices)
    result.append("];")
    for original, target, kind in [("SlopeCorrTable", "SLOPE", "i64"), ("FilterTable", "FILTER", "i64"), ("SinTable", "SIN", "f32")]:
        body = re.search(original + r"\[\]\s*=\s*\{(.*?)\};", source, re.S).group(1)
        values = re.findall(r"-?\d+(?:\.\d+)?f?", body)
        values = [v.rstrip("f") for v in values]
        if target == "SIN":
            result.append("// 원문의 f32 반올림을 유지해야 같은 래스터를 만든다.")
            result.append("#[allow(clippy::excessive_precision, clippy::approx_constant)]")
        result.append("pub const {}: [{}; {}] = [{}];".format(target, kind, len(values), ",".join(values)))
    output.write_text("\n".join(result) + "\n", encoding="utf-8")
    blocks = []
    for text in (source, font_source):
        header = text.split("//M*/", 1)[0]
        start = header.index("// Copyright")
        lines = []
        for line in header[start:].splitlines():
            if line.startswith("//"):
                line = line[2:]
                if line.startswith(" "):
                    line = line[1:]
            lines.append(line)
        blocks.append("\n".join(lines).strip())
    notice.write_text(
        "OpenCV drawing.cpp / hershey_fonts.cpp at b4c5ec4042f097e2a5b386b9d413ec7333d0a184\n\n"
        + "\n\n".join(dict.fromkeys(blocks))
        + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    generate(*(Path(value) for value in sys.argv[1:]))
