// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Named workflows for the live MCP host.
//!
//! Each workflow is one `skills/<name>/SKILL.md` file. The same text is served
//! as an MCP prompt, so `prompts/list` shows it in any client, and the
//! directory can be linked into a client's skills folder. The tools do the
//! work; these say in what order and where the designer's approval sits.

use serde_json::{Value, json};

/// One workflow, read from its skill file at compile time.
#[derive(Debug)]
pub struct Prompt {
    /// The MCP prompt name.
    pub name: &'static str,
    /// Named arguments spliced into `{{name}}` placeholders, with a
    /// description and whether the client must supply them.
    pub arguments: &'static [(&'static str, &'static str, bool)],
    file: &'static str,
}

/// The tool contract these skills are written for. A skill file that
/// names a different surface is refused, so an old skill cannot drive a
/// host whose tools changed under it.
pub const SURFACE: &str = "runebender-live-v1";

const PROMPTS: &[Prompt] = &[
    Prompt {
        name: "runebender",
        arguments: &[],
        file: include_str!("../../skills/runebender/SKILL.md"),
    },
    Prompt {
        name: "review-glyph",
        arguments: &[
            ("glyph", "Glyph name to review", true),
            ("source", "Stable source id from project_info", false),
        ],
        file: include_str!("../../skills/runebender-review-glyph/SKILL.md"),
    },
    Prompt {
        name: "triage-font",
        arguments: &[
            ("source", "Stable source id from project_info", false),
            ("limit", "Names per list, 1 to 100", false),
        ],
        file: include_str!("../../skills/runebender-triage-font/SKILL.md"),
    },
    Prompt {
        name: "compatibility-check",
        arguments: &[
            ("glyphs", "Glyph names, or 'all'", true),
            ("source", "Reference source id", false),
        ],
        file: include_str!("../../skills/runebender-compatibility-check/SKILL.md"),
    },
    Prompt {
        name: "spacing-pass",
        arguments: &[
            ("glyphs", "Glyph names to space", true),
            ("source", "Stable source id", false),
        ],
        file: include_str!("../../skills/runebender-spacing-pass/SKILL.md"),
    },
    Prompt {
        name: "kerning-pass",
        arguments: &[
            ("pairs", "Pairs such as AV, To, or a line of text", true),
            ("source", "Stable source id", false),
        ],
        file: include_str!("../../skills/runebender-kerning-pass/SKILL.md"),
    },
    Prompt {
        name: "draft-weight",
        arguments: &[
            ("glyphs", "Glyph names to draft", true),
            ("from", "Approved source id to draft from", true),
            ("into", "Target source id", true),
        ],
        file: include_str!("../../skills/runebender-draft-weight/SKILL.md"),
    },
];

/// Every workflow, in the order clients list them.
pub fn prompts() -> &'static [Prompt] {
    PROMPTS
}

impl Prompt {
    /// The one line description from the skill file's front matter.
    pub fn description(&self) -> &'static str {
        front_matter(self.file)
            .lines()
            .find_map(|line| line.strip_prefix("description:"))
            .map(str::trim)
            .unwrap_or_default()
    }

    /// The `surface:` line from the front matter, if any.
    pub fn surface(&self) -> Option<&'static str> {
        front_matter(self.file)
            .lines()
            .find_map(|line| line.strip_prefix("surface:"))
            .map(str::trim)
    }

    /// The workflow text after the front matter.
    pub fn body(&self) -> &'static str {
        body(self.file)
    }

    /// The workflow text with the supplied arguments spliced in.
    pub fn render(&self, arguments: &Value) -> Result<String, String> {
        let mut text = self.body().to_owned();
        for (name, _, required) in self.arguments {
            let value = arguments.get(name).and_then(Value::as_str);
            match value {
                Some(value) => text = text.replace(&format!("{{{{{name}}}}}"), value),
                None if *required => return Err(format!("argument {name} is required")),
                None => text = text.replace(&format!("{{{{{name}}}}}"), "not given"),
            }
        }
        Ok(text)
    }
}

fn front_matter(file: &str) -> &str {
    let Some(rest) = file.strip_prefix("---\n") else {
        return "";
    };
    rest.split("\n---\n").next().unwrap_or_default()
}

fn body(file: &str) -> &str {
    match file.strip_prefix("---\n") {
        Some(rest) => rest
            .split_once("\n---\n")
            .map_or(file, |(_, body)| body.trim_start()),
        None => file,
    }
}

/// A chat line that names a workflow, `/review-glyph H` or
/// `/draft-weight n o from=0 into=1`, rendered to the workflow text, so the
/// in-editor chat sends the same words a Claude client gets. Positional
/// words fill the arguments in declared order; the last positional takes
/// the rest of the line, so `/spacing-pass n o p` spaces three glyphs.
/// `name=value` sets one by name. `None` means the line is not a command.
pub fn from_slash(line: &str) -> Option<Result<String, String>> {
    let line = line.trim();
    let rest = line.strip_prefix('/')?;
    let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let prompt = PROMPTS.iter().find(|prompt| prompt.name == name)?;
    let mut named = serde_json::Map::new();
    let mut positional: Vec<&str> = Vec::new();
    for word in args.split_whitespace() {
        match word.split_once('=') {
            Some((key, value)) if prompt.arguments.iter().any(|(n, _, _)| *n == key) => {
                named.insert(key.into(), Value::String(value.into()));
            }
            _ => positional.push(word),
        }
    }
    let open: Vec<&str> = prompt
        .arguments
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| !named.contains_key(*n))
        .collect();
    for (index, slot) in open.iter().enumerate() {
        if index >= positional.len() {
            break;
        }
        let value = if index + 1 == open.len() {
            positional[index..].join(" ")
        } else {
            positional[index].to_owned()
        };
        named.insert((*slot).into(), Value::String(value));
    }
    Some(get(name, &Value::Object(named)).map(|value| {
        value["messages"][0]["content"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }))
}

