"""Record 0153: the annotators' view (plans/0153 section 2). For a split file,
writes exactly each ticket's number, title and body, in the split's order:
no maintainer labels, state, dates or system output.

  annotator_view.py D3_DIR SPLIT_TSV OUT_TXT"""
import json, pathlib, sys

d3, split, out = sys.argv[1:4]
by = {i["number"]: i for i in json.load(open(pathlib.Path(d3) / "issues.json"))}
ns = [int(l) for l in open(split).read().splitlines()[1:] if l]
parts = []
for n in ns:
    i = by[n]
    parts.append(f"===== ticket {n} =====\nTITLE: {i['title']}\nBODY:\n{i['body']}\n")
pathlib.Path(out).write_text("\n".join(parts))
print(f"{len(ns)} tickets written to {out}")
