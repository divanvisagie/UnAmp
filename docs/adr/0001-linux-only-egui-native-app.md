# 0001. Build UnAmp as a Linux-only native egui/eframe app, following Photograph

Date: 2026-10-09

## Status

Accepted. "Linux only" is relaxed by [ADR-0025](0025-release-from-a-tag-with-ci-packages.md): releases also ship an
automated, unsigned macOS `.dmg`, while Linux stays the platform UnAmp is built for.

## Context

UnAmp is a music player in the spirit of Winamp: point it at a folder or a network mount, play
what's there, show album art. It was asked for explicitly as "egui, just like Photograph", so it
can share Photograph's toolchain, layout conventions (sidebar of locations, path bar, central
content), packaging (`Makefile` → `.deb`) and release flow, and code where it fits.

Photograph is Linux-only ([Photograph ADR-0012](https://github.com/divanvisagie/Photograph/blob/master/docs/adr/0012-drop-macos-support-linux-only.md))
and ships a `.deb` rather than a Snap, because strict confinement can't reach arbitrary mounts like
`/tank` or GVfs shares ([Photograph ADR-0016](https://github.com/divanvisagie/Photograph/blob/master/docs/adr/0016-drop-snap-packaging.md)).
A music player that browses NAS shares hits exactly the same wall.

Unlike Photograph, UnAmp needs no GPU pipeline: eframe's default renderer is enough and there is no
Vulkan dependency. Its platform dependency is audio output, through ALSA (which PipeWire and
PulseAudio both serve).

## Decision

We will build UnAmp in Rust on eframe/egui at the same version as Photograph (0.35), target Linux
only, and package it as a `.deb` via a `Makefile` derived from Photograph's.

## Consequences

- Fixes, patterns and code move freely between the two apps (the sidebar's mount discovery is
  already shared this way — see [ADR-0005](0005-reuse-photograph-mount-discovery.md)).
- egui upgrades should happen in both apps together, or the two drift apart in API.
- No macOS/Windows builds. rodio and eframe are cross-platform, so this is a support decision, not
  a technical lock-in — but nothing outside Linux is tested, and mount discovery reads
  `/proc/self/mounts`.
- No Snap/Flatpak: same trade-off as Photograph — no storefront discovery in exchange for reaching
  any mounted share.
