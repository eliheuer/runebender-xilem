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

const PROMPTS: &[Prompt] = &[
    Prompt {
        name: "review-glyph",
        arguments: &[
            ("glyph", "Glyph name to review", true),
            ("source", "Stable source id from project_info", false),
        ],
        file: include_str!("../../skills/runebender-review-glyph/SKILL.md"),
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
            assert!(
                front.contains(&format!("name: runebender-{}", prompt.name)),
                "{} skill name must be runebender-{}",
                prompt.name,
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
    fn get_renders_arguments_and_requires_the_required_ones() {
        let value = get("review-glyph", &json!({"glyph": "H"})).unwrap();
        let text = value["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("Glyph: H."));
        assert!(text.contains("Source id: not given."));
        assert!(!text.contains("{{"));
        assert!(get("review-glyph", &json!({})).is_err());
        assert!(get("nothing", &json!({})).is_err());
        let listed = list();
        assert_eq!(listed["prompts"].as_array().unwrap().len(), prompts().len());
        assert_eq!(listed["prompts"][0]["arguments"][0]["required"], true);
    }
}
