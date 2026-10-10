# Agent collaboration guidelines

## Objective
Improve the **shogi-rsi project as a whole**, not just playing strength. Judge orchestration by measurable outcomes: validated improvements merged, quality, reduced duplicated work, and operating cost. Centralizing schedules or adopting a framework is an option, **not a goal**.

## Current agents
- Claude and ChatGPT already run independent scheduled improvement tasks. Their schedules and execution environments may not be accessible from this repository.
- GitHub Copilot may be used for scoped Issue-based work. Do not assume that assigning an Issue automatically creates a recurring manager.
- Before proposing new automation, inventory existing jobs, their owners, scopes, permissions, schedules, and outputs. Mark unknowns explicitly rather than inventing details.

## First assignment: Issue #12
Review https://github.com/tokutake/shogi-rsi/issues/12 and propose a minimal, practical coordination design. In the first PR, **produce a design document only** (suggested path: `docs/AGENT_ORCHESTRATION.md`). Include:
1. Current-state inventory with verified facts versus information requiring human input.
2. A comparison of leaving schedulers separate with shared Issue/PR reporting versus consolidating scheduling into GitHub Actions or another controller.
3. A minimal task ownership and collision-avoidance protocol, including concurrent PRs and stale tasks.
4. An implementation sequence with reversible steps, security/permission boundaries, cost limits, and no duplicate scheduled runs.
5. Observable success criteria and a small pilot that tests whether coordination improves actual delivery.

Do not introduce infrastructure, create recurring jobs, assign other agents, change existing schedules, merge PRs, or enable automatic dispatch during this initial design task. Do not edit `CLAUDE.md` as part of this task. Ask for missing information rather than pretending to see ChatGPT/Claude scheduler internals.

## Implementation conventions
- One active implementation owner per Issue; work on a dedicated branch and open a PR.
- Keep changes scoped, test where applicable, and report evidence and limitations.
- Avoid changing other agents' work without coordination.
- Human approval is required before introducing new credentials, autonomous agent dispatch, or auto-merge.
