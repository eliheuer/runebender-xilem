// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The command line as a script sees it: JSON out, exit codes in.
//!
//! Needs the Virtua Grotesk fixture next to this checkout, or
//! `$RUNEBENDER_TEST_FONTS`. Each test copies the UFO into a fresh
//! temporary directory, so nothing edits the fixture.

use std::path::{Path, PathBuf};
use std::process::Command;

use runebender_core::document::proposal;

fn fixture() -> PathBuf {
    let dir = match std::env::var_os("RUNEBENDER_TEST_FONTS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources"),
    };
    let ufo = dir.join("VirtuaGrotesk-Regular.ufo");
    assert!(
        ufo.is_dir(),
        "fixture fonts not found at {}: clone eliheuer/virtua-grotesk next to this \
         repository, or set RUNEBENDER_TEST_FONTS",
        dir.display()
    );
    ufo
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create");
    for entry in std::fs::read_dir(src).expect("read") {
        let entry = entry.expect("entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// A directory under the system temp dir that goes away when the
/// test drops it.
struct Scratch(PathBuf);

impl Scratch {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A private copy of the fixture, in a directory that goes away
/// with the test.
fn scratch_ufo() -> (Scratch, PathBuf) {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("runebender-cli-test-{}-{n}", std::process::id()));
    let ufo = dir.join("Virtua.ufo");
    copy_dir(&fixture(), &ufo);
    (Scratch(dir), ufo)
}

fn run(args: &[&str]) -> (i32, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
        .arg("--json")
        .args(args)
        .output()
        .expect("the binary runs");
    let code = output.status.code().expect("an exit code");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout
        .lines()
        .last()
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or(serde_json::Value::Null);
    (code, value)
}

#[test]
fn info_reports_the_font_as_json() {
    let (_dir, ufo) = scratch_ufo();
    let (code, out) = run(&["info", ufo.to_str().expect("utf8")]);
    assert_eq!(code, 0);
    assert_eq!(out["ok"], true);
    assert_eq!(out["family"], "Virtua Grotesk");
    assert_eq!(out["unitsPerEm"], 1024.0);
    assert!(out["glyphs"].as_u64().expect("count") > 100);
    assert_eq!(out["proposals"].as_array().expect("list").len(), 0);
}

#[test]
fn a_bad_path_is_a_usage_error() {
    let (code, out) = run(&["info", "/nowhere/Nothing.ufo"]);
    assert_eq!(code, 2);
    assert_eq!(out["ok"], false);
}

#[test]
fn proof_writes_svg_and_metrics() {
    let (dir, ufo) = scratch_ufo();
    let svg = dir.path().join("proof.svg");
    let (code, out) = run(&[
        "proof",
        ufo.to_str().expect("utf8"),
        "--glyphs",
        "H,O",
        "--out",
        svg.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");
    let text = std::fs::read_to_string(&svg).expect("svg written");
    assert!(text.starts_with("<svg"));
    assert_eq!(text.matches("<path ").count(), 2);
    let glyphs = out["glyphs"].as_array().expect("metrics");
    assert_eq!(glyphs[0]["glyph"], "H");
    assert!(glyphs[0]["advance"].as_f64().expect("advance") > 0.0);
    assert!(glyphs[0]["lsb"].is_number());
}

#[test]
fn proposals_list_install_and_discard_through_the_binary() {
    let (_dir, ufo) = scratch_ufo();
    let path = ufo.to_str().expect("utf8");

    // A tool wrote a proposal: H moved right, O with a contour gone.
    let mut font = norad::Font::load(&ufo).expect("loads");
    let mut h = font.get_glyph("H").expect("H").clone();
    let width_before = h.width;
    for c in &mut h.contours {
        for p in &mut c.points {
            p.x += 20.0;
        }
    }
    h.width += 40.0;
    let mut o = font.get_glyph("O").expect("O").clone();
    o.contours.pop();
    proposal::write(&mut font, "bolden", [h, o]).expect("written");
    font.save(&ufo).expect("saved");

    let (code, out) = run(&["proposal", "list", path]);
    assert_eq!(code, 0);
    let list = out["proposals"].as_array().expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["task"], "bolden");
    assert_eq!(list[0]["compatible"], serde_json::json!(["H"]));
    assert_eq!(list[0]["incompatible"].as_array().expect("pairs").len(), 1);

    let (code, out) = run(&["proposal", "install", path, "--task", "bolden"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["installed"]["installed"], serde_json::json!(["H"]));
    assert_eq!(
        out["installed"]["skipped"].as_array().expect("skips").len(),
        1
    );
    assert_eq!(out["installed"]["layer_removed"], false);

    let font = norad::Font::load(&ufo).expect("reloads");
    assert_eq!(font.get_glyph("H").expect("H").width, width_before + 40.0);
    assert_eq!(font.get_glyph("O").expect("O").contours.len(), 2);

    let (code, out) = run(&["proposal", "discard", path, "--task", "bolden"]);
    assert_eq!(code, 0);
    assert_eq!(out["discarded"], 1);
    let (code, out) = run(&["proposal", "discard", path, "--task", "bolden"]);
    assert_eq!(code, 2);
    assert_eq!(out["error"]["kind"], "no_proposal");
}

#[test]
fn propose_without_font_ml_says_so_with_code_3() {
    let (_dir, ufo) = scratch_ufo();
    let output = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
        .args(["--json", "propose", "bolden", ufo.to_str().expect("utf8")])
        .env("RUNEBENDER_FONT_ML", "")
        .env("PATH", "/nonexistent")
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(3));
    let out: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json on stdout");
    assert_eq!(out["ok"], false);
}

#[test]
fn propose_runs_the_tool_and_reports_what_it_left() {
    let (dir, ufo) = scratch_ufo();
    // A stand-in font-ml: prints JSON and writes a proposal layer by
    // hand, the way the real one will.
    let tool = dir.path().join("font-ml");
    std::fs::write(
        &tool,
        "#!/bin/sh\n\
         case \"$1\" in tasks) echo '{\"tasks\":[{\"name\":\"bolden\",\"implemented\":true},{\"name\":\"kerning\",\"implemented\":false}]}'; exit 0;; esac\n\
         case \"$*\" in *--write*) ;; *) echo '{\"error\":\"no --write\"}'; exit 4;; esac\n\
         echo '{\"ok\":true,\"glyph\":\"H\"}'\n\
         exit 0\n",
    )
    .expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let mut font = norad::Font::load(&ufo).expect("loads");
    let h = font.get_glyph("H").expect("H").clone();
    proposal::write(&mut font, "bolden", [h]).expect("written");
    font.save(&ufo).expect("saved");

    let (code, out) = run(&[
        "propose",
        "bolden",
        ufo.to_str().expect("utf8"),
        "--tool",
        tool.to_str().expect("utf8"),
        "--glyphs",
        "H",
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["ok"], true);
    assert_eq!(out["report"]["glyph"], "H");
    assert_eq!(out["proposal"]["task"], "bolden");
    assert_eq!(out["proposal"]["glyphs"], serde_json::json!(["H"]));

    // A task the tool names but has not built is code 3; one it does
    // not name is a usage error that lists what it knows.
    let tool_arg = tool.to_str().expect("utf8");
    let (code, out) = run(&[
        "propose",
        "kerning",
        ufo.to_str().expect("utf8"),
        "--tool",
        tool_arg,
    ]);
    assert_eq!(code, 3, "{out}");
    let (code, out) = run(&[
        "propose",
        "hint",
        ufo.to_str().expect("utf8"),
        "--tool",
        tool_arg,
    ]);
    assert_eq!(code, 2, "{out}");
    assert!(out["error"].as_str().expect("message").contains("bolden"));
}

/// The demo workflow: font and model into bolden, then compare and
/// install. Written here rather than read from runebender-demo, so
/// the test needs nothing beside this checkout.
fn demo_nodes(dir: &Path) -> PathBuf {
    let file = dir.join("bolden.nodes.json");
    std::fs::write(
        &file,
        r#"{
  "version": 1,
  "nodes": [
    { "id": 1, "type": "core.source" },
    { "id": 2, "type": "core.model", "values": { "name": "virtua-12m-bolden" } },
    { "id": 3, "type": "font-ml.bolden" },
    { "id": 4, "type": "core.master", "values": { "name": "Bold" } },
    { "id": 5, "type": "core.compare" },
    { "id": 6, "type": "core.install" }
  ],
  "links": [
    [1, "source", 3, "source"],
    [1, "glyphs", 3, "glyphs"],
    [2, "model", 3, "model"],
    [3, "layer", 5, "layer"],
    [4, "source", 5, "against"],
    [3, "layer", 6, "layer"]
  ]
}"#,
    )
    .expect("write");
    file
}

/// A stand-in font-ml that declares bolden and nothing else.
fn stand_in_font_ml(dir: &Path) -> PathBuf {
    let tool = dir.join("font-ml");
    std::fs::write(
        &tool,
        r#"#!/bin/sh
if [ "$1" = "tasks" ]; then
  echo '{"tasks":[{"name":"bolden","title":"Bolden","help":"","implemented":true,"inputs":[{"name":"source","kind":"source","required":true,"help":""},{"name":"model","kind":"model","required":true,"help":""},{"name":"glyph","kind":"glyphs","required":false,"help":""},{"name":"strength","kind":"number","required":false,"default":1.0,"help":""},{"name":"write","kind":"flag","required":false,"help":""}],"outputs":[{"name":"com.runebender.proposal.bolden","kind":"layer","help":""},{"name":"glyphs","kind":"rows","help":""}]}]}'
  exit 0
fi
exit 4
"#,
    )
    .expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    tool
}

#[test]
fn nodes_check_accepts_the_demo_with_the_tool_present() {
    let (dir, _ufo) = scratch_ufo();
    let file = demo_nodes(dir.path());
    let tool = stand_in_font_ml(dir.path());
    let (code, out) = run(&[
        "nodes",
        "check",
        file.to_str().expect("utf8"),
        "--tool",
        tool.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["ok"], true);
    assert_eq!(out["nodes"], 6);
    assert_eq!(out["problems"].as_array().expect("list").len(), 0);
    let order = out["order"].as_array().expect("order");
    let at = |id: u64| {
        order
            .iter()
            .position(|v| v.as_u64() == Some(id))
            .expect("in order")
    };
    assert!(at(3) > at(1) && at(3) > at(2) && at(5) > at(3) && at(6) > at(3));
}

#[test]
fn nodes_check_names_a_missing_tool_and_a_bad_link() {
    let (dir, _ufo) = scratch_ufo();
    let file = demo_nodes(dir.path());
    let mut text = std::fs::read_to_string(&file).expect("read");
    text = text.replace(
        r#"[1, "glyphs", 3, "glyphs"]"#,
        r#"[1, "glyphs", 3, "model"]"#,
    );
    std::fs::write(&file, text).expect("write");
    // A tool path that does not exist: font-ml.bolden is then unknown.
    let (code, out) = run(&[
        "nodes",
        "check",
        file.to_str().expect("utf8"),
        "--tool",
        "/nowhere/font-ml",
    ]);
    assert_eq!(code, 2);
    assert_eq!(out["ok"], false);
    let problems = out["problems"].as_array().expect("list");
    assert!(problems.iter().any(|p| p["problem"] == "unknown_type"));
    assert!(problems.iter().any(|p| p["problem"] == "double_input"));
}

#[test]
fn nodes_schema_and_types_print() {
    let (code, out) = run(&["nodes", "schema"]);
    assert_eq!(code, 0);
    assert!(out["properties"]["links"].is_object());
    let (code, out) = run(&["nodes", "types", "--tool", "/nowhere/font-ml"]);
    assert_eq!(code, 0);
    let names: Vec<&str> = out["types"]
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"core.source") && names.contains(&"core.install"));
    assert!(out["tool"].is_null());
}

#[test]
fn nodes_run_runs_core_nodes_and_skips_them_the_second_time() {
    let (dir, ufo) = scratch_ufo();
    let file = dir.path().join("proof.nodes.json");
    let svg = dir.path().join("sheet.svg");
    std::fs::write(
        &file,
        format!(
            r#"{{
  "version": 1,
  "nodes": [
    {{ "id": 1, "type": "core.source" }},
    {{ "id": 2, "type": "core.proof", "values": {{ "out": "{}" }} }},
    {{ "id": 3, "type": "core.note", "values": {{ "text": "H and O only" }} }}
  ],
  "links": [[1, "source", 2, "source"]]
}}"#,
            svg.display()
        ),
    )
    .expect("write");
    let args = [
        "nodes",
        "run",
        file.to_str().expect("utf8"),
        "--font",
        ufo.to_str().expect("utf8"),
        "--glyphs",
        "H,O",
        "--tool",
        "/nowhere/font-ml",
    ];
    let (code, out) = run(&args);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["ok"], true);
    let nodes = out["nodes"].as_array().expect("nodes");
    assert_eq!(nodes.len(), 3);
    assert!(nodes.iter().all(|n| n["status"] == "ran"), "{out}");
    assert!(svg.is_file());
    let text = std::fs::read_to_string(&svg).expect("svg");
    assert!(
        text.matches("<path ").count() > 2,
        "every drawn glyph, not the selection"
    );

    // Nothing changed: the proof is skipped. The cache sits beside
    // the file.
    assert!(dir.path().join(".proof.nodes.json.cache").is_file());
    let (code, out) = run(&args);
    assert_eq!(code, 0, "{out}");
    let nodes = out["nodes"].as_array().expect("nodes");
    let proof = nodes.iter().find(|n| n["id"] == 2).expect("proof node");
    assert_eq!(proof["status"], "skipped", "{out}");

    // Edit a glyph in the font: the proof runs again.
    let glif = ufo.join("glyphs").join("H_.glif");
    let mut text = std::fs::read_to_string(&glif).expect("glif");
    text = text.replacen("<point x=\"", "<point x=\"1", 1);
    std::fs::write(&glif, text).expect("write glif");
    let (code, out) = run(&args);
    assert_eq!(code, 0, "{out}");
    let nodes = out["nodes"].as_array().expect("nodes");
    let proof = nodes.iter().find(|n| n["id"] == 2).expect("proof node");
    assert_eq!(proof["status"], "ran", "{out}");
}

