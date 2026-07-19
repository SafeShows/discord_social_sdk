#!/usr/bin/env bash
#
# Build the API documentation locally and publish it to the `gh-pages` branch.
#
# docs.rs cannot build this crate: it has no network access and no copy of the
# Discord Social SDK, and the SDK's header is not redistributable inside the
# published crate. So documentation is built here, where the SDK is present, and
# pushed to GitHub Pages instead.
#
#   ./scripts/publish-docs.sh           build and stage, then print the push command
#   ./scripts/publish-docs.sh --push    build, stage and push
#
# Pushing rewrites the whole `gh-pages` branch, so it is opt-in rather than the
# default.

set -euo pipefail

BRANCH="${DOCS_BRANCH:-gh-pages}"
REMOTE="${DOCS_REMOTE:-origin}"
ROOT_CRATE="discord_social_sdk"
PUSH=0

for arg in "$@"; do
    case "$arg" in
        --push) PUSH=1 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \?//'; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

cd "$(git rev-parse --show-toplevel)"

# The SDK has to be reachable, otherwise the build script cannot generate
# bindings and there is nothing to document.
if [ -z "${DISCORD_SOCIAL_SDK_DIR:-}" ] && [ ! -f discord_social_sdk/include/cdiscord.h ]; then
    echo "error: the Discord Social SDK was not found." >&2
    echo "Place it at ./discord_social_sdk or set DISCORD_SOCIAL_SDK_DIR." >&2
    exit 1
fi

echo "==> Building documentation"
cargo doc --no-deps -p discord_social_sdk -p discord_social_sdk_sys

# `cargo doc` writes no landing page at the root, so visiting the Pages URL would
# otherwise 404. Redirect to the main crate.
cat > target/doc/index.html <<HTML
<!doctype html>
<meta charset="utf-8">
<title>$ROOT_CRATE</title>
<meta http-equiv="refresh" content="0; url=$ROOT_CRATE/index.html">
<a href="$ROOT_CRATE/index.html">Continue to the $ROOT_CRATE documentation</a>
HTML

# Tell GitHub Pages not to run the output through Jekyll, which would otherwise
# strip the underscore-prefixed files rustdoc emits.
touch target/doc/.nojekyll

# Generated output is served verbatim, so keep git from rewriting line endings.
# Without this, a machine with core.autocrlf=true reports every file as modified
# on each rebuild and floods the run with warnings.
printf '* -text\n' > target/doc/.gitattributes

WORKTREE="$(mktemp -d -t ghpages-XXXXXX)"
cleanup() {
    git worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
    rm -rf "$WORKTREE"
}
trap cleanup EXIT

echo "==> Staging into $BRANCH"
if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
    git worktree add "$WORKTREE" "$BRANCH" >/dev/null
elif git ls-remote --exit-code --heads "$REMOTE" "$BRANCH" >/dev/null 2>&1; then
    git fetch "$REMOTE" "$BRANCH":"$BRANCH" >/dev/null
    git worktree add "$WORKTREE" "$BRANCH" >/dev/null
else
    # First run: start the branch with no history, since documentation is
    # regenerated wholesale and its past versions are not interesting.
    echo "    (creating $BRANCH)"
    git worktree add --detach "$WORKTREE" >/dev/null
    git -C "$WORKTREE" checkout --orphan "$BRANCH" >/dev/null
    git -C "$WORKTREE" rm -rf . >/dev/null 2>&1 || true
fi

# Replace the contents wholesale so files deleted since the last build disappear.
find "$WORKTREE" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
cp -r target/doc/. "$WORKTREE"/
# Cargo's build lock is an artifact of the local build, not documentation.
rm -f "$WORKTREE/.lock"

git -C "$WORKTREE" add --all
if git -C "$WORKTREE" diff --cached --quiet; then
    echo "==> No documentation changes"
    exit 0
fi

git -C "$WORKTREE" commit -q -m "docs: rebuild from $(git rev-parse --short HEAD)"
echo "==> Committed to $BRANCH"

if [ "$PUSH" -eq 1 ]; then
    git -C "$WORKTREE" push "$REMOTE" "$BRANCH"
    echo "==> Pushed to $REMOTE/$BRANCH"
else
    echo
    echo "Staged but not pushed. To publish:"
    echo "    git push $REMOTE $BRANCH"
    echo "Or re-run with --push."
fi
