# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.1.6] - 2026-03-04
- Add support for Hetzner Cloud provider ([#24](https://github.com/codercengiz/consul-external-dns/pull/24))
- Fix CI: Replace broken rust-cross action with cross CLI ([#23](https://github.com/codercengiz/consul-external-dns/pull/23))

## [0.1.5] - 2025-05-20
- Gracefully handle 422 responses from the Hetzner API
- Fix Consul lock acquisition issue ([#19](https://github.com/codercengiz/consul-external-dns/issues/19))

## [0.1.4] - 2024-10-15
- Fix wiping out old state when acquiring lock

## [0.1.3] - 2024-10-10
- Take the Hetzner DNS zone ID from the environment 
- Take the consul address from the environment

## [0.1.2] - 2024-10-02
- Add missing Hetzner DNS API URL slash
- Guard all long-running internal loop on the cancellation token
- Add tracing-subscriber for log output

## [0.1.1] - 2024-10-02
- Fix container image

## [0.1.0] - 2024-10-01
- Initial release

<!-- next-url -->
[Unreleased]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.6...HEAD
[0.1.6]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/codercengiz/consul-external-dns/compare/v0.1.0...v0.1.1