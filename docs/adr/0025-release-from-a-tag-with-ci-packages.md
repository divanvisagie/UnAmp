# 0025. Cut releases locally with one command, and build the .deb and .dmg on GitHub

Date: 2026-10-10

## Status

Proposed. The user asked for the flow: a single local `make release` that increments the version,
pushes a release tag and publishes to crates.io; a GitHub worker that builds a macOS `.dmg` and a
Linux `.deb`; then the website's links moved to the new files under the new release. The details
below are my design.

## Context

`make release` built the `.deb` on the user's machine and uploaded it with the GitHub release.
Before that, `make bump` had to set the version and the website's download links in a separate
step. There was no macOS build: [ADR-0001](0001-linux-only-egui-native-app.md) made UnAmp
Linux-only, following Photograph. The user now wants a `.dmg` as well. A Mac build can't come
from a Linux machine, and building packages on whichever machine cuts the release makes them
depend on that machine's setup.

UnAmp has little platform-specific code. Audio goes through cpal, which uses CoreAudio on macOS.
The parts that are Linux-only already fail gracefully when unavailable:
- mount discovery reads `/proc/self/mounts`;
- MPRIS and the light/dark watcher need a session D-Bus.

## Decision

- **One local target, `make release`:**
  - fast-forward `master`, then run the usual checks: on master, clean, in sync, tag unused;
  - bump the version: the patch number by default, or `BUMP=minor|major`, or `V=x.y.z`, which may
    be the current never-tagged version, so no bump commit is made;
  - dry-run `cargo publish` before anything leaves the machine;
  - tag, and push `master` and the tag;
  - open a **draft** GitHub release, with any `NOTES=` file above the generated changelog;
  - `cargo publish`, last, because it can't be undone.
- **The workflow (`.github/workflows/release.yml`) runs on the tag:**
  - **deb:** `make build-deb` on Ubuntu 22.04, the oldest supported runner, which keeps the glibc
    floor low. It also makes an unversioned `unamp_amd64.deb` copy for `releases/latest`.
  - **dmg:** `make dmg` on macOS 14 (`packaging/macos/build-dmg.sh`). It builds a universal
    Apple Silicon + Intel `UnAmp.app` with an icon rendered from the SVG, ad-hoc signed. The app
    goes on a drag-to-Applications disk image as `UnAmp-<version>.dmg`, plus an `UnAmp.dmg` copy.
  - **publish:** attach both packages to the draft (creating it if the tag came from elsewhere)
    and publish it as latest.
  - **site:** run `scripts/update-site-links.sh` on `master`, which moves the website's download
    buttons, wget and apt lines to the new versioned files. Commit as `github-actions[bot]`,
    push, and request a Pages build, since pushes made with the workflow token don't start other
    workflows.
- **Testing the packages without releasing:** the workflow can also be run by hand
  (`workflow_dispatch`). It then builds both packages as artifacts and skips publishing and the
  site.
- **macOS is supported as a release build only:** UnAmp is still developed and used on Linux.
  The `.dmg` isn't notarised. The website and README tell users how to allow it, and that drives,
  shares and media keys are Linux-only.

## Consequences

- A release is one command and a wait. The release only becomes public, and the website only
  moves to it, once both packages are attached, so `releases/latest` never lacks a download.
- Packages are built the same way every time, on clean runners, no matter who cut the release.
- The crate is published before the packages are built. A failed package build leaves crates.io
  ahead of GitHub until the workflow is fixed and re-run on the tag.
- The site update is a bot commit on GitHub's `master`. Local `master` falls behind and the urd
  mirror lags until the next pull and push. `make release` pulls first, so the next release
  isn't blocked by it.
- `make bump` is gone. Its version bump is part of `make release`, and the link update moved to
  the workflow.
- The macOS build isn't tried on a Mac before release; the runner compiling and packaging it is
  the only check. Without signing and notarisation, every user has to allow it by hand.
- This partly supersedes ADR-0001's "target Linux only": Linux stays the platform UnAmp is built
  for, and macOS gets an automated, best-effort `.dmg`.
