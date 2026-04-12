# react-native-enriched-markdown vendoring

This repo is migrating `react-native-enriched-markdown` away from `patch-package`
to a dedicated fork managed as a git submodule.

## Snapshot saved before migration

The pre-migration local state was captured at:

`/Users/mono/Repos/xmtp-app/.cache/vendor-snapshots/react-native-enriched-markdown-2026-04-12`

That snapshot includes:

- `react-native-enriched-markdown+0.4.1.patch`
- `react-native-enriched-markdown-0.4.1.tgz`
- `history.log`
- `worktree-status.txt`
- `package.json.snapshot`

## Current layout

The vendored dependency now lives at:

`/Users/mono/Repos/xmtp-app/vendor/react-native-enriched-markdown`

`xmtp-mobile/package.json` points to it via:

`"react-native-enriched-markdown": "file:../vendor/react-native-enriched-markdown"`

The previous `postinstall` hook for `patch-package` has been removed.

## Fork provenance

The original local migration repo was:

`/Users/mono/Repos/react-native-enriched-markdown-fork`

The canonical fork is now:

`git@github.com:andelf/react-native-enriched-markdown.git`

The XMTP-managed branch is:

`xmtp-mobile`

Current custom fork history on that branch:

- `ef84b58` Import react-native-enriched-markdown 0.4.1 upstream package
- `c47914d` Apply XMTP mobile markdown customizations
- `1ad4568` Remove prepare hook for vendored consumption
- `148c7df` Fix code block height measurement for long lines

## Submodule note

The submodule now points at the GitHub fork and tracks the custom branch:

- `url = git@github.com:andelf/react-native-enriched-markdown.git`
- `branch = xmtp-mobile`

The main repo still pins an exact submodule commit. The branch entry is there
to make future submodule updates more obvious.

## Validation completed

The migration has been validated with:

- `npm install` in `xmtp-mobile`
- `npm ls react-native-enriched-markdown`
- `./gradlew :react-native-enriched-markdown:compileReleaseKotlin`

## Recommended next steps

1. Decide whether to keep `xmtp-mobile/patches/react-native-enriched-markdown+0.4.1.patch`
   as historical reference or delete it once the fork becomes canonical.
2. Continue future markdown fixes inside the fork, then advance the submodule
   pointer in this repo.
