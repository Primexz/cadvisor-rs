//! Central escaping and validation for Prometheus label sets.

use std::{collections::BTreeSet, fmt::Write, io};

use crate::model::Container;

/// Validated allowlist of Docker label keys and Prometheus-safe output names.
#[derive(Clone, Debug, Default)]
pub struct Labels(Vec<(String, String)>);

impl Labels {
    /// Normalize Docker label keys and reject collisions after normalization.
    ///
    /// # Errors
    /// Returns an error for empty keys or duplicate normalized output names.
    pub fn new(keys: &[String]) -> io::Result<Self> {
        let mut seen = BTreeSet::new();
        let mut labels = Vec::new();
        for key in keys {
            let normalized = format!(
                "container_label_{}",
                key.chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect::<String>()
            );
            if key.is_empty() || !seen.insert(normalized.clone()) {
                return Err(io::Error::other(
                    "Docker label keys must be nonempty and unique after Prometheus normalization",
                ));
            }
            labels.push((key.clone(), normalized));
        }
        Ok(Self(labels))
    }

    pub(super) fn write(
        &self,
        out: &mut String,
        container: &Container,
        extra: Option<(&str, &str)>,
    ) {
        if let Some(extra) = extra {
            self.write_many(out, container, &[extra]);
        } else {
            self.write_many(out, container, &[]);
        }
    }

    pub(super) fn write_many(
        &self,
        out: &mut String,
        container: &Container,
        extra: &[(&str, &str)],
    ) {
        out.push('{');
        pair(out, "id", &container.id);
        out.push(',');
        pair(out, "name", &container.metadata.name);
        out.push(',');
        pair(out, "image", &container.metadata.image);
        for (key, normalized) in &self.0 {
            out.push(',');
            pair(
                out,
                normalized,
                container
                    .metadata
                    .labels
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or(""),
            );
        }
        for (key, value) in extra {
            out.push(',');
            pair(out, key, value);
        }
        out.push('}');
    }
}

pub(super) fn pair(out: &mut String, key: &str, value: &str) {
    let _ = write!(out, "{key}=\"");
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(character),
        }
    }
    out.push('"');
}
