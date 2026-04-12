# Research Document Index

This directory contains project-facing research and design notes for `xmtp-app`.

The goal of this index is to make three things explicit:

- which files are authoritative
- which files are focused appendices or deep dives
- which docs should be updated when project reality changes

## Authoritative research docs

### `xmtp.md`
Public-API snapshot of the Rust `xmtp 0.8.1` crate.

Use this for:
- SDK architecture and API-shape understanding
- Rust-side capability boundaries inferred from docs.rs
- trade-off discussions around client, conversation, content, and stream APIs

Do not use this for:
- mobile SDK assumptions
- active XIP status
- product implementation status across this repo

### `xmtp-active-proposals.md`
Authoritative project-facing summary of high-signal XMTP ecosystem features and proposals.

Use this for:
- what matters most to this repo right now
- whether a topic is implemented here, worth tracking next, or just worth watching
- current project stance on Actions / Intent, edit/delete, typing, payments, and atomic membership

### `xmtp-payment-content-types.md`
Authoritative note for XMTP payment-adjacent content types.

Use this for:
- `TransactionReference`
- `WalletSendCalls`
- `Messaging Fees`
- keeping protocol-level payment notes separate from mobile implementation detail

If you are editing mobile wallet UX plans, update this file when schema/support claims change.

### `identity-resolution.md`
Focused evaluation of app-layer identity resolution options.

Use this for:
- ENS vs Airstack vs other profile-enrichment approaches
- privacy / centralization trade-offs
- deciding whether richer identity is worth adding

## Focused appendices and deep dives

### `chat-action-typing-indicator.md`
Focused appendix preserving the reasoning behind the typing-indicator / ephemeral-status decision.

Use this for:
- why persisted-message simulation is a bad fallback
- the UX reasoning behind ephemeral typing/thinking states

Project-facing current status still belongs in `xmtp-active-proposals.md`.

### `2026-04-12-mobile-foldable-layout-report.md`
Specialized research report for Samsung foldables / large-screen adaptive layout issues in `xmtp-mobile`.

Use this for:
- foldable status-bar overlap analysis
- keyboard-gap root-cause analysis
- two-pane large-screen recommendations

### `cc-bridge/`
Implementation-prep research package for a parallel `xmtp-cli cc-bridge` path.

Read order:
1. `cc-bridge/README.md`
2. `cc-bridge/feasibility.md`
3. `cc-bridge/change-scope.md`
4. `cc-bridge/phased-plan.md`

Use this set for:
- deciding whether to build `cc-bridge`
- understanding why shared bridge-core extraction comes before adapter expansion
- evaluating transport and daemon hardening implications

## Related non-research docs outside this directory

### `docs/wallet-connect-integration-plan.md`
Implementation plan for mobile wallet execution UX.

This file should focus on:
- app flow
- SDK integration details
- deep links
- user approval flow
- session and error handling

It should not duplicate protocol schema details that already live in `xmtp-payment-content-types.md`.

## Maintenance rules

When updating research docs in this directory:

1. Keep protocol schema knowledge in one place.
   - Payment schemas belong in `xmtp-payment-content-types.md`
   - Other plans should link there instead of copying full definitions

2. Keep project status aligned with code reality.
   - If a feature is implemented, remove or rewrite stale “research TODO” language
   - `xmtp-active-proposals.md` should reflect the current repo, not an earlier plan

3. Distinguish ecosystem support from repo support.
   - “available somewhere in XMTP JS tooling” is not the same as “works natively in this repo today”

4. Mark version/time scope where it matters.
   - especially for SDK snapshots and proposal-status docs

5. Prefer preserving useful reasoning over preserving stale detail.
   - archive or downgrade duplicate content, but keep hard-won design conclusions when they still help future work
