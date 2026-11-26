#!/bin/sh
# What every agent has in flight, read from the remote alone: each branch on the remote that
# holds commits `main` lacks, newest first, with the status and next step from its task note
# (docs/agents/sync.md). Needs only git, so it runs on the workstation and in a cloud sandbox.
#
#   scripts/agent-tasks.sh             unmerged branches and their task notes
#   scripts/agent-tasks.sh --merged    branches already contained in main (safe to delete)
#   scripts/agent-tasks.sh --no-fetch  use what was last fetched
#
# REMOTE and BASE override the remote and the base branch (origin, main).
set -eu

remote=${REMOTE:-origin}
base=${BASE:-main}
notes=.scratch/project-b/handoffs
mode=open
fetch=1
for arg in "$@"; do
    case $arg in
        --merged) mode=merged ;;
        --no-fetch) fetch=0 ;;
        -h | --help)
            sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

# Note paths are given from the top of the checkout, wherever this is run from.
cd "$(git rev-parse --show-toplevel)"

if [ "$fetch" = 1 ]; then
    git fetch --prune --quiet "$remote" || echo "fetch failed; showing what was last fetched" >&2
fi
git rev-parse --verify --quiet "$remote/$base" >/dev/null || {
    echo "no $remote/$base here" >&2
    exit 1
}

# The first 20 lines of a note on a branch. The blob is looked up by id rather than as
# `branch:path`, which Git Bash on Windows rewrites as a path list.
header() {
    blob=$(git ls-tree --full-tree "$1" -- "$2" | cut -f1 | cut -d' ' -f3)
    [ -n "$blob" ] && git cat-file blob "$blob" | sed -n 1,20p
}

# The first line of that name in a header, without the label.
field() {
    printf '%s\n' "$1" | sed -n "s/^$2:[[:space:]]*//p" | sed -n 1p
}

tab=$(printf '\t')
total=0
git for-each-ref --sort=-committerdate \
    --format="%(refname:short)$tab%(committerdate:short)$tab%(authorname)$tab%(subject)" \
    "refs/remotes/$remote" |
    {
        while IFS=$tab read -r ref date author subject; do
            case $ref in "$remote" | "$remote/HEAD" | "$remote/$base") continue ;; esac
            ahead=$(git rev-list --count "$remote/$base..$ref")
            if [ "$mode" = merged ]; then
                [ "$ahead" = 0 ] || continue
                total=$((total + 1))
                echo "${ref#"$remote"/}  ($date)"
                continue
            fi
            [ "$ahead" = 0 ] && continue
            total=$((total + 1))
            behind=$(git rev-list --count "$ref..$remote/$base")
            # The task note is the note the branch touches whose header names this branch;
            # failing that, the last by name of the notes it touches.
            name=${ref#"$remote"/}
            note=
            head=
            for candidate in $(git diff --name-only "$remote/$base...$ref" -- "$notes" 2>/dev/null | sort); do
                note=$candidate
                head=$(header "$ref" "$candidate" || true)
                [ "$(field "$head" Branch)" = "$name" ] && break
            done
            echo "$name"
            echo "  last push  $date  $author: $subject"
            echo "  commits    $ahead ahead of $base, $behind behind"
            if [ -n "$note" ]; then
                status=$(field "$head" Status)
                owner=$(field "$head" Owner)
                next=$(field "$head" Next)
                echo "  note       $note"
                echo "  status     ${status:-not stated}${owner:+  ($owner)}"
                [ -n "$next" ] && echo "  next       $next"
            else
                echo "  note       none on this branch"
            fi
            echo
        done
        if [ "$mode" = merged ]; then
            echo "$total branches on $remote are already contained in $base"
        else
            echo "$total unmerged branches on $remote"
        fi
    }
