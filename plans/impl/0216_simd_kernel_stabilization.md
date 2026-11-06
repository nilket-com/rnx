# Implementation plan: ADR 0216

Begin on published 99790566 with ADR 0215 preserved in a verified bundle,
including its signed plan/implementation and separate uncommitted parser fix.
Retain its evidence index and explicitly invalidated parent PASS receipt.

Implement explicit eight-lane main loops in the four protected kernels, keeping
one arithmetic definition and existing overflow accumulation. Follow with a
four-lane remainder and scalar tail. Add boundary, mask and arithmetic controls;
run format, Clippy and focused tests, then targeted Miri.

Prepare signed paired production builds and a recorded inert-kernel composition
control. Compare loop and tail assembly shapes with symbol/alias resolution and
negative controls. Collect the protected small dynamic matrix once, retaining
per-case durable receipts and source/binary/recipe identity. Require both child
exit success and a matching PASS receipt in every controller. No performance
capture retry to select favorable results.

Send any shape or dynamic regression for review before further work. An all-PASS
packet includes protected-path attribution, both-composition evidence and
proportional host validation. Publication and later ADR 0215 restack follow the
reviewed sequence; the parked implementation is not published with this record.
