# CC Bridge Research Set

This folder captures the feasibility study for adding a new `xmtp-cli cc-bridge` command alongside the existing `xmtp-cli acp` flow.

## Documents

1. [`feasibility.md`](./feasibility.md)
   - Whether `cc-sdk` is viable as the transport/runtime for a parallel bridge path
   - WebSocket support, heartbeat, reconnect, replay, and control features
   - Key constraints and unknowns

2. [`change-scope.md`](./change-scope.md)
   - What parts of the current bridge are reusable vs ACP-specific
   - Likely module split
   - Testing, CLI, daemon, and migration impact

3. [`phased-plan.md`](./phased-plan.md)
   - Recommended rollout order
   - Minimum viable `cc-bridge`
   - Refactor sequence to avoid destabilizing the existing ACP path

## Bottom line

Current conclusion: adding `cc-bridge` looks feasible and strategically useful, but it should be done as:

- a **parallel command**, not a replacement for `acp`
- on top of a new **shared XMTP bridge core**
- with a **transport adapter layer** separating ACP-specific and `cc-sdk`-specific protocol handling

The main architectural payoff is not just WebSocket transport. It is the opportunity to separate:

- XMTP ingress/egress and conversation runtime
- shared progress/tool/reply logic
- protocol-specific agent transport and event mapping
