"""Record 0133, diagnostic: truncation under the frozen rules, counted with the
pinned tokenizer (special tokens included).

  truncation.py MODEL_DIR DATA_DIR      (needs the `tokenizers` Python package)"""
import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import chunking
from tokenizers import Tokenizer

model, data = sys.argv[1:3]
tok = Tokenizer.from_file(f"{model}/tokenizer.json")
tok.no_truncation(); tok.no_padding()


def report(name, texts):
    lens = [len(tok.encode(t).ids) for t in texts]
    over = [l for l in lens if l > 256]
    lost = sum(l - 256 for l in over)
    s = sorted(lens)
    print(f"{name}: {len(lens)} inputs; tokens median {s[len(s)//2]}, p90 {s[int(len(s)*.9)]}, max {s[-1]}; "
          f"over 256: {len(over)} ({100*len(over)/len(lens):.1f}%); tokens lost to truncation: {lost} of {sum(lens)} ({100*lost/sum(lens):.1f}%)")
    return lens


docs = chunking.load(f"{data}/d1/plans")
passages = [(name, c) for name, _, cs in docs for c in cs]
lens = report("D1 passages (pinned tokenizer, special tokens included)", [c for _, c in passages])
rubric = {r for line in (pathlib.Path(__file__).parent.parent / "rubric.tsv").read_text().splitlines()[1:] for r in line.split("\t")[3].split()}
hit = [l for (n, _), l in zip(passages, lens) if n[:4] in rubric and l > 256]
print(f"rubric records' passages truncated: {len(hit)} of {sum(1 for n, _ in passages if n[:4] in rubric)}")
issues = json.load(open(f"{data}/d2/issues.json"))
report("D2 ticket inputs", [i["title"] + "\n" + " ".join(chunking.words(i["body"])[:180]) for i in issues])
