//! Word counts and the Insert menu snippets.

/// Words and lines shown in the editor status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStats {
    pub words: usize,
    pub lines: usize,
}

/// Words are whitespace-separated. Lines are `\n` plus one.
pub fn stats(text: &str) -> TextStats {
    TextStats {
        words: text.split_whitespace().count(),
        lines: text.matches('\n').count() + 1,
    }
}

/// A block the Insert menu can drop at the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snippet {
    Table,
    CodeBlock,
    Mermaid,
    Image,
    Quote,
    Rule,
}

/// Snippet text, starting and ending with a blank line, and the placeholder range.
pub fn snippet(kind: Snippet) -> (String, std::ops::Range<usize>) {
    let (body, placeholder) = match kind {
        Snippet::Table => (
            "| Column | Column |\n|---|---|\n| Cell | Cell |\n| Cell | Cell |",
            "Column",
        ),
        Snippet::CodeBlock => ("```text\ncode\n```", "code"),
        Snippet::Mermaid => (
            "```mermaid\nflowchart LR\n  A[Start] --> B[End]\n```",
            "Start",
        ),
        Snippet::Image => ("![alt text](path/to/image.png)", "alt text"),
        Snippet::Quote => ("> Quote", "Quote"),
        Snippet::Rule => ("---", ""),
    };
    let text = format!("\n{body}\n\n");
    let start = if placeholder.is_empty() {
        text.find("---").unwrap_or(0)
    } else {
        text.find(placeholder).unwrap_or(0)
    };
    (text, start..start + placeholder.len())
}
