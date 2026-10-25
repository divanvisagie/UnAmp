# 0032. Browse and mount network shares through GVfs, as Photograph and Files do

Date: 2026-10-25

## Status

Accepted. The user asked for UnAmp to do the same network browsing Photograph now does
([Photograph ADR-0029](https://github.com/divanvisagie/Photograph/blob/master/docs/adr/0029-browse-and-mount-network-shares-through-gvfs.md)).
Supersedes [ADR-0005](0005-reuse-photograph-mount-discovery.md)'s GVfs discovery (a `read_dir` of
`$XDG_RUNTIME_DIR/gvfs`); its copy of the storage and classic network mounts stands.

## Context

UnAmp's sidebar listed network shares that were already mounted: NFS, CIFS and sshfs from
`/proc/self/mounts`, and GVfs shares by reading the FUSE bridge's directory, both copied from
Photograph (ADR-0005). Playing from a share that wasn't mounted meant mounting it in Nautilus first.

Photograph and Sector F's Files now mount shares themselves through GVfs: `network:///` lists the
servers DNS-SD and SMB browsing find, `smb://host/` lists a server's shares, mounting one asks for a
password through a `GMountOperation` (which can save it in the keyring), and every mount appears as
a folder under `/run/user/<uid>/gvfs`. Files' ADR-0005 ruled out `mount.cifs`, libsmbclient and
the `gio` command line; UnAmp reads tracks only by path, so the same reasons hold.

## Decision

We will copy Photograph's GVfs module into UnAmp (`src/network.rs`), on a thread of its own running
a GLib main loop, as ADR-0005 copied its mount discovery:

- **The sidebar's Network section** starts with a Network place, listing the network's servers in
  place of the tracks, as rows like the folder view's. Then come the classic mounts, then every
  GVfs share from GIO's volume monitor (mounts made by Nautilus, Files or Photograph too), each with
  an unmount button.
- **A server opens to its shares; a share mounts and opens** as an ordinary folder, the tree rooted
  at it. Up from a share's top goes to its server, and from a server to the network. A share
  unmounted while open sends the library to its server.
- **A server's password or question** comes up as Nautilus's dialog. Errors go to the status bar.
- **The path bar takes `smb://` URIs.**
- **Linux only.** `gio` is a Linux-only dependency; on macOS the Network place isn't shown.
- Screenshot mode leaves the user's GVfs shares out, as it leaves out their other mounts.

## Consequences

- UnAmp shares mounts and saved passwords with the rest of the desktop.
- UnAmp links GLib and GIO on Linux. The `.deb` depends on `libglib2.0-0` and recommends
  `gvfs-backends` and `gvfs-fuse`; the CI runner installs `libglib2.0-dev`.
- There are now three copies of the module (Files, Photograph, UnAmp). A fix to one has to be made
  to the others. This is ADR-0005's trigger to extract a crate, and the case for one is stronger
  now that the code holds a password dialog's state as well as discovery.
- Browsing depends on what GVfs finds; servers that advertise neither DNS-SD nor WS-Discovery have
  to be typed into the path bar.
