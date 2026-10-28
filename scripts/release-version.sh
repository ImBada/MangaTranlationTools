#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release-version.sh [patch|minor|major|x.y.z] [--dry-run]

Examples:
  scripts/release-version.sh
  scripts/release-version.sh patch
  scripts/release-version.sh minor
  scripts/release-version.sh 0.9.14
  scripts/release-version.sh --dry-run

Creates a version commit, annotated vX.Y.Z tag, and pushes both to origin; the tag starts
the Release workflow. Only Cargo.toml and Cargo.lock are included in the release commit.
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

run() {
  echo "> $*"
  "$@"
}

dry_run=false
bump="patch"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      dry_run=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    patch|minor|major|[0-9]*.[0-9]*.[0-9]*)
      bump="$1"
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
  shift
done

command -v git >/dev/null 2>&1 || die "git is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

branch="$(git symbolic-ref --quiet --short HEAD)" || die "not on a branch"
remote="${REMOTE:-origin}"
remote_ref="$remote/$branch"

# The workspace version: the first `version = "..."` line of Cargo.toml ([workspace.package]).
current_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[[ "$current_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "invalid workspace version: $current_version"

if [[ "$bump" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  next_version="$bump"
else
  IFS=. read -r major minor patch <<<"$current_version"
  case "$bump" in
    major) next_version="$((major + 1)).0.0" ;;
    minor) next_version="$major.$((minor + 1)).0" ;;
    patch) next_version="$major.$minor.$((patch + 1))" ;;
  esac
fi

tag="v$next_version"

if ! git diff --quiet -- Cargo.toml Cargo.lock; then
  die "Cargo.toml or Cargo.lock already has unstaged changes"
fi

if ! git diff --cached --quiet -- Cargo.toml Cargo.lock; then
  die "Cargo.toml or Cargo.lock already has staged changes"
fi

run git fetch "$remote" --tags

if git rev-parse --verify --quiet "$remote_ref" >/dev/null; then
  merge_base="$(git merge-base HEAD "$remote_ref")"
  remote_sha="$(git rev-parse "$remote_ref")"
  head_sha="$(git rev-parse HEAD)"
  if [[ "$merge_base" != "$remote_sha" && "$head_sha" != "$remote_sha" ]]; then
    die "local $branch has diverged from $remote_ref; rebase or merge before releasing"
  fi
  if [[ "$merge_base" != "$remote_sha" ]]; then
    die "local $branch is behind $remote_ref; pull before releasing"
  fi
fi

if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
  die "local tag already exists: $tag"
fi

if git ls-remote --exit-code --tags "$remote" "$tag" >/dev/null 2>&1; then
  die "remote tag already exists: $tag"
fi

echo "Current version: $current_version"
echo "Next version:    $next_version"
echo "Tag:             $tag"
echo "Branch:          $branch"
echo "Remote:          $remote"

if [[ "$dry_run" == true ]]; then
  echo "Dry run only. No files changed, committed, tagged, or pushed."
  exit 0
fi

# Only the first match: the workspace version, not dependency versions.
run perl -0pi -e "s/^version = \"[^\"]*\"/version = \"$next_version\"/m" Cargo.toml
# Updates the workspace crates' entries in Cargo.lock without touching dependencies.
run cargo update --workspace --offline
run git add Cargo.toml Cargo.lock
run git commit -m "Bump version to $next_version"
run git tag -a "$tag" -m "$tag"
run git push "$remote" "$branch"
run git push "$remote" "$tag"
