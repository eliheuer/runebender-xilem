# Releasing

How a runebender-core release will be cut. No release exists yet;
this file exists so the first one is mechanical.

## Checklist

1. Make sure CI is green on `main`.
2. Run `cargo vet` and clear anything it raises. New dependencies
   land as audits or exemptions in `supply-chain/`, never silently.
3. Move the `Unreleased` notes in `CHANGELOG.md` under the new
   version heading, with the date.
4. Bump `version` in `Cargo.toml`. The front-ends pin this crate by
   git revision, so bump their pins in the same session.
5. Tag `vX.Y.Z` and push the tag.
6. Create a GitHub release from the tag, pasting the changelog
   section.

## Known blocker for crates.io

Publishing to crates.io is not possible yet: `img2bez` and `spline`
are git dependencies, and crates.io rejects those. Until both are
published or replaced, a release is a git tag, and consumers install
with `cargo install --git` at the tag.

## Versioning

Semantic Versioning from the first release. Before 1.0, breaking
changes bump the minor version.
