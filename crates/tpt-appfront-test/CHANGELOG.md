# Changelog

All notable changes to `tpt-appfront-test` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial crate: `Harness<Msg>`, a headless harness for `UITree`s with `new`,
  `rerender`, `tree`, `snapshot`, `find_by_text`, `find_by_role`, `find_by_key`,
  `click`, `type_into`, `press_key`, `drain_msgs`, `assert_msg_emitted` and `lint`.
- `lint_tree` and `assert_accessible`, with the rules `unlabeled-input`,
  `nameless-button`, `heading-skip`, `duplicate-key`, `missing-alt` and
  `action-without-description`.
- `role`, `text_of` and `walk` helpers.
- `parity` feature: `parity_gaps`, `assert_parity` and `BackendGap`, which check
  HTML, TUI and AI schema output against the words declared in the tree.
