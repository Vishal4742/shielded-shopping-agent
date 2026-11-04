# Shielded Shopping Agent — Documentation

Free Society Hackathon (Mumbai, 4–5 Nov 2026). Status: **design baseline v1.0, no code yet.**

| # | Document | Standard / purpose | Status |
|---|---|---|---|
| 01 | [Project Charter](01-project-charter.md) | Vision, objectives, scope, stakeholders, milestones | Draft, awaiting approval |
| 02 | [Feasibility Study](02-feasibility-study.md) | Technical, economic, operational, schedule, legal; option scoring | v1.0 |
| 03 | [SRS](03-srs.md) | ISO/IEC/IEEE 29148: FR, BR, NFR, use cases | v1.0 |
| 04 | [Architecture](04-architecture.md) | arc42 + C4 (context, container, component), runtime, deployment | v1.0 |
| 05 | [Detailed Design](05-detailed-design.md) | Data dictionary, state machine, algorithms, interfaces, message catalogue | v1.0 |
| 06 | [ADRs](06-adrs.md) | 10 architecture decisions | Accepted |
| 07 | [Threat Model](07-threat-model.md) | STRIDE, trust boundaries, security checklist | v1.0 |
| 08 | [Test Plan](08-test-plan.md) | IEEE 829 (condensed): ~45 test cases, environments, exit criteria | v1.0 |
| 09 | [Project Plan](09-project-plan.md) | WBS, Phase 0 prep, hack-day schedule, cut list | v1.0 |
| 10 | [Risk Register](10-risk-register.md) | 15 risks, scored, with mitigations | v1.0 |
| 11 | [Traceability Matrix](11-traceability-matrix.md) | Requirement → design → threat → test | v1.0 |
| 12 | [Ops Runbook](12-ops-runbook.md) | Start/stop, demo-day prep, incident playbook | v1.0 |
| 13 | [Engineering Standards](13-engineering-standards.md) | Code, security, git, Definition of Done | v1.0 |
| R1 | [Design Review 30 Oct](reviews/2026-10-30-design-review.md) | Independent staff-engineer review + how each finding was addressed | Closed |

## Reading order
Charter → Feasibility → SRS → Architecture → Detailed Design → Test Plan → Project Plan.

## Change control
Changes to requirements update the SRS first, then the traceability matrix, then the design/tests.
Each document's version line is bumped on change.