/// Drives the MCP server over its stdio: one JSON-RPC message per
/// line in, one per line out.
fn mcp_session(font: &Path, tool: &Path, requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    use std::io::{BufRead as _, BufReader, Write as _};
    let mut child = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
        .arg("mcp")
        .arg("--font")
        .arg(font)
        .arg("--tool")
        .arg(tool)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the server starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut lines = BufReader::new(stdout).lines();
    let mut replies = Vec::new();
    for request in requests {
        writeln!(stdin, "{request}").expect("write");
        stdin.flush().expect("flush");
        // A notification (no id) gets no reply.
        if request.get("id").is_some() {
            let line = lines.next().expect("a reply").expect("a line");
            replies.push(serde_json::from_str(&line).expect("json"));
        }
    }
    drop(stdin);
    let _ = child.wait();
    replies
}

#[test]
fn mcp_lists_the_agent_tools_and_calls_them() {
    let (dir, ufo) = scratch_ufo();
    let tool = stand_in_font_ml(dir.path());
    let replies = mcp_session(
        &ufo,
        &tool,
        &[
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {},
                            "clientInfo": { "name": "test", "version": "0" } } }),
            serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            serde_json::json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "font_info", "arguments": {} } }),
            serde_json::json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "propose",
                            "arguments": { "task": "bolden", "model": "/nowhere", "glyphs": ["H"] } } }),
            serde_json::json!({ "jsonrpc": "2.0", "id": 5, "method": "nothing/here" }),
        ],
    );
    assert_eq!(replies.len(), 5);
    let init = &replies[0]["result"];
    assert_eq!(init["protocolVersion"], "2024-11-05");
    assert_eq!(init["serverInfo"]["name"], "runebender-core");
    assert!(
        init["instructions"]
            .as_str()
            .expect("text")
            .contains("No tool edits the font")
    );

    let tools = replies[1]["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"font_info") && names.contains(&"propose"));
    assert!(
        !names.iter().any(|n| n.contains("install")),
        "install is not a tool"
    );
    assert!(tools[0]["inputSchema"]["type"] == "object");

    // font_info: the result is the info JSON as text.
    let info = &replies[2]["result"];
    assert_eq!(info["isError"], false);
    let text = info["content"][0]["text"].as_str().expect("text");
    let parsed: serde_json::Value = serde_json::from_str(text).expect("json in text");
    assert_eq!(parsed["family"], "Virtua Grotesk");

    // propose through the stand-in, which exits 4: reported, not crashed.
    let propose = &replies[3]["result"];
    assert_eq!(propose["isError"], true, "{propose}");

    // An unknown method is a JSON-RPC error, not a dropped line.
    assert_eq!(replies[4]["error"]["code"], -32601);
}

