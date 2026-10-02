"""Record 0149 (review round 1, R1): the first-divergence diagnosis of a
native end-to-end difference, as a function the synthetic controls test.

classify(native_prompt, rnx_prompt, native_ids, rnx_ids, logits_at, tol):
- "identical": the same prompt ids and generated ids;
- ("i", None): a proven tokenizer or template difference (the prompt ids
  differ);
- ("ii", k): an explained numeric argmax difference on the same prefix: the
  prompts and ids before k are equal, and at k HF's logits (teacher-forced
  on that shared prefix, `logits_at(k)`) have rnx's token within 2 × tol of
  their maximum; later tokens are a different context and not compared;
- ("iii", k): anything else, an unexplained decoder or model difference.
A length difference with an equal shared prefix is classified at the first
missing position: rnx's token there (or its absence) is judged the same way."""


def classify(native_prompt, rnx_prompt, native_ids, rnx_ids, logits_at, tol):
    if list(native_prompt) != list(rnx_prompt):
        return ("i", None)
    if list(native_ids) == list(rnx_ids):
        return ("identical", None)
    k = next((j for j in range(min(len(native_ids), len(rnx_ids))) if native_ids[j] != rnx_ids[j]),
             min(len(native_ids), len(rnx_ids)))
    if k >= len(rnx_ids):
        # rnx stopped earlier with the same prefix: impossible for a greedy
        # decoder with the same stop policy unless its last token differs
        return ("iii", k)
    logits = logits_at(k)
    top = max(logits)
    return ("ii", k) if logits[rnx_ids[k]] >= top - 2 * tol else ("iii", k)


def controls():
    ok = True

    def expect(name, got, want):
        nonlocal ok
        ok &= got == want
        print(f"{'pass' if got == want else 'WRONG'}: {name}: {got}")

    tol = 1e-3
    near = lambda k: [0.0, 5.0, 5.0 - 1e-3]       # rnx's token 2 within 2 x tol
    clear = lambda k: [0.0, 5.0, 4.0]             # rnx's token 2 a full logit behind
    expect("identical", classify([1, 2], [1, 2], [5, 6], [5, 6], near, tol), ("identical", None))
    expect("a prompt-id difference", classify([1, 2], [1, 3], [5], [5], near, tol), ("i", None))
    expect("a first divergence after equal prefixes, a near tie", classify([1], [1], [0, 1, 1], [0, 2, 0], near, tol), ("ii", 1))
    expect("the same with a clear margin", classify([1], [1], [0, 1, 1], [0, 2, 0], clear, tol), ("iii", 1))
    expect("rnx shorter on an equal prefix", classify([1], [1], [0, 1, 1], [0, 1], near, tol), ("iii", 2))
    return ok


if __name__ == "__main__":
    import sys
    good = controls()
    print("all diagnosis controls behave" if good else "CONTROLS FAILED")
    sys.exit(0 if good else 1)
