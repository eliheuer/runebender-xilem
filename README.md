# runebender-xix

A font editor built on [xix](https://github.com/eliheuer/xix), a fork of
Xilem. This is the third Runebender shell; it shares the editing engine
[runebender-core](https://github.com/eliheuer/runebender-core) with
[runebender-gpui](https://github.com/eliheuer/runebender-gpui) and
[runebender-web](https://github.com/eliheuer/runebender-web).

The port is a test of xix: Runebender is the kind of application xix is
for, and `PORT.md` records what the port forces into the framework.

## Run

```sh
cargo run -- path/to/Font.ufo
```

A `.designspace` opens its first master. Headless screenshot:

```sh
XIX_SCREENSHOT=screenshots/editor.png cargo run -- path/to/Font.ufo
```

To develop against local checkouts of xix and runebender-core, put a
`.cargo/config.toml` in the directory above the repositories with a
`[patch]` section for the two git sources. Nothing in this repository
points at a local path.

## License

Apache-2.0 OR MIT, the Linebender convention.
