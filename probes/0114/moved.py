#!/usr/bin/env python3
"""Record 0114: nothing was lost, added or duplicated in the move.

Splits the base `main.rs` and every file of the new tree into top-level items
(a small Rust lexer: comments, strings, raw strings, chars and lifetimes),
normalizes each item (whitespace, `pub(crate)`, rustfmt's trailing commas),
and compares the two multisets. `use`/`mod` items are ignored. The only
differences allowed are the ones this record's plan names, each checked
exactly:

  1. the oracle harness raw strings, now `include_str!` files: they are
     inlined back before comparing;
  2. the 32 nested self-test calls, now the ordered `SELF_TESTS` list:
     removed from the base items before comparing, and the list must name
     all 34 in the base call order;
  3. `main`, cut into `main`, `pipeline::generate` and `output::write`: the
     base body must equal the three bodies joined, up to the listed seam
     lines;
  4. the `#[test]` wrappers: one per self-test, calling only that test.

  moved.py <base main.rs> <new src dir>
  moved.py --self-test
"""
import collections, glob, os, re, sys


def lex_items(src):
    """Top-level items as source slices (leading comments/attributes kept)."""
    items, i, n, depth, start = [], 0, len(src), 0, 0
    while i < n:
        c = src[i]
        if src.startswith('//', i):
            i = src.index('\n', i) if '\n' in src[i:] else n
            continue
        if src.startswith('/*', i):
            i = src.index('*/', i) + 2
            continue
        m = re.match(r'b?r(#*)"', src[i:])
        if m and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == '_')):
            close = '"' + m.group(1)
            i = src.index(close, i + len(m.group(0))) + len(close)
            continue
        if c == '"':
            i += 1
            while src[i] != '"':
                i += 2 if src[i] == '\\' else 1
            i += 1
            continue
        if c == "'":
            m = re.match(r"'(\\.[^']*|[^\\'])'", src[i:])
            i += len(m.group(0)) if m else 1   # a char literal, else a lifetime
            continue
        if c in '{([':
            depth += 1
        elif c in '})]':
            depth -= 1
            if depth == 0 and c == '}':
                j = i + 1
                if src[j:j + 1] == ';':
                    j += 1
                items.append(src[start:j]); start = j
                i = j
                continue
        elif c == ';' and depth == 0:
            items.append(src[start:i + 1]); start = i + 1
        i += 1
    tail = src[start:].strip()
    assert depth == 0 and not re.sub(r'//[^\n]*', '', tail).strip(), tail[:200]
    return items


def norm(item):
    t = re.sub(r'\s+', '', item)
    t = t.replace('pub(crate)', '')
    return re.sub(r',([)}\]])', r'\1', t)


def head(item):
    body = re.sub(r'(?m)^\s*(//[^\n]*|#\[[^\n]*\])\s*$', '', item).strip()
    m = re.match(r'(?:pub\(crate\) )?(fn|struct|enum|const|static|impl(?:<[^>]*>)?|mod|use)\s+([A-Za-z_<\'][^\s({:=;]*)', body)
    return (m.group(1), m.group(2)) if m else ('?', body[:40])


SELF_TEST_CALL = re.compile(r'\n\t[a-z_]+_self_test\(\);')


def inline_harness(item, src_dir):
    def sub(m):
        text = open(os.path.join(src_dir, 'oracle', m.group(1))).read()
        assert '"#' not in text
        return 'r#"' + text + '"#'
    # `concat!(include_str!(f), "\\n")` keeps a final blank line out of the file
    item = re.sub(r'concat!\(include_str!\("([a-z_]+\.rs\.in)"\),\s*"\\n"\)',
                  lambda m: 'include_str!("' + m.group(1) + '")' + '\x00NL', item)
    item = re.sub(r'include_str!\("([a-z_]+\.rs\.in)"\)', sub, item)
    return item.replace('"#\x00NL', '\n"#')


