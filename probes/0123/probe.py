#!/usr/bin/env python3
"""Record 0123 (from 0122's probe): the workflow probe, with a display check. Runs every step through an rnx-polars
binary, compares each result with its Rust twin, attributes every blocked
step from the committed table below, and ranks the blocker families by the
plan's rule (fixed before probing).

  probe.py <rnx-polars binary> <twins.json or -> <out-dir> <pin> [--fixed FAMILY ...] [--display <presenter binary>]

With `--display`, every computed frame result is also shown the two ways a
user sees it, through a binary that registers the adapter's presenter (as a
`:dep polars` session does): printed (`println!("{}", result)`, the frame's
DISPLAY_FMT) and presented (the bare value at a session prompt, record
0068's presenter). Computation and display are reported apart; a workflow is
usable only if it computes and its result displays both ways.

With `-` for twins (0.55.2, where the twins' Polars features are absent),
outcomes are recorded by exit status only and nothing is ranked.
"""
import json
import os
import re
import subprocess
import sys
import tempfile

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
from steps import PRESENTATION, SETUPS, STEPS, WORKFLOWS, script  # noqa: E402

# steps whose rows Polars does not order (a left join without maintain_order)
UNORDERED = {"w6.join_lazy", "w5.eager_group_by", "W6"}
# steps whose float results Polars does not reproduce bit for bit from run to
# run (its `std` reduction varies in the last ulp: 2.1602468994692865 and
# 2.160246899469287 from the same binary, probed); compared to 12 digits
APPROX = {"w2.summary"}

# every blocker family: its kind, risk (plan: low / medium / high), and the
# evidence behind it (a surface row's recorded reason, or the host gap)
FAMILIES = {
    "A.collect_dtypes": ("host", "low",
                         "hand-written LazyFrame::collect (record 0058) rejects any column dtype outside String/Int64/Float64/Boolean (files::validate), although generated eager operations already return frames of any dtype"),
    "B.read_csv_schema": ("host", "low",
                          "hand-written read_csv requires an explicit, non-empty (name, dtype) schema; a plain read_csv(path) is an arity error"),
    "C.json_reader": ("surface", "medium",
                      "polars_io::json::JsonReader: generic owner (the reader over script bytes is not instantiated as 0118 did for CSV/IPC/Parquet)"),
    "D.path_type": ("surface", "medium",
                    "LazyFrame::scan_parquet takes polars_utils::pl_path::PlRefPath, an internal-crate type out of scope; a script string is not converted"),
    "E.schema_display": ("presentation", "low",
                         "SchemaRef has no DISPLAY_FMT protocol, so a schema cannot be shown in a template or print"),
    "F.dtype_expr_arg": ("surface", "medium",
                         "Expr::cast takes DataTypeExpr; a DataType argument is not converted (DataTypeExpr::from_data_type exists, but natural code passes a DataType)"),
    "G.eager_group_by": ("surface", "high",
                         "DataFrame::group_by / group_by_stable: lifetime type: return (polars_core::frame::group_by::GroupBy)"),
    "H.lazy_pivot": ("surface", "medium",
                     "LazyFrame::pivot: foreign type: on_columns (alloc::sync::Arc<polars_core::frame::dataframe::DataFrame>)"),
    "I.arity": ("surface", "medium",
                "LazyFrame::pivot has 8 arguments besides the receiver; the generator binds at most 5 (receiver included) and refuses by arity"),
}

# display families: why a computed frame does not display
DISPLAY_FAMILIES = {
    "P.preview_dtypes": ("presentation", "low",
                         "the adapter's preview (record 0058, preview.rs dtype()) renders only String/Int64/Float64/Boolean columns; printing and the session presenter both use it, so a Date, a count (UInt32) or any other dtype shows 'preview unavailable: unsupported column dtype'"),
    "Q.preview_columns": ("presentation", "low",
                          "the preview shows the first columns up to its limit and omits the rest ('[0 rows and N columns omitted by display limits]'); a workflow's derived columns are appended last, so they are the ones cut, and a dtype the preview cannot show is never reached (Polars' own display elides middle columns instead)"),
}


def omitted_columns(text):
    m = re.search(r"(\d+) rows and (\d+) columns omitted by display limits", text)
    return int(m.group(2)) if m else 0


def display_family(text):
    if "unsupported column dtype" in text:
        return "P.preview_dtypes"
    if omitted_columns(text):
        return "Q.preview_columns"
    return None


