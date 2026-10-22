# Logging in to a real server

## What it is

How CI proves justelnet logs in to a real Unix telnetd (#35): the `e2e` job in `.github/workflows/ci.yml` builds and starts `ci/inetutils-telnetd`, runs `justelnet-expect/tests/real_telnetd.rs` against it with `--ignored`, runs it again with a wrong password that must fail, and uploads a byte recording. This is the only test in the repository or CI against a real server; everything else is deterministic. Probes against the maintainer's lab telnetd (netkit on RHEL 9), recorded in other notes, are run by hand — the same test can be pointed there.

## Governing decisions

**None.** #24 and #35 specify the job; the server and placement choices are the maintainer's calls, dated below.

## Design model

From the source, the workflow, the Dockerfile and CONTRIBUTING.md.

### The server is GNU inetutils telnetd 2.8, built from source

The container builds telnetd and inetd from the signed GNU release (`inetutils-2.8.tar.gz`, signature by Simon Josefsson checked with the GNU keyring on 2026-10-07, sha256 pinned in the Dockerfile's `ADD --checksum`) on a Debian image pinned by digest (trixie-slim, per the maintainer, 2026-10-07; the digest alone does not name it). Building from the release rather than installing Debian's `inetutils-telnetd` was the maintainer's call, made on 2026-10-07: it tests upstream without distribution patches, and the pinned tarball stays on ftp.gnu.org, where a pinned Debian package version leaves the archive at its next security update. The cost is a 49 s image build (measured locally, 2026-10-07).

From 2.7 on, releases ship only as `.tar.gz`; there is no `.tar.xz`.

The test lives in `justelnet-expect/tests/` as an ignored test, and the container under `ci/`, rather than in a separate workspace crate or as an example: the maintainer's call, made on 2026-10-07.

### The test account and overrides

The test account (`e2e`, password `justelnet-e2e`) is written in the Dockerfile and the test (`USER`, `PASSWORD`), in the open: it is a throwaway that exists only in the image, which needs no secrets (#24 story 3), and the container listens only on the runner's loopback (`-p 127.0.0.1:2323:23`).

The account, its password and the shell prompt's regex (`SHELL_PROMPT`, `\ne2e\$ $`) default to the container's and can be overridden from the environment (`JUSTELNET_E2E_USER`, `JUSTELNET_E2E_PASSWORD`, `JUSTELNET_E2E_SHELL_PROMPT`; #92, commit `9a09008`), so the same test logs in to another server by hand; the login and password prompts and the command it runs are not overridable, since the lab netkit telnetd matches them as they are. Run against the lab on 2026-10-22 with its account and prompt (`\n\[testuser@localhost ~\]\$ $`), it logged in, read `justelnet-42` and saw `Closed` in 0.6 s; a prompt override that never matches timed out waiting for the shell prompt. The lab's address and password stay out of the repository. Pointed at a server it cannot log in to, the test is still the capture tool for that server's opening ([transcript replay](transcript-replay.md), "What a capture can and cannot supply").

### The socket

The test wraps its stream to record it (`Recorder`), so it connects with a plain `TcpStream` and `Client::new` rather than `Client::connect`, and sets on the socket what `connect` sets — see [socket setup matches connect](socket-setup-matches-connect.md). It reaches socket2 through a dev-dependency of `justelnet-expect`, already in the tree through `justelnet`. That is the maintainer's call, made on 2026-10-22 over a public function in `justelnet` that both `connect` and the test would call (#85).

### Traps in running it

- telnetd does not listen by itself; it serves the socket inetd hands it on stdin (Debian's package depends on `inetutils-inetd | inet-superserver`). inetd is the container's main process, with `--foreground` (added in inetutils 2.7).
- inetd built with the default prefix reads `/usr/local/etc/inetd.conf`, so the config path is given on its command line (`CMD` in the Dockerfile); without it inetd starts, logs that the file is missing, and serves nothing.
- 2.8's telnetd ignores every environment option by default (the fix for CVE-2026-24061 and CVE-2026-28372), so what the default policy answers to NEW-ENVIRON never reaches login.
- The banner names the host's kernel (`Linux 6.6.87.2-microsoft-standard-WSL2 (e2e)` locally, `Linux 6.17.0-1022-azure (e2e)` in the recorded Transcript), so nothing matches on it. The prompts are matched by anchored regexes; `docker run --hostname e2e` keeps the login prompt itself the same on every run (`e2e login: `, where Docker's default would be the container ID), though the test anchors only `login: $`.

### What the default policy meets there

Recorded on 2026-10-07 with the default Option policy (`justelnet-core/tests/transcripts/telnetd-inetutils-2.8-recorded.txt`): the server opens with `DO TTYPE, TSPEED, XDISPLOC, NEW-ENVIRON, OLD-ENVIRON`, asks TTYPE twice, then sends `DO TM` and `DO LINEMODE` among others. The client answers `WONT TM` and `WONT LINEMODE`, and the login completes. These are the first sightings of both on a real server; until then the refusals rested on the target research's reading of inetutils and NetBSD source (#16). The lab netkit telnetd sends neither.

The server also sends `DO BINARY`, which the default policy accepts (`WILL BINARY`, local BINARY on), so `send_line` sends a bare CR (`e2e\r`), and telnetd takes it as the end of the line ([sending](sending.md)).

### What the job proves, and how it fails

The test asserts that the command's output line (`justelnet-42`, computed by the shell) arrives, not just its echo (`echo justelnet-$((6*7))`), and that after `exit` the client sees the Closed Event with no error. A failed wait panics with the buffer it carried (`wait`), which is what a stuck login shows in the CI log.

A second step runs the same test with a wrong password (`JUSTELNET_E2E_PASSWORD: wrong`) and requires it to fail with `Login incorrect` in the output, so the failure path is shown on every run, not just assumed. Mutating the command's output and dropping the logout each turned the test red (2026-10-07).

Measured locally on 2026-10-07: the login takes 0.8 s, the wrong-password step 10.5 s (almost all of it the 10 s `WAIT` for a shell prompt that never comes), and the image build 49 s.

### The recording artifact

Every run uploads `inetutils-telnetd-recording` (`JUSTELNET_E2E_RECORDING`): the bytes each side sent, in order, as `server:` and `client:` lines, written even when the test fails (`Recording`'s `Drop`). Its header names the server by the address the test was given, not as inetutils (#92). It is not replayable as is, since the runner also checks `event:` lines, which a byte recording does not have. Promoting it into the replay suite is the capture-to-Transcript procedure in [docs/transcripts.md](../transcripts.md#from-the-end-to-end-recording). Up to the login prompt it was the same in six runs (2026-10-07 to 2026-10-08) and is promoted as `telnetd-inetutils-2.8-recorded.txt`; what follows the prompt carries the test account and is not.

#24 and #35 call this artifact a Transcript, but a Transcript (GLOSSARY.md) carries the `event:` lines too. Calling it a recording is the maintainer's call, made on 2026-10-07 over keeping the spec's word or adding a glossary entry.

## Code

- `justelnet-expect/tests/real_telnetd.rs` — `a_login_to_inetutils_telnetd`, `wait`, `Recorder`, `Recording`, `USER`, `PASSWORD`, `SHELL_PROMPT`, `WAIT`
- `ci/inetutils-telnetd/Dockerfile`
- `.github/workflows/ci.yml` — job `e2e`
- `justelnet-expect/Cargo.toml` — `socket2` dev-dependency
- `CONTRIBUTING.md` — "Logging in to a real telnetd"

## Reference behaviour

- `justelnet-core/tests/transcripts/telnetd-inetutils-2.8-recorded.txt` against `telnetd-inetutils-negotiation.txt` (source-read): the recording vs the source.
- [Target research](../research/target-options-quirks.md): inetutils and BSD's phase-2 `DO TM`, `DO LINEMODE`.

## Cross-cutting invariants

- [Socket setup matches connect](socket-setup-matches-connect.md) — the test is one of its sites.

## Blast radius

- [Option policy](option-policy.md) — the test runs the default policy; a change to it changes what inetutils is told (BINARY, TM, LINEMODE) and the recorded Transcript.
- [Waiting for patterns](waiting-for-patterns.md) — the test is a sequence of `expect` waits on anchored regexes; a change to wait semantics shows here first against a real server.
- [Sending](sending.md) — under inetutils' DO BINARY, `send_line` sends a bare CR; an end-of-line change is tested here only.
- [Driving a stream](driving-a-stream.md) — built with `Client::new`; `Event::Closed` after `exit` is asserted.
- [Transcript replay](transcript-replay.md) — the recording's line format is the Transcript format's `server:`/`client:` lines.
- [CI gates](ci-gates.md) and [releasing](releasing.md) — `e2e (ubuntu-latest, inetutils telnetd)` is a required job by its exact name.

## Known holes / open

- Only one real server in CI; the lab netkit run is by hand, and network gear is uncaptured. Tracked: #9.
- The Synch path is not exercised against the container (its recording held no Synch); the deterministic proof is the local `urgent_data_arrives_inline` test.
