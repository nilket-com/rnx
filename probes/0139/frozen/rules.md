# Record 0139: annotation rules (frozen before any score)

One primary label per ticket, from the ticket's title and body alone, one of:
bug, feature, question, documentation, performance.

1. A ticket reporting that something that should work doesn't is **bug**, even if slow or poorly documented.
2. A slowness, memory or efficiency complaint without incorrect behaviour is **performance**.
3. A report that the docs are missing or wrong, with no code defect claimed, is **documentation**.
4. A request for new behaviour is **feature**; a request for help using existing behaviour is **question**.
5. If two still fit, the label matching the **title's** main ask wins.

Annotation file format (`annotations-<annotator>.tsv`): a header line `number\tlabel`,
then one line per sample ticket (all 40, in `sample.tsv`'s order), the label
one of the five above. Annotators work independently, without model scores
or each other's labels; disagreements are not resolved after scoring.
