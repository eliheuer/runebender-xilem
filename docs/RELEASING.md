# Releasing Runebender

The first native release is being prepared as `0.1.0`.
The canonical repository is [fontgarden/runebender](https://github.com/fontgarden/runebender).
It was transferred from `eliheuer/runebender-xilem` on October 17, 2026, preserving its history and GitHub identity.
Keep the old GitHub repository path unused so its redirects continue to work.
The packaging command below creates a macOS application bundle and a disk image with an Applications shortcut.
Packaging alone does not establish that the editor is ready for release.

Initial validation on October 16, 2026 passed for a local Apple Silicon package: release build, bundle signature verification, DMG integrity, mounting, copying the app out, and ejecting the image.
That earlier copied app reported `0.1.0` and rendered both the no-document screen and a copied Babelfont fixture through the headless path, from a temporary working directory with no user configuration or developer environment.
That earlier artifact verifies packaging and headless startup for its source state; it does not validate later source or packaging changes.
Interactive Finder launch, file dialogs, saving, Intel compatibility, Developer ID signing, and notarization remain release gates.

On October 17, 2026, the canonical checkout produced an Apple Silicon test DMG with the temporary icon and bundled Shapes font.
Two consecutive invocations preserved separate output directories; the second reused the compiled binary.
The second image passed checksum, mount/copy/eject, copied-app signature, bundle-icon and sample-font checks, `--version`, and visually inspected headless welcome and Shapes captures in an isolated environment.
This was a dirty-tree local test build at `b338d2a`, not a public release candidate or an interactive installation test.
Its artifacts, build receipt, and validation results are under the corresponding timestamped `target/macos/` directory.

## Release strategy

The first public release targets macOS, initially Apple Silicon, with Linux installation from a pinned Git tag through Cargo.
Ship an Intel DMG only after testing it on an Intel Mac; being able to cross-compile it is insufficient.
Treat Linux as an experimental source installation until a Linux desktop has passed the editing and installation checks.
Windows remains a visible CI diagnostic, without an installer or a supported desktop release.
Homebrew, crates.io publication, and automatic updates are subsequent milestones rather than requirements for the first DMG.

| Component | Initial approach | Status |
| --- | --- | --- |
| Local macOS packages | `./scripts/build_dmg.sh`, using the temporary icon | Implemented; local test signing |
| Public macOS packages | Developer ID Application, notarization, stapling | Packaging commands exist; signed path awaits validation |
| Release validation | Existing Linux/macOS CI plus installation tests | CI needs repair; manual checklist below |
| Release orchestration | Small Python script wrapping existing tools | Planned; no `scripts/release.py` exists yet |
| Hosting | Versioned GitHub Release assets | Planned; no public release yet |
| Website | Download page linking to those assets | Planned in the website repository |
| Linux installation | Cargo from the exact release tag, with `--locked` | Must be tested outside the checkout |
| Changelog | Curated [CHANGELOG.md](CHANGELOG.md) | Started; first release notes still need curation |
| Updates | Download a new DMG and replace the closed app | Initial policy; updater integration is follow-up work |

Keep signing on the maintainer's Mac for the first release.
Use GitHub Actions for validation without uploading the Developer ID private key to CI at this stage.
This avoids making certificate export and CI secret management a prerequisite for v0.1.
Later, a manually dispatched workflow can use a protected release environment and reviewed permissions.
Pull requests must never receive release credentials.

### Where a release comes from

Work normally in the canonical checkout, currently `~/GH/repos/runebender`, using branches and reviewed commits.
Prepare the Cargo version, changelog, icon, lockfile, packaging changes, and installation documentation in committed source before choosing the release commit.
Record its full commit SHA; later changes on `main` belong to a subsequent release.
The release source must be reachable from the canonical repository's `main` branch, and the required checks must pass for that exact SHA.

Build public artifacts in a separate, clean checkout or detached worktree at that SHA, with a separate Cargo target directory.
Do not build public releases from an editor's dirty development tree, or reset or stash unrelated work to make it clean.
Do not follow a moving branch during packaging or substitute the latest successful CI run from another commit.
Record the toolchain, dependency lockfile hash, test-fixture revision, source SHA, architecture, minimum macOS version, icon hash, signing identity, artifact hashes, and validation results.
The result should be traceable and repeatable; signing timestamps mean that repeated builds need not have identical bytes.

The current package script permits dirty source for local experiments and records that fact in `build-info.json`.
That receipt is diagnostic information, not a guarantee of an immutable source snapshot or a successful public-release review.
The planned release wrapper must enforce a clean source snapshot and reject changes detected during a public build.

### The release sequence

1. **Prepare:** merge the version, curated notes, icon, and release configuration; select the full commit SHA.
2. **Validate:** run the required checks on that SHA, including the browser checks when shared sources affect it, and retain the results.
3. **Build:** create the app and DMG on macOS; sign, notarize, staple, and verify the public artifacts.
4. **Test:** exercise the packaged app using [the macOS installation checklist](TESTING-MACOS.md), and test the Linux source-install command in a clean environment.
5. **Stage:** create a draft GitHub Release whose version tag resolves to the chosen SHA; upload the already tested artifacts, notes, checksums, and release manifest.
6. **Review and publish:** inspect the draft, tag target, support claims, and checksums, then explicitly publish it.
7. **Verify and announce:** download the public assets, verify their checksums, finish the browser-download Gatekeeper test, and update the website's stable downloads only after those checks pass.

Staging and publishing must reuse the tested files rather than rebuilding them.
Draft assets do not constitute a public download route; arrange the installation test locally before publication and repeat the download test against the public URL afterward.
Never move a public version tag or silently replace an artifact with different bytes under the same version.
If a published release is defective, remove it from the advertised stable downloads, document the problem, and ship a new patch version.
Keep prior release files available for rollback where safe; warn when a source-format change makes downgrading unsafe.

### Planned automation contract

Implement one small `scripts/release.py` using Python's standard library, Git, Cargo, the GitHub CLI, and the existing package script.
The interface below is a design target, not commands that work today:

| Proposed command | Responsibility |
| --- | --- |
| `prepare --commit FULL_SHA` | Verify canonical remote, clean committed inputs, version and notes; create an isolated source checkout and release receipt |
| `validate --receipt PATH` | Run or verify the required checks for that SHA and retain their evidence |
| `build-macos --receipt PATH --target TARGET` | Invoke the package script with committed icon and signing settings; append notarization results and final artifact hashes |
| `draft --receipt PATH` | Check release gates and tag target; upload the existing files and curated notes to a draft |
| `publish --receipt PATH` | Recheck remote draft contents and hashes, then publish only when explicitly requested |

Store non-secret release configuration, such as supported targets and a numeric macOS build number, in a small committed `release.toml` when implementing the wrapper.
Read the public version from Cargo instead of maintaining a second version string there.
Keep local test builds separate from public artifacts, and make a missing check or rejected notarization stop the pipeline.
Retain failed-build diagnostics; do not automatically delete worktrees that may contain changes.
Retries must not overwrite existing public assets or create a second release for the same version.

Use SemVer tags such as `v0.1.0` and, when needed, explicitly marked prereleases such as `v0.1.0-rc.1`.
Prereleases must not become the website's stable download.
Before packaging an RC, separate the user-visible version from Apple's numeric bundle version fields: `CFBundleShortVersionString` can remain `0.1.0`, while `CFBundleVersion` uses a monotonically increasing numeric build number.
The current package script copies the Cargo version into both fields, so prerelease version support is an implementation task, not an established capability.

### Hosting and website downloads

Host DMGs and their checksums on [GitHub Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases) in `fontgarden/runebender`.
This keeps binaries with the version tag, source history, release notes, and previous versions.
The website can stay static; no separate file server, object-storage account, or application backend is needed for v0.1.
Do not commit DMGs to either Git repository or use expiring Actions artifacts as public download links.

Keep architecture-specific, versioned filenames.
The current public packager's convention is `Runebender-0.1.0-macos-apple-silicon-notarized.dmg`, with `intel` for the Intel build.
Use the actual tested asset name in the website metadata, not a guessed name or a link to a moving branch.
For example, the intended versioned URL is `https://github.com/fontgarden/runebender/releases/download/v0.1.0/Runebender-0.1.0-macos-apple-silicon-notarized.dmg`.
This example is not an available download until that release is published.

Generate a public `release.json` manifest alongside `SHA256SUMS`.
Its initial schema should include `schema_version`, `version`, `tag`, `source_commit`, `channel`, `notes_url`, and an `artifacts` array with each file's platform, architecture, URL, byte size, SHA-256, and minimum OS version.
Include only successfully validated public artifacts; local build receipts and credentials are not this manifest.
Checksums help verify downloaded bytes; they do not replace Developer ID signing or an updater's signature checks.

The website source lives in the separate `runebender-dot-org` repository and deploys as an Astro site on GitHub Pages.
Add a stable download page with Apple Silicon and, only if supported, Intel choices, installation instructions, release notes, checksums, and the Linux source-install instructions.
Copy the published manifest into a checked-in site data file and review the resulting website change.
Avoid fetching the GitHub API from every visitor's browser or putting an API token into the site.
Verify all asset URLs and hashes before deploying, build the site, and check its public download links afterward.
Update the homepage, `src/content/docs/install.mdx`, and relevant source links together; they currently advertise the old unpinned Git installation.
The browser editor is a separate artifact and must not be presented as the same build unless it was independently built and validated for that release.

### Linux and crates.io

For the first release, test a pinned Git installation before advertising it:

```sh
# Future release example: requires the published v0.1.0 tag.
rustup toolchain install 1.96
cargo +1.96 install --locked --git https://github.com/fontgarden/runebender \
  --tag v0.1.0 --bin runebender runebender
```

Before the tag exists, use `--rev FULL_RELEASE_SHA` in a clean installation test.
The explicit toolchain reflects the current pinned Rust version; update the instructions when that requirement changes.
Cargo can install from Git even while the package has `publish = false`.
Publish distro-specific system-library prerequisites rather than implying that Cargo supplies graphics, window-system, or dialog dependencies.
Use `.github/workflows/ci.yml` as the current Ubuntu dependency inventory, add compiler/linker and `pkg-config` prerequisites, and verify the resulting instructions on a clean Linux environment.

Test outside the developer checkout with a separate installation root, then remove access to the source/build directory before launching the installed executable.
In particular, `sample_shapes_ufo()` currently falls back to `CARGO_MANIFEST_DIR` for `Shapes.ufo`, while Cargo installs only the executable.
Decide how the sample asset is installed or embedded, or clearly limit that optional feature; do not claim parity with the macOS bundle until this is resolved.
Test the advertised editing workflow on a Linux desktop; headless CI does not establish native dialog, input, or GPU behavior.

`cargo install runebender --locked` from crates.io is a separate milestone.
The current manifests explicitly disable publishing, use the path-only `nufo` package, and depend on Git versions of Xilem/Masonry, Babelfont, NeuralType, img2bez, and spline.
[Cargo's registry dependency rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations) require publishable registry dependencies rather than Git/path-only normal dependencies.
Adding an unrelated registry version as a fallback does not establish that it preserves the fork's behavior, including the current Metal workaround.

The public crates.io API returned no packages for `runebender`, `nufo`, or `img2bez` on October 17, 2026.
That is a point-in-time availability check, not a reservation; check names and ownership again before publication.
Inventory transitive dependencies, arrange upstream releases or deliberately maintained publishable packages, publish local dependencies in dependency order, and replace Git/path-only dependencies with compatible registry versions.
Review packaged files and licenses, retain the lockfile, and pass `cargo publish --dry-run --locked` plus a clean package-install test before enabling publication.
After publication, verify the documented registry install independently.
Retain Eli's individual crates.io ownership and add an appropriate Font Garden GitHub team owner if desired; moving a GitHub repository does not establish crates.io ownership.

### Changelog and updates

Maintain [CHANGELOG.md](CHANGELOG.md) with an `Unreleased` section and concise user-facing Added, Changed, Fixed, and Known limitations entries as applicable.
At release preparation, move the reviewed entries into a dated version section and start a fresh `Unreleased` section.
Use that version section for the GitHub Release body and website notes, including installation requirements, tested platforms, source-format caveats, and separately installed AI tools.
Generated commit lists may supplement these notes but should not be the main changelog.
Do not invent historical release dates or imply that the Cargo version means a public release already exists.

For v0.1, document manual updates: quit Runebender, download the newer DMG, replace the app in Applications, eject, and relaunch.
Add a visible application version and a menu action such as “Downloads and Release Notes…” that opens the website; this menu action is still to be implemented.
This provides a discoverable update route without claiming that an in-app version checker already exists.
Keep preferences outside the signed app bundle and test replacement without losing them.

For automatic macOS updates, evaluate [Sparkle 2](https://sparkle-project.org/documentation/) after the basic signed release workflow works.
Use its archive signatures and appcast rather than a custom executable downloader.
Store the separate EdDSA signing key securely, embed its public key, and test updates between two genuinely signed builds, including failures and offline behavior.
Host the appcast on runebender.org with versioned archives on GitHub Releases.
The Rust/Xilem integration and nested framework/helper signing need a prototype that respects this repository's prohibition on unsafe workspace code.
Do not delay the first DMG on this integration or silently replace installations managed by another package manager.

### Homebrew later

Start with a first-party `fontgarden/homebrew-tap` cask that installs the already notarized DMG, rather than rebuilding the GUI application from source.
The intended command would be `brew install --cask fontgarden/tap/runebender` after the tap exists.
Pin its version, architecture-specific URL, minimum macOS requirement, and SHA-256 according to the [Cask Cookbook](https://docs.brew.sh/Cask-Cookbook).
Test installation, upgrade, and uninstall before advertising it; updates for those installations should go through Homebrew.
Submission to Homebrew's main cask repository can come later.

### Work order toward the first release

- [x] Build and inspect an icon-bearing local DMG, including copied-app headless smoke checks.
- [ ] Complete the manual Finder installation and editing checklist.
- [ ] Repair Linux Clippy and macOS tests, update the browser adapter for the current Xilem pin, and obtain passing checks on the chosen release SHA.
- [ ] Pin or record the font-fixture revision and make external test dependencies explicit.
- [ ] Complete Apple activation and validate Developer ID signing, notarization, stapling, and browser-download Gatekeeper behavior.
- [ ] Implement the small release wrapper, numeric bundle build version, immutable-source checks, draft upload, and public manifest.
- [ ] Curate v0.1 notes, verify Linux installation and runtime assets, and decide the tested OS/architecture support matrix.
- [ ] Stage a release, finish installation review, publish explicitly, verify the public downloads, and deploy the website download page.
- [ ] Follow up separately with registry dependencies/crates.io, a Homebrew tap, and Sparkle.

Aim the first week at the signed Apple Silicon DMG and a reliable source-install route.
If Intel hardware, Linux desktop validation, or Apple activation is unavailable, narrow the advertised support or keep the build a test candidate rather than inventing a passing result.

As of October 17, 2026, [CI at `b338d2a`](https://github.com/fontgarden/runebender/actions/runs/39061911415) passes formatting and advisories but fails Linux Clippy and four macOS proof tests that require an unavailable Designbot executable.
The browser adapter also still expects an older Xilem revision.
Resolve these dependencies deliberately and preserve meaningful coverage; do not disable failing checks to label the release ready.
Windows diagnostics are separate from the supported-platform release gates.


## Local macOS package

Run on macOS with Rust from `rust-toolchain.toml`, Python 3, and Apple's command-line tools installed:

```sh
./scripts/build_dmg.sh
```

This always runs `cargo build --release --locked --bin runebender` before packaging.
It gets the version and output directory from Cargo and the minimum macOS version from the built executable.
The default package targets the current Mac's architecture.
On hosts with less than 24 GiB of memory, the command sets `CARGO_BUILD_JOBS=2`; do not run another Cargo command concurrently on those hosts.

Each build goes into a fresh timestamped directory under `target/macos/` on an Apple Silicon host with Cargo's default target directory.
The directory contains the `.dmg`, `SHA256SUMS`, `build-info.json`, and `volume/Runebender.app`.
Existing output directories are never overwritten; pass `--output /path/to/a/new-directory` for another build.
The app includes Runebender's license files and the license for its embedded Virtua Grotesk font.
Core editing does not require a source checkout or a Rust installation on the recipient's Mac.
Local AI models, training tools, and their runtimes are separate installations.

`--unsigned` uses ad-hoc signing for local testing.
It does not provide a Developer ID identity, notarization, or a public distribution build accepted by Gatekeeper.
Do not publish it as the normal macOS download.

To package another architecture, first install its Rust target and pass it explicitly:

```sh
rustup target add x86_64-apple-darwin
python3 scripts/package_macos.py --unsigned --target x86_64-apple-darwin
```

The supported target names are `aarch64-apple-darwin` and `x86_64-apple-darwin`.
Builds are separate Apple Silicon and Intel downloads; there is no universal binary yet.
Cross-compilation and a minimum OS declaration do not replace testing on that hardware and OS.
Packaging rejects dynamic dependencies outside Apple's system library directories instead of producing an app that depends on a developer's Homebrew installation.

## Apple enrollment and signing setup

Public direct downloads use a **Developer ID Application** certificate and Apple's notarization service.
These require [Apple Developer Program membership](https://developer.apple.com/programs/enroll/), currently US$99 per membership year with regional pricing.
An individual account requires an Apple Account with two-factor authentication and identity verification.
Organization enrollment additionally requires a legal entity, authority to enroll it, and normally a D-U-N-S number.
Enrollment and certificate issuance must be completed before the signed release command can succeed.
Runebender can be distributed directly; a Mac App Store listing is not required.

Follow Apple's [Developer ID guide](https://developer.apple.com/developer-id/) to create a Developer ID Application certificate and install it with its private key in the signing Mac's Keychain.
List available signing identities:

```sh
security find-identity -v -p codesigning
```

Store notarization credentials in the Keychain using the interactive command:

```sh
xcrun notarytool store-credentials runebender-notary
```

Use an authentication method supported by `notarytool`, such as an Apple Account with an app-specific password and the membership's Team ID.
Keep passwords, private keys, exported certificates, and API keys out of the repository and chat.
The packaging command accepts a Keychain profile name rather than credentials.

## Signed public package

```sh
python3 scripts/package_macos.py \
  --identity 'Developer ID Application: YOUR LEGAL NAME (TEAMID)' \
  --notary-profile runebender-notary
```

The command signs the application with the hardened runtime and a secure timestamp, submits an archive to Apple, waits for acceptance, and staples and validates the app's ticket.
It then creates and signs the DMG, submits that DMG, staples its ticket, and assesses both the app and image with Gatekeeper.
Checksums are written only after those checks succeed.
Notarization response JSON is kept alongside the artifact; rejected submissions also request Apple's diagnostic log.
A failed build may leave partial output for diagnosis; choose a new output directory after correcting the error.
No runtime exceptions or App Sandbox entitlements are added by default.
Test the actual signed application, including file dialogs, clipboard, rendering, saving, and any external tools intended for the release.

The default application icon comes from `assets/icons/runebender.png` and is temporary artwork for testing.
Pass `--icon /path/to/Runebender.icns` or `--icon /path/to/Runebender.png` to override it.
Square PNGs must be at least 1024 pixels wide; the package script converts them with Apple's icon tools before signing.
The bundle identifier is `org.runebender.Runebender`; keep it stable across releases.
The application is opened from Applications and fonts are opened through File > Open or a drop onto the editor window.
Finder document associations are not advertised until native open-document event handling has been implemented and tested.

Apple's [packaging guidance](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution) recommends notarizing the outermost distribution container.
The current script also notarizes the standalone app first, so both its staged app and its DMG have stapled tickets; that adds an extra submission.
Review whether to consolidate this after the first signed-path validation, while retaining the offline installed-app test.
The package script uses `syspolicy_check distribution` where available, falling back to `spctl` on older macOS hosts, following [Apple DTS testing guidance](https://developer.apple.com/forums/thread/130560).

## v0.1 release gates

- Choose the supported Mac architectures and test the oldest supported macOS version on each.
- Supply and review the application icon and Finder presentation of the DMG.
- Complete Apple enrollment, install the signing identity, and validate the signed and notarized artifacts.
- Download the final DMG through a browser on a separate Mac or clean user account, retaining quarantine metadata.
  Verify mounting, copying to Applications, ejecting, and launching without a terminal or developer tools.
  Repeat first launch without network access to check stapled tickets.
- Exercise a representative UFO and Designspace: open, edit, undo, save, close, reopen, and compile.
  Use disposable copies and verify that unrelated source data survives the round trip.
- Verify first launch with no source checkout, no local AI runtime, and no developer-specific environment variables.
- Decide whether Finder's Open With and dropping a font onto the app icon are required for v0.1.
  These need native macOS open-document event handling and bundle document declarations.
- Pass the repository's required native CI checks on supported platforms.
  Document unsupported-platform failures honestly without turning off their diagnostic coverage.
- Review third-party license and notice requirements for the binary's complete dependency graph.
  The packaged project and font licenses are a starting point, not a complete dependency-license audit.
- Write release notes covering supported source formats, known limitations, backup advice, and separately installed AI features.
- Publish the validated DMGs and checksums on a tagged GitHub Release, then update the website's installation instructions to link to those artifacts.

Do not create the `v0.1.0` tag or publish a release until these gates have been reviewed.
Signing and notarization have not been validated merely by successfully creating a local test DMG.
