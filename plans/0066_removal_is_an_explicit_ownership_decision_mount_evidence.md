# 0066 gate 1: stopped at the unprivileged mount prerequisite

Status: **stopped for review**, as gate 1 requires when its mount fixture cannot
run unprivileged. No product deletion primitive, candidate port or removal command
has been written. No live user store, project, session or kernel was touched.

Source baseline is the signed plan commit rnx eff151b; bench baseline a0dd0bf.
The driver and complete stop evidence are committed at rnx-bench 1f0b85d.
The reproducible driver is probes/removal-preflight/check.py and its guarded child
is namespace.py. preflight.json retains exact commands, output, statuses, tool and
kernel observations and script hashes. product.patch is empty against eff151b.

| Control | Exit | Observation |
|---|---:|---|
| Ordinary Python child | 0 | Execution works |
| User namespace without mapping | 0 | Namespace creation alone works |
| Mount namespace without user mapping | 1 | unshare: Operation not permitted |
| Current-user mapping plus mount namespace | 1 | write to /proc/self/uid_map: Operation not permitted |
| Root mapping plus actual nested/bind fixture | 1 | write to /proc/self/uid_map: Operation not permitted |

The caller is unprivileged (UID 1003). The final child never reaches its entry
marker, so neither tmpfs nor bind mount is created. The parent mount namespace and
outside sentinel are unchanged, children are waited and the private temporary
tree is removed. No privilege escalation or alternative privileged mount was used.
The results locate the observable refusal; they do not attribute it to a specific
host security configuration or establish that Linux lacks the capability.

The driver records a successful control run with a STOP decision and
`gate1_passed: false`. This is not evidence for the deletion primitive's mount
refusals. No claims are made for live old-consumer preservation, manifest
annotations, busy/selected/self-target refusal, rename visibility, deletion,
hardlinks, symlinks or interrupted resume. Those remain gate-1 prototype work once
the prerequisite can be exercised in a suitable unprivileged environment.

The plan's stop is followed rather than replacing a real mount with a synthetic
metadata result or dropping the bind-mount case. The next review decision is how
to supply that environment. Gates 1–4 remain open; removal is not enabled.
