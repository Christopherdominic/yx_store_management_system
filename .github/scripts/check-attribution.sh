#!/usr/bin/env bash
# Fails if any commit in the given range, or the PR body, credits an AI assistant
# (e.g. "Co-Authored-By: Claude", "Generated with Claude Code").
#
# Usage: check-attribution.sh <rev-range>     e.g. origin/main..HEAD
#        PR_BODY="..." check-attribution.sh <rev-range>
set -euo pipefail

range="${1:?usage: check-attribution.sh <rev-range>}"
pattern='co-authored-by:.*(claude|anthropic)|noreply@anthropic\.com|generated with \[?claude( code)?'
status=0

while read -r sha; do
  [ -z "$sha" ] && continue
  if git log -1 --format='%an <%ae>%n%cn <%ce>%n%B' "$sha" | grep -qiE "$pattern"; then
    echo "::error::Commit $sha contains AI attribution:"
    git log -1 --format='%B' "$sha" | grep -iE "$pattern" | sed 's/^/    /'
    status=1
  fi
done < <(git rev-list "$range")

if [ -n "${PR_BODY:-}" ] && printf '%s' "$PR_BODY" | grep -qiE "$pattern"; then
  echo "::error::Pull request description contains AI attribution:"
  printf '%s\n' "$PR_BODY" | grep -iE "$pattern" | sed 's/^/    /'
  status=1
fi

if [ "$status" -ne 0 ]; then
  echo "Remove the attribution lines (git commit --amend / git rebase) and edit the PR description."
else
  echo "No AI attribution found."
fi
exit "$status"
