# Record 0153: annotation rules (frozen before any holdout text is read)

One primary label per ticket, from the ticket's number, title and body alone
(annotator_view.py's output), one of: bug, feature, question, documentation,
performance.

The five classification rules, verbatim from probes/0139/frozen/rules.md
(SHA-256 8aa28e2af45288406023237baa3a193e30bf816450e9181c0f124be876a4c928):

1. A ticket reporting that something that should work doesn't is **bug**, even if slow or poorly documented.
2. A slowness, memory or efficiency complaint without incorrect behaviour is **performance**.
3. A report that the docs are missing or wrong, with no code defect claimed, is **documentation**.
4. A request for new behaviour is **feature**; a request for help using existing behaviour is **question**.
5. If two still fit, the label matching the **title's** main ask wins.

Annotation file format: a header line `number\tlabel`, then exactly one row
per ticket of the split being annotated (D3-hold: 150 rows; D3-dev: every
D3-dev ticket), in the split file's order (ascending number); each label
exactly one of the five above, lowercase; no blank, missing, extra or
duplicate rows. Annotators work independently, from the view alone, without
maintainer labels, system output or each other's labels; labels are never
revised after any score.
