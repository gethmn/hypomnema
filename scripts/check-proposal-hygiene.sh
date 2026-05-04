#!/usr/bin/env bash
# Flags shipped proposals/intakes that were not moved to notes/proposals/archive/.
#
# For each notes/roadmap/archive/roadmap-N.md, parses the **Intakes**: block
# (markdown links of the form [`notes/proposals/<file>.md`](...)). For each
# linked intake-<slug>.md, also checks the matching <slug>.md and
# <slug>-stories.md. Anything still living in notes/proposals/ (rather than
# notes/proposals/archive/) is reported. Exits non-zero if any orphans found.
#
# Owner: coordinator at round close (see notes/playbook/coordinator.md
# § "Step Boundary" item 3a).

set -Eeuo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

archive_dir="notes/roadmap/archive"
proposals_dir="notes/proposals"
proposals_archive="$proposals_dir/archive"

orphans=()

shopt -s nullglob
for roadmap in "$archive_dir"/roadmap-*.md; do
  # Extract the **Intakes**: block (until next blank line or next ** heading).
  paths=$(awk '
    /^\*\*Intakes\*\*:/ { in_block = 1; next }
    in_block && /^$/ { in_block = 0 }
    in_block && /^\*\*/ { in_block = 0 }
    in_block { print }
  ' "$roadmap" | { grep -oE '`notes/proposals/[^`]+`' || true; } | tr -d '`' | sort -u)

  [ -z "$paths" ] && continue

  while IFS= read -r p; do
    [ -z "$p" ] && continue
    base=$(basename "$p")

    # Derive companion files for intake-<slug>.md
    candidates=("$base")
    if [[ "$base" == intake-*.md ]]; then
      slug=${base#intake-}
      slug=${slug%.md}
      candidates+=("$slug.md" "$slug-stories.md")
    fi

    for f in "${candidates[@]}"; do
      if [ -f "$proposals_dir/$f" ]; then
        orphans+=("$proposals_dir/$f (referenced by $roadmap)")
      fi
    done
  done <<<"$paths"
done

if [ ${#orphans[@]} -eq 0 ]; then
  echo "OK: no orphaned proposals."
  exit 0
fi

echo "Orphaned proposals (shipped but still in notes/proposals/):"
printf '  %s\n' "${orphans[@]}"
echo
echo "Move these to notes/proposals/archive/ and re-run."
exit 1
