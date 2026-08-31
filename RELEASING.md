# Releasing

How a runebender-xilem release will be cut. No release exists yet;
this file exists so the first one is mechanical.

## Checklist

1. Make sure CI is green on `main`.
2. Run `cargo vet` and clear anything it raises. New dependencies
   land as audits or exemptions in `supply-chain/`, never silently.
3. Pin `runebender-core` to that crate's release tag, not a loose
   revision.
4. Move the `Unreleased` notes in `CHANGELOG.md` under the new
   version heading, with the date.
5. Bump `version` in `Cargo.toml`, tag `vX.Y.Z`, and push the tag.
6. Create a GitHub release from the tag, pasting the changelog
   section.

## Distribution

The editor depends on Xilem from its git repository at a pinned
revision, so this crate cannot be published to crates.io. A release
is a git tag; users install with
`cargo install --git https://github.com/eliheuer/runebender-xilem --tag vX.Y.Z`.

## Versioning

Semantic Versioning from the first release. Before 1.0, breaking
changes bump the minor version.
