# 0194: self-contained cross-repository verification

Authority: Rune V2a and S8, including the live census and enrollment handoff.
Start from rnx 0193. Pin the landed Rune V2a/S8 commit consistently across the
product, project tool, lockfiles and worker version, without changing routing.

The census and enrollment verifier must build and execute both engines live from
versioned source and inputs. Compare every observable error field, preserving the
reviewed treatment of unavailable origins and intentional policy differences.
Retire stored engine observations as test oracles; do not depend on historical
review directories or another checkout. Keep refusal classifications explicit.

Run the real-registration census and enrollment against the final Rune pin.
From an isolated fresh clone on the correctness host, run the documented default
cross-verification command with only pinned toolchains and declared dependency
sources available. Keep raw output in scratch. Rune lands its V2a and S8 work
first; this record does not enable the new engine by default.
