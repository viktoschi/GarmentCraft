#!/usr/bin/env bash
# Tasks for agents working in parallel: list, check for overlaps, claim,
# release. Workflow and rules are in CONTRIBUTING.md.
#
# Needs only gh (logged in). Runs on Linux, macOS (bash 3.2 too) and Git Bash.
# Another repository: KLON3D_REPO=owner/name tools/task.sh …
#
# Ownership is not the assignee (all agents share one account) but the
# comments: the first "Claimed" since the last release by the owner wins.
set -euo pipefail

REPO="${KLON3D_REPO:-viktoschi/GarmentCraft}"

usage() {
  cat <<'EOF'
tools/task.sh <command> …

  list                              open tasks with labels and owner
  scope   <nr>                      files from the "### Files" section
  check   <nr>                      scope against claimed tasks and open PRs
  claim   <nr> <session>            check, claim comment, label in-progress
  release <nr> <session> [note]     back to ready, with a handoff note
  done    <nr> <session> <pr>       to review, link the PR
  labels                            create or update the labels

Session name: <tool>-<keyword>-<MMDD>, e.g. claude-seams-1031.
EOF
}

fail() { echo "task.sh: $*" >&2; exit 2; }

number() { [[ "$1" =~ ^[0-9]+$ ]] || fail "not an issue number: $1"; }

# Scope of a task: lines below "### Files" up to the next heading, without
# code fences, bullets and anything after the path.
scope() {
  local body
  body="$(gh issue view "$1" -R "$REPO" --json body --jq .body)" || fail "#$1 not readable (gh)"
  printf '%s\n' "$body" | tr -d '\r' | awk '
    /^###[[:space:]]/ { inside = ($0 ~ /^###[[:space:]]+Files/); next }
    !inside { next }
    /^[[:space:]]*```/ || /_No response_/ { next }
    {
      sub(/^[[:space:]]+/, ""); sub(/^[-*][[:space:]]+/, ""); gsub(/`/, "")
      sub(/[[:space:]].*$/, ""); sub(/^\.\//, "")
      if ($0 != "") print
    }'
}

# Equal, or one lies inside the other's folder.
overlaps() {
  local a="${1%/}" b="${2%/}"
  [[ "$a" == "$b" || "$a" == "$b/"* || "$b" == "$a/"* ]]
}

