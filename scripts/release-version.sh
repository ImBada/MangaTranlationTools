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

Creates a version commit, annotated vX.Y.Z tag, and pushes both to origin.
Only package.json and package-lock.json are included in the release commit.
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
command -v npm >/dev/null 2>&1 || die "npm is required"
command -v node >/dev/null 2>&1 || die "node is required"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

branch="$(git symbolic-ref --quiet --short HEAD)" || die "not on a branch"
remote="${REMOTE:-origin}"
remote_ref="$remote/$branch"

current_version="$(node -p "require('./package.json').version")"

if [[ "$bump" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  next_version="$bump"
else
  next_version="$(
    node -e '
      const current = process.argv[1];
      const bump = process.argv[2];
      const parts = current.split(".").map(Number);
      if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part) || part < 0)) {
        console.error(`Invalid package version: ${current}`);
        process.exit(1);
      }
      if (bump === "major") {
        parts[0] += 1;
        parts[1] = 0;
        parts[2] = 0;
      } else if (bump === "minor") {
        parts[1] += 1;
        parts[2] = 0;
      } else if (bump === "patch") {
        parts[2] += 1;
      } else {
        console.error(`Invalid bump type: ${bump}`);
        process.exit(1);
      }
      console.log(parts.join("."));
    ' "$current_version" "$bump"
  )"
fi

tag="v$next_version"

if ! git diff --quiet -- package.json package-lock.json; then
  die "package.json or package-lock.json already has unstaged changes"
fi

if ! git diff --cached --quiet -- package.json package-lock.json; then
  die "package.json or package-lock.json already has staged changes"
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

run npm version "$next_version" --no-git-tag-version
run git add package.json package-lock.json
run git commit -m "Bump version to $next_version"
run git tag -a "$tag" -m "$tag"
run git push "$remote" "$branch"
run git push "$remote" "$tag"
