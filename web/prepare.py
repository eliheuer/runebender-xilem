#!/usr/bin/env python3
"""Prepare a browser-only manifest patch for the pinned Xilem adapter.

The adapter crate `xilem_masonry` is copied from the exact git revision the desktop
build uses, and its manifest is rewritten so it carries no unconditional multi-thread
Tokio runtime, which cannot compile for WebAssembly. Its Rust source is unchanged.
"""
import json, pathlib, re, shutil, subprocess, sys, tomllib
root = pathlib.Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=root))
pkg = next(p for p in metadata['packages'] if p['name'] == 'xilem_masonry')
match = re.fullmatch(r'git\+(?P<url>[^?]+)\?rev=(?P<rev>[0-9a-f]+)#(?P<commit>[0-9a-f]+)', pkg['source'] or '')
assert match, f"xilem_masonry must come from a pinned git revision, not {pkg['source']!r}"
url, rev = match['url'], match['rev']
source = pathlib.Path(pkg['manifest_path']).parent
workspace = tomllib.loads((source.parent / 'Cargo.toml').read_text())
version = workspace['package']['version'] if 'package' in workspace else workspace['workspace']['package']['version']
deps = workspace['workspace']['dependencies']
def plain(name):
    dep = deps[name]
    return dep if isinstance(dep, str) else dep['version']
dest = root / 'web/target/xilem_masonry'
shutil.copytree(source, dest, dirs_exist_ok=True)
# Upstream source is unchanged. Only its unconditional desktop runtime feature is removed.
(dest / 'Cargo.toml').write_text(f'''[package]
name = "xilem_masonry"
version = "{version}"
edition = "2024"
license = "Apache-2.0"
publish = false
[features]
default = []
[dependencies]
xilem_core = {{ git = "{url}", rev = "{rev}" }}
masonry = {{ git = "{url}", rev = "{rev}", default-features = false }}
smallvec = "{plain('smallvec')}"
tracing = "{plain('tracing')}"
tokio = {{ version = "1.50", features = ["rt", "time", "sync"] }}
usvg = "{plain('usvg')}"
''')
# The browser workspace must pin the same revision as the desktop build.
for manifest in (root / 'web/Cargo.toml', root / 'web/compat/Cargo.toml'):
    text = manifest.read_text()
    updated = re.sub(r'masonry = \{ git = "[^"]+", rev = "[0-9a-f]+"', f'masonry = {{ git = "{url}", rev = "{rev}"', text)
    if updated != text:
        manifest.write_text(updated)
        print(f'Pinned {manifest.relative_to(root)} to {url}@{rev[:8]}', file=sys.stderr)
print(f'Prepared pinned Xilem widget adapter {version} from {url}@{rev[:8]} with a single-thread browser runtime')
