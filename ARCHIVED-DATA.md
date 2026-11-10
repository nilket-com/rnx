# Archived data

ADR 0240 moves historical measurement artifacts out of git. They are not
regenerated or replaced with new measurements. Historical plans keep their
original citations; resolve them by repository, original path and Git blob
through the archive index. The index and bundle checksums are pinned in
`ci/data_archive.json`.

The archive is currently local to the maintainer's host, not hosted with this
repository. Ask the maintainer for the `0240-data` archive when original
measurement evidence is needed. A fresh clone can run the supported tests
without this archive; historical measurement replays require it.

Given a copy of the archive:

```sh
python3 tools/archive_data.py --archive "$RNX_DATA_ARCHIVE" \
  --repository rnx --path probes/0125/frozen-0.55.2.json
```

The tool verifies the index and file checksum and prints the original
revision/path/blob and local location. `files/rnx/` and `files/rnx-bench/`
contain complete tracked snapshots, including historical scripts and their
relative inputs. For a full historical replay, clone the corresponding
bundle into a separate directory and check out the revision in the catalog;
do not overlay archived files on the active checkout. Original measurements
remain associated with their original source/binary/receipt hashes.

Current fixtures, source policy, licenses and unresolved inputs stay in git.
Historical scorer/test clusters are retained until their dependencies can be
retired together. A script left under `probes/` is not a claim that its old
measurement inputs remain in the active checkout: use the archived snapshot
for historical replays. Active inventory generation and required fixtures
are preserved.
