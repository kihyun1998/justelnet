# Changelog

All notable changes to `justelnet-expect` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- A wait whose timeout is too large to add to the current time, such as
  `Duration::MAX`, no longer panics; it waits with no deadline.

## [0.1.0] - 2026-10-12

### Added

- Initial release: Expect-style automation over a justelnet client.

[Unreleased]: https://github.com/kihyun1998/justelnet/compare/justelnet-expect-v0.1.0...HEAD
[0.1.0]: https://github.com/kihyun1998/justelnet/releases/tag/justelnet-expect-v0.1.0