# every blocked step's blockers (all of them; a step blocked by more than one
# family is credited to none)
BLOCKERS = {
    "w1.csv_file": ["B.read_csv_schema"],
    "w1.json_bytes": ["C.json_reader"],
    "w1.scan_parquet": ["D.path_type"],
    "w2.schema": ["E.schema_display"],
    "w3.cast": ["F.dtype_expr_arg"],
    "w4.parse_date": ["A.collect_dtypes"],
    "w4.temporal": ["A.collect_dtypes"],
    "w5.eager_group_by": ["G.eager_group_by"],
    "w5.lazy_group_by": ["A.collect_dtypes"],
    "w6.pivot": ["H.lazy_pivot", "I.arity"],
    "w7.rank": ["A.collect_dtypes"],
}

# every composed workflow's blockers (its steps', composed)
WF_BLOCKERS = {
    "W1": ["B.read_csv_schema", "C.json_reader", "D.path_type"],
    "W2": ["E.schema_display"],
    "W3": ["F.dtype_expr_arg"],
    "W4": ["A.collect_dtypes"],
    "W5": ["A.collect_dtypes", "G.eager_group_by"],
    "W6": ["H.lazy_pivot", "I.arity"],
    "W7": ["A.collect_dtypes"],
}


def unordered(text):
    """A frame's rows as a multiset: the text between `rows=[` and `]`."""
    head, sep, rest = text.partition("rows=[")
    if not sep:
        return text
    rows, _, tail = rest.rpartition("]")
    return head + sep + " | ".join(sorted(rows.split(" | "))) + "]" + tail


def approx(text):
    import re
    return re.sub(r"Float64\(([-0-9.e+]+)\)", lambda m: f"Float64({float(m.group(1)):.12g})", text)


def run_all(binary):
    data = os.path.join(here, "data")
    work = tempfile.mkdtemp(prefix="probe0122-")
    subs = lambda t: t.replace("{DATA}", data).replace("{OUT}", work)

    def run(src, name):
        path = os.path.join(work, name + ".rn")
        open(path, "w").write(subs(src))
        p = subprocess.run([binary, "run", path], capture_output=True, text=True, timeout=120,
                           stdin=subprocess.DEVNULL, cwd=work)
        err = [l for l in p.stderr.splitlines() if l.strip() and not l.strip().startswith(("|", "=", "-->"))
               and set(l.strip()) - set("^ ")]
        return {"exit": p.returncode, "stdout": p.stdout.strip(),
                "error": re.sub(r"\(\d+\)", "(N)", " // ".join(err[:2]).replace(work, "{OUT}").replace(data, "{DATA}"))[:400]}

    setup = ("pub fn main(args) {\n    " + SETUPS["sales"] + "\n"
             '    df.write_parquet_new("{OUT}/sales.parquet")?;\n    Ok(())\n}\n')
    assert run(setup, "fixture")["exit"] == 0
    return {s[0]: run(script(s), s[0].replace(".", "_")) for s in STEPS + WORKFLOWS}


def display_all(binary, computed):
    """Print and present every computed frame result through the presenter binary."""
    data = os.path.join(here, "data")
    work = tempfile.mkdtemp(prefix="probe0123-display-")
    subs = lambda t: t.replace("{DATA}", data).replace("{OUT}", work)
    setup = ("pub fn main(args) {\n    " + SETUPS["sales"] + "\n"
             '    df.write_parquet_new("{OUT}/sales.parquet")?;\n    Ok(())\n}\n')
    path = os.path.join(work, "fixture.rn")
    open(path, "w").write(subs(setup))
    assert subprocess.run([binary, "run", path], capture_output=True, stdin=subprocess.DEVNULL, cwd=work).returncode == 0
    out = {}
    for step in STEPS + WORKFLOWS:
        sid = step[0]
        if sid not in computed:
            continue
        src = script(step).replace('println!("{}", polars::oracle_repr(result)?);', 'println!("{}", result);')
        p = os.path.join(work, sid.replace(".", "_") + "_print.rn")
        open(p, "w").write(subs(src))
        r = subprocess.run([binary, "run", p], capture_output=True, text=True, timeout=120, stdin=subprocess.DEVNULL, cwd=work)
        printed = r.stdout.strip().splitlines()[-1:] and r.stdout
        # the session: the same computation as one bare expression at the prompt
        _sid, _t, setup_name, code = step
        body = code.replace("{regions}", SETUPS["regions"])
        pre = SETUPS[setup_name] if setup_name else ""
        line = "{ " + (pre + " " if pre else "") + " ".join(l.strip() for l in body.split("\n")) + " }"
        q = subprocess.run([binary, "repl"], input=subs(line) + "\n", capture_output=True, text=True, timeout=120, cwd=work)
        presented = "\n".join(l for l in q.stdout.splitlines() if not l.startswith("rnx: a Rune session"))

        def outcome(text, ok_marker):
            if "preview unavailable" in text or "error" in text.lower() and ok_marker not in text:
                return {"outcome": "refused", "cause": display_family(text), "text": re.sub(r"\(\d+\)", "(N)", text.strip())[-300:]}
            if ok_marker not in text:
                return {"outcome": "refused", "cause": display_family(text), "text": text.strip()[:200]}
            # shown, but with columns cut by the display limit: the result is not all visible
            if omitted_columns(text):
                return {"outcome": "partial", "cause": "Q.preview_columns", "columns_omitted": omitted_columns(text),
                        "text": text.strip()[:200]}
            return {"outcome": "displayed", "cause": None, "text": text.strip()[:200]}
        shown = {"printed": outcome(r.stdout + r.stderr, " rows ×"), "presented": outcome(presented + q.stderr, " rows ×")}
        if sid in APPROX:
            # the same last-ulp variation as the computation (0122): 12 digits
            for v in shown.values():
                v["text"] = re.sub(r"\d+\.\d{13,}", lambda m: f"{float(m.group()):.12g}", v["text"])
        out[sid] = shown
    return out


