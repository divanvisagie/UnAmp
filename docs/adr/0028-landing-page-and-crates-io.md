# 0028. Publish a landing page from `docs/` and the crate to crates.io on every release

Date: 2026-10-10

## Status

Proposed. Backfilled from commit 3420abd. The release steps are superseded by
[ADR-0025](0025-release-from-a-tag-with-ci-packages.md).

## Context

UnAmp was released as a `.deb` attached to a GitHub release by `make release`. There was no page
to point people at, and no way to install it with `cargo install`. Photograph, which UnAmp follows
([ADR-0001](0001-linux-only-egui-native-app.md)), already had a landing page.

## Decision

- `docs/index.html` is a landing page in the same style as Photograph's, served by GitHub Pages
  from `master`'s `docs/` folder. Its download button and wget line point at the versioned `.deb`.
- `make bump` sets the version and moves the page's download links to it in the same commit;
  `release-check` refuses a release the page doesn't link.
- `make release` ends with `cargo publish`, last because it can't be undone. If it fails, the
  GitHub release is already out, so only `cargo publish` is re-run.
- `make docs` serves the site locally. The README links the site and documents the `.deb` and
  `cargo install`.

## Consequences

- Every release is on crates.io, so `cargo install unamp` works. A published version can't be
  withdrawn, only yanked, and the crate name is now committed to.
- The page lives next to the code and moves with each release, but only if `make bump` is used;
  a hand-edited version leaves stale links, which `release-check` catches.
- The release now has two irreversible steps (the GitHub release and the crates.io publish) that
  can end up out of step if the second fails.