# Owner according to the comments (empty when free).
owner() {
  gh issue view "$1" -R "$REPO" --json comments --jq '.comments[].body | split("\n")[0]' |
    tr -d '\r' | awk '
      function name(s) { sub(/^[^`]*`/, "", s); sub(/`.*$/, "", s); return s }
      /^\*\*Claimed\*\* by `/          { if (who == "") who = name($0); next }
      /^\*\*(Released|Done)\*\* by `/ { if (name($0) == who) who = ""; next }
      END { print who }'
}

# set_labels <nr> <add> <remove>… – removes only labels that are present.
set_labels() {
  local nr="$1" add="$2"; shift 2
  local present args=(--add-label "$add") rm
  present=$'\n'"$(gh issue view "$nr" -R "$REPO" --json labels --jq '.labels[].name')"$'\n'
  for rm in "$@"; do
    [[ "$present" == *$'\n'"$rm"$'\n'* ]] && args+=(--remove-label "$rm")
  done
  gh issue edit "$nr" -R "$REPO" "${args[@]}" >/dev/null
}

comment() { gh issue comment "$1" -R "$REPO" --body-file - >/dev/null; }

is_open() {
  [[ "$(gh issue view "$1" -R "$REPO" --json state --jq .state)" == OPEN ]] || fail "#$1 is not open"
}

list() {
  local n=0 nr labels title who
  while IFS=$'\t' read -r nr labels title; do
    n=$((n + 1))
    who="$(owner "$nr")"; [[ -n "$who" ]] && who="  ← $who"
    printf '#%-4s %-30s %s%s\n' "$nr" "$labels" "$title" "$who"
  done < <(gh issue list -R "$REPO" --state open --label task --limit 200 --json number,title,labels \
    --jq '.[] | [(.number|tostring), ([.labels[].name | select(. != "task")] | join(",")), .title] | @tsv')
  ((n > 0)) || echo "no open tasks in $REPO"
}

check() {
  # Every gh call sits in an assignment: if it fails, set -e stops the script
  # instead of an empty answer passing as "no overlap".
  local nr="$1" conflict=0 line other who p pr e text tasks prs
  local own=()
  text="$(scope "$nr")"
  while IFS= read -r line; do [[ -n "$line" ]] && own+=("$line"); done <<<"$text"
  ((${#own[@]} > 0)) || fail "#$nr has no scope under ### Files"

  # Claimed tasks. The comments decide, not the in-progress label: the label
  # search lags behind changes by a few seconds.
  tasks="$(gh issue list -R "$REPO" --state open --label task --limit 200 --json number --jq '.[].number')"
  while IFS= read -r other; do
    [[ -z "$other" || "$other" == "$nr" ]] && continue
    who="$(owner "$other")"
    [[ -n "$who" ]] || continue
    text="$(scope "$other")"
    while IFS= read -r p; do
      [[ -n "$p" ]] || continue
      for e in "${own[@]}"; do
        if overlaps "$e" "$p"; then
          echo "overlap with #$other ($who): $e ↔ $p"; conflict=1
        fi
      done
    done <<<"$text"
  done <<<"$tasks"

  # Open PRs, except those that close this task themselves.
  prs="$(gh pr list -R "$REPO" --state open --limit 200 --json number,body,files --jq "
    .[] | select((.body // \"\") | test(\"(Closes|Fixes|Resolves) #$nr\\\\b\"; \"i\") | not)
        | .number as \$n | .files[] | \"\(\$n)\t\(.path)\"")"
  while IFS=$'\t' read -r pr p; do
    [[ -z "$pr" ]] && continue
    for e in "${own[@]}"; do
      if overlaps "$e" "$p"; then
        echo "overlap with PR #$pr: $e ↔ $p"; conflict=1
      fi
    done
  done <<<"$prs"

  if ((conflict == 0)); then
    echo "#$nr: no overlap (${#own[@]} paths)"
  fi
  return "$conflict"
}

claim() {
  local nr="$1" session="$2" who base
  [[ "$session" =~ ^[A-Za-z0-9._-]+$ ]] || fail "session name may only contain letters, digits, . _ - (e.g. claude-seams-1031)"
  is_open "$nr"
  who="$(owner "$nr")"
  [[ -z "$who" ]] || fail "#$nr already belongs to $who"
  check "$nr" || fail "#$nr not claimed – resolve the overlap first"

  base="$(git rev-parse --short origin/main 2>/dev/null || echo '?')"
  { printf '**Claimed** by `%s`\n\n' "$session"
    printf 'Base: %s\nBranch: %s-<keyword>\n\nScope:\n' "$base" "$nr"
    scope "$nr" | sed 's/^/- /'
  } | comment "$nr"
  set_labels "$nr" in-progress ready review

  # Claimed at the same time? Then the earlier comment wins.
  who="$(owner "$nr")"
  if [[ "$who" != "$session" ]]; then
    printf '**Withdrawn** by `%s` – `%s` was faster.\n' "$session" "$who" | comment "$nr"
    fail "#$nr was claimed at the same time by $who – let it go"
  fi
  echo "#$nr now belongs to $session."
  echo "Next: git worktree add ../klon3d-wt/$nr-<keyword> -b $nr-<keyword> origin/main"
}

hand_back() { # hand_back <nr> <session> <word> <label> <text>
  local nr="$1" session="$2" word="$3" label="$4" text="$5" who
  is_open "$nr"
  who="$(owner "$nr")"
  [[ "$who" == "$session" ]] || fail "#$nr does not belong to $session but to ${who:-nobody}"
  printf '**%s** by `%s`\n\n%s\n' "$word" "$session" "$text" | comment "$nr"
  set_labels "$nr" "$label" in-progress
}

labels() {
  local name color text
  while IFS='|' read -r name color text; do
    gh label create "$name" -R "$REPO" --color "$color" --description "$text" --force >/dev/null
    echo "label $name"
  done <<'EOF'
task|1d76db|Task created from the template (CONTRIBUTING.md)
ready|0e8a16|Free, can be claimed
in-progress|fbca04|Claimed; the owner is in the claim comment
review|5319e7|Pull request open, waiting for review
blocked|b60205|Waiting for another task or a decision
needs-clo-data|c5def5|Can only be checked on the main machine
shared-file|d93f0b|Touches a shared file (AGENTS.md)
bug|d73a4a|Bug report, not yet a task
EOF
}

cmd="${1:-help}"
(($# > 0)) && shift
case "$cmd" in
  list)    list ;;
  scope)   (($# == 1)) || fail "scope <nr>"; number "$1"; scope "$1" ;;
  check)   (($# == 1)) || fail "check <nr>"; number "$1"; check "$1" ;;
  claim)   (($# == 2)) || fail "claim <nr> <session>"; number "$1"; claim "$1" "$2" ;;
  release) (($# >= 2)) || fail "release <nr> <session> [note]"; number "$1"
           hand_back "$1" "$2" Released ready "${3:-no note}"
           echo "#$1 is free again." ;;
  done)    (($# == 3)) || fail "done <nr> <session> <pr>"; number "$1"; number "$3"
           hand_back "$1" "$2" Done review "PR: #$3"
           echo "#$1 is waiting for review (PR #$3)." ;;
  labels)  labels ;;
  help|-h|--help) usage ;;
  *)       usage >&2; exit 2 ;;
esac
