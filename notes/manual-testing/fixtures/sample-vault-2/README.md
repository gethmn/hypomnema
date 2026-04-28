# Sample fixture vault — kitchen edition

A small second vault used by Hypomnema's manual-testing runbook to
exercise multi-vault behavior end-to-end (cross-vault search,
`--vaults` filtering, partial-results diagnostics, vault-management
operations). Topic and file names are deliberately disjoint from the
companion [`sample-vault/`](../sample-vault/) so search results never
collide between the two vaults.

The expected-results contract for queries against this vault lives in
[`../README.md`](../README.md).

Do not edit these files casually; the runbook claims specific match
counts and top-N rankings against this exact content.
