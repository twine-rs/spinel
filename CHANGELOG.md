# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Request timeouts in the connection actor, with a sweep that expires
  overdue requests.

### Changed

- Doc comment wording and code style across the workspace, with no
  functional change.

### Fixed

- HDLC encode escapes frame and checksum bytes.
- HDLC decode rejects short input with an error instead of a panic.
- The net stream property ID is 0x72 per the protocol numbering.
- A device reset resolves every pending request instead of leaving the
  callers hanging.
- A blocked transport write cannot stall the actor reads.
- The actor stops when the last handle drops, which releases the
  transport.
- Frames on the broadcast IID pass the filter, so reset notifications
  reach every instance.
- spinel-cli waits for the reset notification before the next request.
