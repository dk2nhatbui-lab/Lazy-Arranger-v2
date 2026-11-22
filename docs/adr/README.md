# Architecture decision records

One file per decision. Status is one of **Proposed**, **Accepted**, **Deferred**
or **Superseded**.

| ADR | Decision | Status |
| --- | --- | --- |
| [001](ADR-001-ui-toolkit.md) | Desktop UI toolkit | **Accepted: egui, v1 Studio Console style** |
| [002](ADR-002-audio-clock.md) | Audio clock, tick/sample conversion, MIDI dispatch | Accepted; internal clock and dispatch worker implemented |
| [003](ADR-003-canonical-style.md) | Canonical style model, manufacturer metadata | Accepted; conversion implemented, CASM import open |
| [004](ADR-004-project-schema.md) | Project schema versioning and legacy migration | Accepted, implemented |
| [005](ADR-005-provenance.md) | Intent-to-generated-MIDI provenance and regeneration | Accepted, implemented (track-level) |
| [006](ADR-006-command-undo-mcp.md) | Typed commands, undo/redo, MCP approval | Accepted (commands), proposed (MCP) |
| [007](ADR-007-plugin-host.md) | Plugin host isolation and licensing | Deferred |
| [008](ADR-008-cpp-interop.md) | C++/Rust interoperability | Accepted, implemented |
| [009](ADR-009-hardware-capabilities.md) | Hardware capability model and SysEx | Accepted (principles) |
| [010](ADR-010-score-engraving.md) | Score engraving seam, tiles and PDF | Proposed (Stage 8) |

The shared type contract that these decisions constrain is
[`../architecture/contracts.md`](../architecture/contracts.md).
