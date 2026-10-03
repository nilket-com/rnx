"""Record 0154: measure fixtures (plans/0154 section 3): hand-built cases for
certain, possible and consensus-only errors and either-label-correct; the
folding of performance into bug; the maintainer mapping's multi and none;
Wilson bounds at 0 and n; the outcome rule at its boundaries.

  fixtures.py"""
import math, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluate as e4

ok = True


def check(name, good, detail=""):
    global ok
    ok &= bool(good)
    print(f"{'pass' if good else 'WRONG'}: {name}{': ' + str(detail) if detail else ''}")


# tickets 1..6: annotators a, b (folded), predictions
a = {1: "bug", 2: "bug", 3: "feature", 4: "question", 5: "documentation", 6: "bug"}
b = {1: "bug", 2: "feature", 3: "question", 4: "question", 5: "documentation", 6: "bug"}
pred = {1: ("bug", 0.5), 2: ("bug", 0.5), 3: ("bug", 0.5), 4: ("question", 0.01), 5: None, 6: ("feature", 0.9)}
m = e4.measures(pred, a, b, 0.1)
check("auto-routed: confidence >= t, mandatory REVIEW never", m["auto_set"] == [1, 2, 3, 6], m["auto_set"])
check("all tickets count: N, A, workload", m["N"] == 6 and abs(m["A"] - 4 / 6) < 1e-15 and abs(m["W"] - 2 / 6) < 1e-15)
check("possible errors: differ from at least one annotator (2, 3, 6)", m["possible"] == 3)
check("certain errors: match neither annotator (3, 6)", m["certain"] == 2)
check("consensus-only: tickets 1 and 6 agree; 6 is wrong", m["consensus_auto"] == 2 and m["consensus_wrong"] == 1)
check("either-label-correct = auto - certain", m["either_correct"] == 2 and abs(m["either_correct_rate"] - 0.5) < 1e-15)
check("two separate costs, never added",
      abs(m["cost_possible"][2] - (2 / 6 + 2 * 3 / 6)) < 1e-15 and abs(m["cost_certain"][5] - (2 / 6 + 5 * 2 / 6)) < 1e-15)
check("misroutes keyed by (Claude, Codex) -> predicted",
      m["misroutes_by_queue"] == {"(bug, feature) -> bug": 1, "(feature, question) -> bug": 1, "(bug, bug) -> feature": 1}, m["misroutes_by_queue"])
check("performance folds into bug", e4.ev.fold("performance") == "bug" and e4.ev.fold("feature") == "feature")

# agreement
ag = e4.agreement({1: "bug", 2: "performance", 3: "feature"}, {1: "bug", 2: "bug", 3: "question"})
check("five-label vs four-queue agreement (performance vs bug agrees as queues)",
      ag["five_labels"]["agree"] == 1 and ag["four_queues"]["agree"] == 2, ag)

# maintainer mapping
issues = {1: {"labels": ["bug"]}, 2: {"labels": ["Enhancement", "new feature"]}, 3: {"labels": ["bug", "question"]},
          4: {"labels": []}, 5: {"labels": ["vm", "priority"]}, 6: {"labels": ["regression", "Docs"]}}
mq = e4.maintainer_queues(issues, [1, 2, 3, 4, 5, 6])
check("maintainer map: case-insensitive, one queue, multi, none",
      mq == {1: "bug", 2: "feature", 3: "multi", 4: "none", 5: "none", 6: "multi"}, mq)

# Wilson at 0 and n
lo, hi = e4.ev.wilson(0, 20)
check("Wilson at 0/n: lower 0", lo == 0.0 and 0 < hi < 1)
lo, hi = e4.ev.wilson(20, 20)
check("Wilson at n/n: upper 1 (to rounding; 0153's pinned function)", abs(hi - 1.0) < 1e-12 and 0 < lo < 1, hi)
check("Wilson with n = 0 is undefined", e4.ev.wilson(0, 0) == (None, None))


# the outcome rule at its boundaries
def fake(upper, lower, auto=10):
    return {"auto": auto, "E_possible_wilson": (0.0, upper), "A_wilson": (lower, 1.0)}


check("upper exactly 0.20 and lower exactly 0.25: target met", e4.outcome(fake(0.20, 0.25)) == "routing target met")
check("upper just above 0.20: not demonstrated", e4.outcome(fake(math.nextafter(0.20, 1), 0.25)) == "not demonstrated")
check("lower just below 0.25: not demonstrated", e4.outcome(fake(0.20, math.nextafter(0.25, 0))) == "not demonstrated")
check("zero auto-routes: not demonstrated", e4.outcome({"auto": 0, "E_possible_wilson": (None, None), "A_wilson": (0.0, 0.1)}) == "not demonstrated")
mz = e4.measures({1: None, 2: None}, {1: "bug", 2: "bug"}, {1: "bug", 2: "bug"}, 0.1)
check("zero auto-routes leave E undefined", mz["E_possible"] is None and mz["E_possible_wilson"] == (None, None))
check("the frozen threshold and selection are 0153's", e4.THRESHOLD == 0.09553107383376902 and e4.SELECTED["selected"] == "C")
cents = e4.frozen_centroids()
check("the frozen centroids decode as 4 queues x 384 f32", sorted(cents) == sorted(e4.ev.QUEUES) and all(len(v) == 384 for v in cents.values()))
print("all 0154 measure fixtures behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
