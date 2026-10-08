# Logging in to a real server

How CI proves justelnet logs in to a real Unix telnetd (#35): the `e2e` job in `.github/workflows/ci.yml` starts `ci/inetutils-telnetd` and runs `justelnet-expect/tests/real_telnetd.rs` against it. This is the only test in the repository or CI against a real server; everything else is deterministic. (Probes against the maintainer's lab telnetd, recorded in other notes, are run by hand.)

The test account (`e2e`, password `justelnet-e2e`) is written in the Dockerfile and the test, in the open: it is a throwaway that exists only in the image, which needs no secrets (#24 story 3), and the container listens only on the runner's loopback.

## The server is GNU inetutils telnetd 2.8, built from source

The container builds telnetd and inetd from the signed GNU release (`inetutils-2.8.tar.gz`, signature by Simon Josefsson checked with the GNU keyring on 2026-10-07, sha256 pinned in the Dockerfile) on a Debian trixie-slim image pinned by digest. Building from the release rather than installing Debian's `inetutils-telnetd` was the maintainer's call, made on 2026-10-07: it tests upstream without distribution patches, and the pinned tarball stays on ftp.gnu.org, where a pinned Debian package version leaves the archive at its next security update. The cost is a 49 s image build (measured locally, 2026-10-07). It is theirs to reverse.

From 2.7 on, releases ship only as `.tar.gz`; there is no `.tar.xz`.

The test lives in `justelnet-expect/tests/` as an ignored test, and the container under `ci/`, rather than in a separate workspace crate or as an example: the maintainer's call, made on 2026-10-07, and theirs to reverse.

## Traps in running it

- telnetd does not listen by itself; it serves the socket inetd hands it on stdin (Debian's package depends on `inetutils-inetd | inet-superserver`). inetd is the container's main process, with `--foreground` (added in inetutils 2.7).
- inetd built with the default prefix reads `/usr/local/etc/inetd.conf`, so the config path is given on its command line; without it inetd starts, logs that the file is missing, and serves nothing.
- 2.8's telnetd ignores every environment option by default (the fix for CVE-2026-24061 and CVE-2026-28372), so what the default policy answers to NEW-ENVIRON never reaches login.
- The banner names the host's kernel (`Linux 6.6.87.2-microsoft-standard-WSL2 (e2e)` locally), so nothing matches on it. The prompts are matched by anchored regexes; `docker run --hostname e2e` keeps the login prompt itself the same on every run (`e2e login: `, where Docker's default would be the container ID), though the test anchors only `login: $`.

## What the default policy meets there

Recorded on 2026-10-07 with the default Option policy: the server opens with `DO TTYPE, TSPEED, XDISPLOC, NEW-ENVIRON, OLD-ENVIRON`, then `DO TM` and `DO LINEMODE` among others. The client answers `WONT TM` and `WONT LINEMODE`, and the login completes. These are the first sightings of both on a real server; until then the refusals rested on the target research's reading of inetutils and NetBSD source (#16). The lab netkit telnetd sends neither.

The server also sends `DO BINARY`, which the default policy accepts, so `send_line` sends a bare CR (`e2e\r`), and telnetd takes it as the end of the line.

## What the job proves, and how it fails

The test asserts that the command's output line (`justelnet-42`, computed by the shell) arrives, not just its echo (`echo justelnet-$((6*7))`), and that after `exit` the client sees the Closed Event with no error. A failed wait panics with the buffer it carried, which is what a stuck login shows in the CI log.

A second step runs the same test with a wrong password and requires it to fail with `Login incorrect` in the output, so the failure path is shown on every run, not just assumed. Mutating the command's output and dropping the logout each turned the test red (2026-10-07).

Measured locally on 2026-10-07: the login takes 0.8 s, the wrong-password step 10.5 s (almost all of it the 10 s wait for a shell prompt that never comes), and the image build 49 s.

## The recording artifact

Every run uploads `inetutils-telnetd-recording`: the bytes each side sent, in order, as `server:` and `client:` lines, written even when the test fails. It is not replayable as is, since the runner also checks `event:` lines, which a byte recording does not have. Promoting it into the replay suite is the capture-to-Transcript procedure in [docs/transcripts.md](../transcripts.md#from-the-end-to-end-recording). Up to the login prompt it was the same in six runs (2026-10-07 to 2026-10-08) and is promoted as `telnetd-inetutils-2.8-recorded.txt`; what follows the prompt carries the test account and is not.

#24 and #35 call this artifact a Transcript, but a Transcript (GLOSSARY.md) carries the `event:` lines too. Calling it a recording (`JUSTELNET_E2E_RECORDING`, artifact `inetutils-telnetd-recording`) is the maintainer's call, made on 2026-10-07 over keeping the spec's word or adding a glossary entry; it is theirs to reverse.