#[test]
fn compose_derives_marks_and_the_result_shapes() {
    use runebender_core::text::shape::{ShapingFont, ShapingGlyph, ShapingSource};
    let (_dir, ufo) = scratch_ufo();
    let path = ufo.to_str().expect("utf8");
    // Report first: a Latin accent and an Arabic hamza both derive
    // from their decompositions; a positional form from its stem.
    let (code, out) = run(&[
        "compose",
        path,
        "--glyphs",
        "Aacute,alefHamzaabove-ar,alefHamzaabove-ar.fina",
    ]);
    assert_eq!(code, 0, "{out}");
    let derived = out["derived"].as_array().expect("derived");
    assert_eq!(derived.len(), 3, "{out}");
    let by_name = |n: &str| derived.iter().find(|d| d["glyph"] == n).expect(n).clone();
    assert_eq!(by_name("Aacute")["recipe"]["base"], "A");
    assert_eq!(by_name("alefHamzaabove-ar")["recipe"]["source"], "unicode");
    assert_eq!(
        by_name("alefHamzaabove-ar.fina")["recipe"]["source"],
        "name"
    );
    assert_eq!(
        by_name("alefHamzaabove-ar.fina")["recipe"]["base"],
        "alef-ar.fina"
    );
    assert!(out["proposal"].is_null(), "no --write, no layer");

    // Write the whole font's recipes and install them.
    let (code, out) = run(&["compose", path, "--write"]);
    assert_eq!(code, 0, "{out}");
    let proposed = out["proposed"].as_array().expect("proposed").len();
    assert!(proposed > 100, "{proposed} proposed");
    assert!(out["skipped"].as_array().expect("skipped").len() < proposed / 2);
    let (code, out) = run(&["proposal", "list", path]);
    assert_eq!(code, 0);
    assert!(out.to_string().contains("\"compose\""), "{out}");
    let (code, out) = run(&[
        "proposal",
        "install",
        path,
        "--task",
        "compose",
        "--any-structure",
    ]);
    assert_eq!(code, 0, "{out}");

    // The installed composites shape: U+0623 is the derived glyph, and
    // in a word its final form, which was derived by name.
    let font = norad::Font::load(&ufo).expect("loads");
    let features = std::fs::read_to_string(ufo.join("features.fea")).expect("fea");
    let order: Vec<String> = std::iter::once(".notdef".to_string())
        .chain(
            font.default_layer()
                .iter()
                .map(|g| g.name().to_string())
                .filter(|n| n != ".notdef"),
        )
        .collect();
    let glyphs = order
        .iter()
        .map(|name| {
            let g = font.get_glyph(name.as_str());
            ShapingGlyph {
                name: name.clone(),
                advance: g.map_or(0.0, |g| g.width),
                unicodes: g
                    .map(|g| g.codepoints.iter().map(|c| c as u32).collect())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let shaper = ShapingFont::build(&ShapingSource {
        units_per_em: 1024.0,
        glyphs,
        features,
    })
    .expect("shaping font builds");
    let names = |text: &str| -> Vec<String> {
        shaper
            .shape(text, true)
            .expect("shapes")
            .iter()
            .map(|g| shaper.glyph_name(g.glyph_id).unwrap_or("?").to_string())
            .collect()
    };
    assert_eq!(names("\u{0623}"), ["alefHamzaabove-ar"]);
    let word = names("\u{0628}\u{0623}");
    assert!(
        word.contains(&"alefHamzaabove-ar.fina".to_string()),
        "{word:?}"
    );
    let installed = font.get_glyph("alefHamzaabove-ar.fina").expect("installed");
    assert_eq!(installed.components[0].base.as_str(), "alef-ar.fina");
}

fn call_agent(font: &Path, name: &str, args: serde_json::Value) -> serde_json::Value {
    run(&[
        "agent",
        "call",
        name,
        "--font",
        font.to_str().unwrap(),
        "--args",
        &args.to_string(),
    ])
    .1
}

#[test]
fn exact_edits_round_trip_without_rewriting_foreground() {
    let (dir, ufo) = scratch_ufo();
    let font = norad::Font::load(&ufo).unwrap();
    let glif = ufo
        .join(font.default_layer().path())
        .join(font.default_layer().get_path("n").unwrap());
    let before = std::fs::read(&glif).unwrap();
    let read = call_agent(&ufo, "read_glyph", serde_json::json!({"glyph": "n"}));
    assert_eq!(read["ok"], true, "{read}");
    let width = read["result"]["advance"].as_f64().unwrap();
    let batch = serde_json::json!({"task": "spacing-test", "reason": "Compare a wider right sidebearing",
        "edits": [{"glyph": "n", "expected_revision": read["result"]["revision"],
        "operations": [{"op": "set_width", "width": width + 12.0}]}]});
    let result = call_agent(&ufo, "propose_edits", batch.clone());
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(std::fs::read(&glif).unwrap(), before);
    let reloaded = norad::Font::load(&ufo).unwrap();
    assert_eq!(reloaded.get_glyph("n").unwrap().width, width);
    assert_eq!(
        reloaded
            .layers
            .get("com.runebender.proposal.spacing-test")
            .unwrap()
            .get_glyph("n")
            .unwrap()
            .width,
        width + 12.0
    );
    assert_eq!(call_agent(&ufo, "propose_edits", batch)["ok"], false);
    let proof = call_agent(
        &ufo,
        "proof",
        serde_json::json!({"glyphs": ["n"], "layer": "com.runebender.proposal.spacing-test"}),
    );
    assert_eq!(proof["ok"], true, "{proof}");
    assert_eq!(proof["result"]["glyphs"][0]["advance"], width + 12.0);
    assert!(
        proof["result"]["svg_content"]
            .as_str()
            .unwrap()
            .contains("<text")
    );
    let _ = std::fs::remove_file(proof["result"]["svg"].as_str().unwrap());
    let svg = dir.path().join("proposal.svg");
    assert_eq!(
        run(&[
            "proof",
            ufo.to_str().unwrap(),
            "--layer",
            "com.runebender.proposal.spacing-test",
            "--out",
            svg.to_str().unwrap()
        ])
        .0,
        0
    );
    let installed = run(&[
        "proposal",
        "install",
        ufo.to_str().unwrap(),
        "--task",
        "spacing-test",
    ]);
    assert_eq!(
        installed.1["installed"]["installed"],
        serde_json::json!(["n"])
    );
}

#[test]
fn agent_rejects_foreground_workflows_and_malformed_scope() {
    let (dir, ufo) = scratch_ufo();
    let file = demo_nodes(dir.path());
    let result = call_agent(&ufo, "nodes_run", serde_json::json!({"file": file}));
    assert_eq!(result["ok"], false, "{result}");
    assert!(
        result["result"]["error"]
            .as_str()
            .unwrap()
            .contains("proposal-only")
    );
    assert_eq!(
        call_agent(&ufo, "proof", serde_json::json!({"glyphs": [42]}))["ok"],
        false
    );
    assert_eq!(
        call_agent(
            &ufo,
            "read_glyph",
            serde_json::json!({"glyph": "n", "master": "bad"})
        )["ok"],
        false
    );
}

#[test]
fn family_requires_explicit_master_and_reports_sources() {
    let (dir, ufo) = scratch_ufo();
    let file = dir.path().join("Family.designspace");
    let second = dir.path().join("Second.ufo");
    copy_dir(&ufo, &second);
    std::fs::write(&file, r#"<?xml version="1.0"?><designspace format="5.0"><axes><axis tag="wght" name="Weight" minimum="100" maximum="900" default="100"/></axes><sources><source filename="Virtua.ufo" name="one" stylename="One"><location><dimension name="Weight" xvalue="100"/></location></source><source filename="Second.ufo" name="two" stylename="Two"><location><dimension name="Weight" xvalue="900"/></location></source></sources></designspace>"#).unwrap();
    let info = call_agent(&file, "project_info", serde_json::json!({}));
    assert_eq!(info["result"]["masters"].as_array().unwrap().len(), 2);
    assert_eq!(
        call_agent(&file, "read_glyph", serde_json::json!({"glyph": "n"}))["ok"],
        false
    );
    let read = call_agent(
        &file,
        "read_glyph",
        serde_json::json!({"glyph": "n", "master": 1}),
    );
    assert_eq!(read["ok"], true, "{read}");
    assert_eq!(read["result"]["master"], 1);
    assert_eq!(read["result"]["source"], second.to_str().unwrap());
}
