# Agent Tool Selection And Spawn

This file is now a compatibility pointer. Its content was split along the
runtime-agnostic / runtime-bound seam:

- Tier concept, selection strategy, role policy, and load-bearing escalation
  → `notes/playbook/capabilities.md` (the agnostic capability contract).
- Solo-specific resolver mechanics (`list_agent_tools()` shape, classification
  tokens, return/failure shapes, spawn behavior) and the `/spawn-agent <tier>`
  control-plane abstraction → `notes/playbook/runtimes/solo.md` § Tier
  Resolver.

If an older prompt or note references this file, treat it as pointing at
those two files. The control-plane abstraction is unchanged: callers request
capability tier (`small`, `medium`, `large`) rather than a runtime-specific
tool id.