def main():
    binary, twins_p, out, pin = os.path.abspath(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
    # Stage 2: families fixed in this build; their steps must now work
    fixed = set()
    if "--fixed" in sys.argv:
        for a in sys.argv[sys.argv.index("--fixed") + 1:]:
            if a.startswith("--"):
                break
            fixed.add(a)
    assert fixed <= set(FAMILIES), fixed - set(FAMILIES)
    strip = lambda table: {k: [f for f in b if f not in fixed] for k, b in table.items() if [f for f in b if f not in fixed]}
    blockers, wf_blockers = strip(BLOCKERS), strip(WF_BLOCKERS)
    enabled = sorted(set(BLOCKERS) - set(blockers))
    completed = sorted(set(WF_BLOCKERS) - set(wf_blockers))
    twins = json.load(open(twins_p)) if twins_p != "-" else None
    results = run_all(binary)
    rows = []
    for sid, title, _setup, _code in STEPS + WORKFLOWS:
        r = results[sid]
        if r["exit"] != 0:
            outcome = "blocked"
        elif sid in PRESENTATION:
            outcome = "presented" if all(m in r["stdout"] for m in PRESENTATION[sid]) else "differs"
        elif twins is None or twins[sid].startswith("<<twin"):
            outcome = "works" if twins is None else "works (no twin)"
        else:
            # a composed workflow's result is its last line (`oracle_repr` of a
            # frame); it may print along the way (W2's schema, W8's preview). A
            # step prints only its result, which may span lines (CSV, JSON)
            composed_ = sid in {w[0] for w in WORKFLOWS}
            a = (r["stdout"].splitlines() or [""])[-1] if composed_ else r["stdout"]
            b = twins[sid].strip()
            if sid in UNORDERED:
                a, b = unordered(a), unordered(b)
            if sid in APPROX:
                a, b = approx(a), approx(b)
            outcome = "works" if a == b else "differs"
        composed = sid in {w[0] for w in WORKFLOWS}
        row = {"step": sid, "workflow": sid if composed else sid.split(".")[0].upper(), "composed": composed,
               "title": title, "outcome": outcome}
        if outcome == "blocked":
            row["error"] = r["error"]
            if twins is not None:
                # a KeyError is an unattributed step or workflow: stop
                row["blockers"] = (wf_blockers if composed else blockers)[sid]
        if outcome == "differs":
            row["rune"], row["rust"] = r["stdout"][:600], twins[sid][:600]
        rows.append(row)
    steps_ = [r for r in rows if not r["composed"]]
    flows = [r for r in rows if r["composed"]]
    count = lambda rs: {k: sum(1 for r in rs if r["outcome"] == k) for k in ("works", "differs", "blocked", "presented")}
    doc = {"pin": pin, "steps": rows, "counts": count(steps_), "workflow_counts": count(flows)}
    if twins is not None:
        # every attributed step must actually be blocked, and vice versa
        blocked = {r["step"] for r in steps_ if r["outcome"] == "blocked"}
        assert blocked == set(blockers), (blocked ^ set(blockers))
        wf_blocked = {r["step"] for r in flows if r["outcome"] == "blocked"}
        assert wf_blocked == set(wf_blockers), (wf_blocked ^ set(wf_blockers))
        # a fixed family's steps and workflows work, matching their twins
        bad = [r["step"] for r in rows if r["step"] in enabled + completed and r["outcome"] != "works"]
        assert not bad, f"fixed family's steps or workflows do not match their twins: {bad}"
        doc["fixed"], doc["enabled"], doc["completed"] = sorted(fixed), enabled, completed
        ranking = []
        for fam, (kind, risk, evidence) in FAMILIES.items():
            if fam in fixed:
                continue
            steps = [s for s, b in blockers.items() if fam in b]
            marginal = [s for s in steps if blockers[s] == [fam]]
            multiply = [s for s in steps if len(blockers[s]) > 1]
            # a composed workflow completes only if this family alone blocks it
            complete = sorted(w for w, b in wf_blockers.items() if b == [fam])
            ranking.append({"family": fam, "kind": kind, "risk": risk, "evidence": evidence,
                            "marginal_steps": marginal, "marginal_workflows": complete,
                            "multiply_blocked_steps": multiply})
        order = {"low": 0, "medium": 1, "high": 2}
        ranking.sort(key=lambda f: (-len(f["marginal_steps"]), -len(f["marginal_workflows"]), order[f["risk"]], f["family"]))
        doc["ranking"] = ranking
    if "--display" in sys.argv:
        presenter = os.path.abspath(sys.argv[sys.argv.index("--display") + 1])
        # a frame result: the oracle's frame text (`shape=`), computed as its twin
        computed = {r["step"] for r in rows if r["outcome"] in ("works", "works (no twin)")
                    and (results[r["step"]]["stdout"].splitlines() or [""])[-1].startswith("shape=")}
        shown = display_all(presenter, computed)
        for r in rows:
            if r["step"] in shown:
                r["display"] = shown[r["step"]]
        usable = lambda r: r["outcome"] == "works" and all(d["outcome"] == "displayed" for d in r.get("display", {}).values()) and "display" in r
        doc["display_counts"] = {
            "computed_frames": len(computed),
            "displayed_both_ways": sum(1 for r in rows if "display" in r and all(d["outcome"] == "displayed" for d in r["display"].values())),
            "partial_both_ways": sum(1 for r in rows if "display" in r and any(d["outcome"] == "partial" for d in r["display"].values())),
            "usable_workflows": sorted(r["step"] for r in rows if r["composed"] and usable(r)),
        }
        # the display ranking: frames each family alone keeps from displaying
        dranking = []
        for fam, (kind, risk, evidence) in DISPLAY_FAMILIES.items():
            steps_ = [r["step"] for r in rows if "display" in r and not r["composed"]
                      and any(d["cause"] == fam for d in r["display"].values())
                      and all(d["outcome"] == "displayed" or d["cause"] == fam for d in r["display"].values())]
            flows_ = [r["step"] for r in rows if "display" in r and r["composed"]
                      and any(d["cause"] == fam for d in r["display"].values())
                      and all(d["outcome"] == "displayed" or d["cause"] == fam for d in r["display"].values())]
            dranking.append({"family": fam, "kind": kind, "risk": risk, "evidence": evidence,
                             "steps_made_displayable": steps_, "workflows_made_usable": flows_})
        unattributed = [r["step"] for r in rows if "display" in r
                        and any(d["outcome"] != "displayed" and d["cause"] is None for d in r["display"].values())]
        doc["display_ranking"], doc["display_unattributed"] = dranking, unattributed
    os.makedirs(out, exist_ok=True)
    path = os.path.join(out, f"outcomes-{pin}.json")
    json.dump(doc, open(path, "w"), indent=1, sort_keys=True)
    open(path, "a").write("\n")
    print(json.dumps({"steps": doc["counts"], "workflows": doc["workflow_counts"], "display": doc.get("display_counts")}))
    for f in doc.get("display_ranking", []):
        print(f"  display {f['family']:18s} steps {len(f['steps_made_displayable'])} workflows {f['workflows_made_usable']}")
    if doc.get("display_unattributed"):
        print("  display unattributed:", doc["display_unattributed"])
    for f in doc.get("ranking", []):
        print(f"  {f['family']:20s} steps {len(f['marginal_steps'])} workflows {f['marginal_workflows']} risk {f['risk']}")


if __name__ == "__main__":
    main()
