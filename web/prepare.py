#!/usr/bin/env python3
"""Prepare a browser-only manifest patch for the pinned upstream Xilem adapter."""
import json, pathlib, shutil, subprocess
root = pathlib.Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=root))
pkg = next(p for p in metadata['packages'] if p['name'] == 'xilem_masonry')
assert pkg['source'].startswith('git+https://github.com/linebender/xilem?rev=b81d8d7a#b81d8d7a'), 'Update the browser adapter when changing the Xilem pin'
source = pathlib.Path(pkg['manifest_path']).parent
dest = root / 'web/target/xilem_masonry'
shutil.copytree(source, dest, dirs_exist_ok=True)
# Upstream source is unchanged. Only its unconditional desktop runtime feature is removed.
(dest / 'Cargo.toml').write_text('''[package]
name = "xilem_masonry"
version = "0.4.0"
edition = "2024"
license = "Apache-2.0"
[features]
default = []
[dependencies]
xilem_core = { git = "https://github.com/linebender/xilem", rev = "b81d8d7a" }
masonry = { git = "https://github.com/linebender/xilem", rev = "b81d8d7a", default-features = false }
smallvec = "1.15.1"
tracing = "0.1"
tokio = { version = "1.50", features = ["rt", "time", "sync"] }
usvg = "0.47.0"
''')
print('Prepared pinned Xilem widget adapter with a single-thread browser runtime')