def compare(base_src, src_dir):
    base = [x for x in lex_items(base_src) if head(x)[0] not in ('use', 'mod')]
    files = sorted(glob.glob(os.path.join(src_dir, '**', '*.rs'), recursive=True))
    files = [f for f in files if os.path.basename(f) not in ('model.rs', 'ty.rs')]
    new, by_name = [], {}
    for f in files:
        for x in lex_items(open(f).read()):
            k, name = head(x)
            if k in ('use', 'mod'):
                if k == 'mod' and re.search(r'(?m)^mod tests \{', x):   # the `#[cfg(test)]` wrappers
                    new.append(('tests', f, x))
                continue
            new.append((name, f, x))
            by_name.setdefault(name, []).append((f, x))
    report = []
    # (2) base self-tests lose their nested tail calls; record the call order
    base_order = ['wrapper']
    base_n = []
    for x in base:
        calls = [c.strip()[:-len('_self_test();')] for c in SELF_TEST_CALL.findall(x)]
        if head(x)[1] == 'main':
            calls = [c for c in calls]
            base_main = x
            continue
        base_order += [c for c in calls if c != 'policy']
        base_n.append(norm(SELF_TEST_CALL.sub('', x)))
    base_order.append('policy')   # main called wrapper, then policy
    # (1) harness inlined; the moved main pieces and tests handled below
    moved = collections.Counter()
    tests, pieces = [], {}
    for name, f, x in new:
        if name == 'tests':
            tests.append((f, x)); continue
        if name in ('main', 'generate', 'write', 'SELF_TESTS'):
            pieces[name] = x; continue
        n = norm(inline_harness(x, src_dir))
        if n:
            moved[n] += 1
        else:
            report.append(f'{f}: an empty item: {x!r}'[:200])
    want = collections.Counter(base_n)
    lost, added = want - moved, moved - want
    for x in lost:
        report.append(f'lost from the base: {x[:120]}')
    for x in added:
        report.append(f'added in the new tree: {x[:120]}')
    # (2) the list names every self-test, in the base order
    listed = re.findall(r'::([a-z_]+)_self_test,', pieces['SELF_TESTS'])
    if listed != base_order:
        report.append(f'SELF_TESTS order {listed} != base call order {base_order}')
    # (4) one wrapper per self-test, calling only that test
    wrapped = []
    for f, x in tests:
        for name, call in re.findall(r'#\[test\]\s*fn ([a-z_]+)\(\) \{\s*super::([a-z_]+)_self_test\(\);\s*\}', x):
            if name != call:
                report.append(f'wrapper {name} calls {call}')
            wrapped.append(name)
        stripped = re.sub(r'#\[test\]\s*fn [a-z_]+\(\) \{\s*super::[a-z_]+_self_test\(\);\s*\}', '', x)
        if norm(stripped) != '#[cfg(test)]modtests{}':
            report.append(f'{f}: tests module holds more than wrappers')
    if sorted(wrapped) != sorted(base_order):
        report.append(f'wrappers {sorted(wrapped)} != self-tests {sorted(base_order)}')
    # (3) main = main + generate + write, up to the seam lines
    base_body = norm(base_main)
    joined = norm(pieces['main']) + norm(pieces['generate']) + norm(pieces['write'])
    seam_base, seam_new = main_seams(base_body, joined)
    if seam_base != SEAM_BASE or seam_new != SEAM_NEW:
        report.append('main seams differ:\n  base-only ' + repr(seam_base) + '\n  new-only  ' + '\n    '.join(map(repr, seam_new)))
    return report, len(base), sum(want.values()), len(wrapped)


def main_seams(a, b):
    """The normalized statements only one side has, in order (a small LCS)."""
    sa = [s for s in re.split(r'(?<=[;{}])', a) if s]
    sb = [s for s in re.split(r'(?<=[;{}])', b) if s]
    import difflib
    only_a, only_b = [], []
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, sa, sb, autojunk=False).get_opcodes():
        if tag in ('replace', 'delete'):
            only_a += sa[i1:i2]
        if tag in ('replace', 'insert'):
            only_b += sb[j1:j2]
    return only_a, only_b


# The seam lines of (3), reviewed: the base's self-test calls and the end of
# `main`'s body; the new signatures, the list loop, the two calls that
# replace the moved bodies, and the returned tuple.
SEAM_BASE = ['wrapper_self_test();', 'policy_self_test();']
SEAM_NEW = [
    'fortestinSELF_TESTS{', 'test();', '}',
    'let(out,census)=pipeline::generate(&world,&inv,&release,&buckets);',
    'emit::output::write(out,world,census,&inv,&release,&release_path,&release_digest,&buckets,&args,check);',
    '}',
    '///Everycallable,inthefixedorder,throughthefamilyrules;',
    'thenthe///deferredoperatorand`INDEX_GET`groupsandthecensusdispositions.fngenerate(world:&World,inv:&Inventory,release:&Release,buckets:&Vec<&str>)->(Emitted,Vec<PairRecord>){',
    '(out,census)}',
    '#[allow(clippy::too_many_arguments)]fnwrite(mutout:Emitted,world:World,census:Vec<PairRecord>,inv:&Inventory,release:&Release,release_path:&Path,release_digest:&str,buckets:&Vec<&str>,args:&[String],check:bool){',
]


def self_test():
    src = 'fn a() { let s = "}"; let c = \'}\'; let r = r#"fn x() {"#; }\n// c\nstruct B<\'a>(&\'a str);\nuse x::{y, z};\nuse w;\n'
    its = lex_items(src)
    assert len(its) == 4 and head(its[0]) == ('fn', 'a') and head(its[1])[0] == 'struct', its
    assert norm('pub(crate) fn f(\n\ta,\n) {}') == norm('fn f(a) {}')
    print('moved self-test: ok')


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-test']:
        self_test(); sys.exit(0)
    base, src = sys.argv[1:3]
    report, nb, nm, nw = compare(open(base).read(), src)
    print(f'base items: {nb}; compared after the named seams: {nm}; test wrappers: {nw}')
    for r in report:
        print(r)
    print('moved: ' + ('ok' if not report else f'{len(report)} differences'))
    sys.exit(1 if report else 0)
