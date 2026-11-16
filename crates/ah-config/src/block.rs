//! Managed text blocks.
//!
//! The kit only ever edits text between its own markers, e.g.
//!
//! ```text
//! # >>> agent-home >>>
//! export EX_HOME=...
//! # <<< agent-home <<<
//! ```
//!
//! Everything outside the markers is preserved byte-for-byte.

/// Comment syntax used for the marker lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    /// `# >>> name >>>` (shell, TOML, plain text).
    Hash,
    /// `<!-- >>> name >>> -->` (Markdown: does not render as a heading).
    Html,
}

impl CommentStyle {
    pub fn begin(self, name: &str) -> String {
        match self {
            CommentStyle::Hash => format!("# >>> {name} >>>"),
            CommentStyle::Html => format!("<!-- >>> {name} >>> -->"),
        }
    }
    pub fn end(self, name: &str) -> String {
        match self {
            CommentStyle::Hash => format!("# <<< {name} <<<"),
            CommentStyle::Html => format!("<!-- <<< {name} <<< -->"),
        }
    }
}

/// Location of a block inside a text, as byte offsets of whole lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    /// Start of the begin-marker line.
    start: usize,
    /// End of the end-marker line, including its newline if present.
    end: usize,
    /// Start of the body (line after the begin marker).
    body_start: usize,
    /// Start of the end-marker line.
    body_end: usize,
}

fn find(text: &str, style: CommentStyle, name: &str) -> Option<Span> {
    let begin = style.begin(name);
    let end = style.end(name);
    let mut offset = 0;
    let mut start = None;
    let mut body_start = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']).trim();
        if start.is_none() && trimmed == begin {
            start = Some(offset);
            body_start = offset + line.len();
        } else if let Some(s) = start {
            if trimmed == end {
                return Some(Span {
                    start: s,
                    end: offset + line.len(),
                    body_start,
                    body_end: offset,
                });
            }
        }
        offset += line.len();
    }
    None
}

/// Return the body of the named block, if present.
pub fn get(text: &str, style: CommentStyle, name: &str) -> Option<String> {
    find(text, style, name).map(|s| text[s.body_start..s.body_end].to_string())
}

/// Render a full block (markers + body), always newline-terminated.
pub fn render(style: CommentStyle, name: &str, body: &str) -> String {
    let mut out = String::new();
    out.push_str(&style.begin(name));
    out.push('\n');
    out.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&style.end(name));
    out.push('\n');
    out
}

/// Insert the block, or replace the body of an existing one. Text outside the block is untouched.
pub fn upsert(text: &str, style: CommentStyle, name: &str, body: &str) -> String {
    let block = render(style, name, body);
    if let Some(s) = find(text, style, name) {
        let mut out = String::with_capacity(text.len() + block.len());
        out.push_str(&text[..s.start]);
        out.push_str(&block);
        out.push_str(&text[s.end..]);
        return out;
    }
    let mut out = text.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&block);
    out
}

/// Remove the named block (and the blank separator line `upsert` added before it).
pub fn remove(text: &str, style: CommentStyle, name: &str) -> String {
    let Some(s) = find(text, style, name) else {
        return text.to_string();
    };
    let mut start = s.start;
    let before = &text[..start];
    if before.ends_with("\n\n") {
        start -= 1;
    } else if before == "\n" {
        start = 0;
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&text[s.end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_into_empty() {
        let out = upsert("", CommentStyle::Hash, "agent-home", "export A=1");
        assert_eq!(
            out,
            "# >>> agent-home >>>\nexport A=1\n# <<< agent-home <<<\n"
        );
    }

    #[test]
    fn insert_preserves_existing_and_remove_restores() {
        let original = "alias ll='ls -l'\n# user comment\n";
        let out = upsert(original, CommentStyle::Hash, "agent-home", "export A=1\n");
        assert!(out.starts_with(original));
        assert_eq!(remove(&out, CommentStyle::Hash, "agent-home"), original);
    }

    #[test]
    fn replace_is_idempotent_and_in_place() {
        let original = "top\n# >>> agent-home >>>\nold\n# <<< agent-home <<<\nbottom\n";
        let out = upsert(original, CommentStyle::Hash, "agent-home", "new\n");
        assert_eq!(
            out,
            "top\n# >>> agent-home >>>\nnew\n# <<< agent-home <<<\nbottom\n"
        );
        assert_eq!(upsert(&out, CommentStyle::Hash, "agent-home", "new\n"), out);
        assert_eq!(
            get(&out, CommentStyle::Hash, "agent-home").unwrap(),
            "new\n"
        );
    }

    #[test]
    fn html_markers_for_markdown() {
        let out = upsert(
            "# My rules\n",
            CommentStyle::Html,
            "agent-home",
            "@/x/AGENTS.md",
        );
        assert_eq!(
            out,
            "# My rules\n\n<!-- >>> agent-home >>> -->\n@/x/AGENTS.md\n<!-- <<< agent-home <<< -->\n"
        );
        assert_eq!(
            remove(&out, CommentStyle::Html, "agent-home"),
            "# My rules\n"
        );
    }

    #[test]
    fn crlf_lines_are_recognised() {
        let t = "a\r\n# >>> agent-home >>>\r\nx\r\n# <<< agent-home <<<\r\nb\r\n";
        assert_eq!(get(t, CommentStyle::Hash, "agent-home").unwrap(), "x\r\n");
        assert_eq!(remove(t, CommentStyle::Hash, "agent-home"), "a\r\nb\r\n");
    }

    #[test]
    fn remove_missing_is_noop() {
        assert_eq!(remove("abc\n", CommentStyle::Hash, "agent-home"), "abc\n");
    }
}
