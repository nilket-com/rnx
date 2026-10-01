"""Record 0133: the frozen chunking rule, in Python. Used for the twin's
cross-check and the diagnostic PyTorch timing. Paragraphs are split on blank
lines and packed into passages of at most 180 words; a longer paragraph is cut
into 180-word windows. A word is a space-separated token after newlines, tabs
and CRs become spaces, which is exactly the Rune script's rule."""
import pathlib


def words(s):
    return [w for w in s.replace("\n", " ").replace("\t", " ").replace("\r", " ").split(" ") if w != ""]


def chunks(body):
    out, cur = [], []
    for para in body.split("\n\n"):
        ws = words(para)
        if not ws:
            continue
        if len(ws) > 180:
            if cur:
                out.append(" ".join(cur)); cur = []
            for i in range(0, len(ws), 180):
                out.append(" ".join(ws[i:i + 180]))
            continue
        if len(cur) + len(ws) > 180:
            out.append(" ".join(cur)); cur = []
        cur.extend(ws)
    if cur:
        out.append(" ".join(cur))
    return out


def load(d):
    docs = []
    for p in sorted(pathlib.Path(d).iterdir(), key=lambda p: p.name.encode()):
        if not p.name.endswith(".md"):
            continue
        text = p.read_text()
        first = text.split("\n")[0]
        docs.append((p.name, first[2:] if first.startswith("# ") else first, chunks(text[len(first):])))
    return docs
