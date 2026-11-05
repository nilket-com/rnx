# ADR 0207: Nominal identity isolation

Status: Accepted

Comparable nominal identities belong to the published global registry. Private
interner tests compare local entry indices and local metadata, not global
semantic equality. Keep interner construction and mutation private to the
registry module so production callers cannot obtain identities from a separate
unpublished namespace.

A concurrent test failure is a defect. Library acceptance requires the original
suite to pass; running a failed test alone cannot establish that result. Remove
the isolated nominal-test retry and enforce this rule with a runner control.
