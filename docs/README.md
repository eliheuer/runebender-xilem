# Runebender documentation

The [main README](../README.md) introduces the project and its local build commands.
[AGENTS.md](../AGENTS.md) records contributor instructions and links to the canonical architecture and design documentation on [runebender.org](https://runebender.org).
Commands in these guides run from the repository root unless stated otherwise.

## Releases and installation testing

- [Release plan and runbook](RELEASING.md): choosing a commit, packaging, signing, notarization, publishing, website downloads, and future distribution channels.
- [Testing a macOS DMG](TESTING-MACOS.md): the everyday build, install, upgrade, and validation workflow.
- [Changelog](CHANGELOG.md): changes being prepared and notes for published versions.

## Interface and local models

- [Interface design and headless screenshots](UI-DESIGN.md): shared panel conventions and visual validation.
- [Local model packages](LOCAL-MODELS.md): package layout, runtime requirements, and installation.
- [Virtua Regular testing](VIRTUA-REGULAR-TESTING.md): scratch-font setup and manual review.

## Planning and review records

These checklists retain implementation decisions, unfinished work, and earlier validation limits.
Check their dates and the current implementation before treating an item as completed or as the next task.

- [Virtua Regular AI checklist](VIRTUA-REGULAR-AI-CHECKLIST.md)
- [AI architecture checklist](AI-ARCHITECTURE-CHECKLIST.md)

Keep repository-wide guides and checklists here, and add them to this index.
Component-specific README files stay beside the code or assets they describe.