/// The `prompts/list` result.
pub fn list() -> Value {
    json!({
        "prompts": PROMPTS.iter().map(|prompt| json!({
            "name": prompt.name,
            "description": prompt.description(),
            "arguments": prompt.arguments.iter().map(|(name, description, required)| json!({
                "name": name, "description": description, "required": required
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>()
    })
}

/// The `prompts/get` result for one named workflow.
pub fn get(name: &str, arguments: &Value) -> Result<Value, String> {
    let prompt = PROMPTS
        .iter()
        .find(|prompt| prompt.name == name)
        .ok_or_else(|| format!("unknown prompt: {name}"))?;
    if prompt.surface() != Some(SURFACE) {
        return Err(format!(
            "prompt {name} is written for surface {}, this host serves {SURFACE}",
            prompt.surface().unwrap_or("none")
        ));
    }
    let text = prompt.render(arguments)?;
    Ok(json!({
        "description": prompt.description(),
        "messages": [{"role": "user", "content": {"type": "text", "text": text}}],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_file_has_front_matter_and_declares_its_placeholders() {
        for prompt in prompts() {
            assert!(
                !prompt.description().is_empty(),
                "{} needs a description",
                prompt.name
            );
            let front = front_matter(prompt.file);
            let expected = if prompt.name == "runebender" {
                "name: runebender\n".to_owned()
            } else {
                format!("name: runebender-{}\n", prompt.name)
            };
            assert!(
                format!("{front}\n").contains(&expected),
                "{} skill name must be {expected}",
                prompt.name
            );
            assert_eq!(
                prompt.surface(),
                Some(SURFACE),
                "{} must declare surface: {SURFACE}",
                prompt.name
            );
            let body = prompt.body();
            assert!(
                body.starts_with("# "),
                "{} body starts at the title",
                prompt.name
            );
            for (name, _, _) in prompt.arguments {
                assert!(
                    body.contains(&format!("{{{{{name}}}}}")),
                    "{} never uses {{{{{name}}}}}",
                    prompt.name
                );
            }
            let mut rest = body;
            while let Some(start) = rest.find("{{") {
                let after = &rest[start + 2..];
                let end = after.find("}}").expect("closed placeholder");
                let placeholder = &after[..end];
                assert!(
                    prompt
                        .arguments
                        .iter()
                        .any(|(name, _, _)| *name == placeholder),
                    "{} uses undeclared placeholder {placeholder}",
                    prompt.name
                );
                rest = &after[end + 2..];
            }
        }
    }

    #[test]
    fn slash_lines_render_workflows_and_other_lines_pass_through() {
        assert!(from_slash("hello").is_none());
        assert!(from_slash("/nothing H").is_none());
        let text = from_slash("/review-glyph H").unwrap().unwrap();
        assert!(text.contains("Glyph: H. Source id: not given."));
        let text = from_slash("  /review-glyph H 1 ").unwrap().unwrap();
        assert!(text.contains("Glyph: H. Source id: 1."));
        let text = from_slash("/spacing-pass n o p source=0").unwrap().unwrap();
        assert!(text.contains("Glyphs: n o p."), "{text}");
        assert!(text.contains("Source id: 0."), "{text}");
        let text = from_slash("/draft-weight n o from=0 into=1")
            .unwrap()
            .unwrap();
        assert!(
            text.contains("Glyphs: n o. From source id: 0. Into source id: 1."),
            "{text}"
        );
        assert!(from_slash("/draft-weight n").unwrap().is_err());
        assert!(
            from_slash("/runebender")
                .unwrap()
                .unwrap()
                .starts_with("# Runebender")
        );
    }

    #[test]
    fn get_renders_arguments_and_requires_the_required_ones() {
        let value = get("review-glyph", &json!({"glyph": "H"})).unwrap();
        let text = value["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("Glyph: H."));
        assert!(text.contains("Source id: not given."));
        assert!(!text.contains("{{"));
        assert!(get("review-glyph", &json!({})).is_err());
        assert!(get("nothing", &json!({})).is_err());
        let hub = get("runebender", &json!({})).unwrap();
        let text = hub["messages"][0]["content"]["text"].as_str().unwrap();
        for name in prompts()
            .iter()
            .map(|p| p.name)
            .filter(|n| *n != "runebender")
        {
            assert!(
                text.contains(&format!("runebender-{name}")),
                "hub must route to {name}"
            );
        }
        let listed = list();
        assert_eq!(listed["prompts"].as_array().unwrap().len(), prompts().len());
        assert_eq!(listed["prompts"][0]["name"], "runebender");
        assert_eq!(listed["prompts"][1]["arguments"][0]["required"], true);
    }
}
