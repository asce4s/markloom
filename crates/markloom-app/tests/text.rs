use markloom_app::text::{self, Snippet};

#[test]
fn stats_counts() {
    let counted = text::stats("a b\n\nc");
    assert_eq!(counted.words, 3);
    assert_eq!(counted.lines, 3);
    let empty = text::stats("");
    assert_eq!(empty.words, 0);
    assert_eq!(empty.lines, 1);
}

#[test]
fn snippets_placeholders() {
    let cases = [
        (Snippet::Table, "Column"),
        (Snippet::CodeBlock, "code"),
        (Snippet::Mermaid, "Start"),
        (Snippet::Image, "alt text"),
        (Snippet::Quote, "Quote"),
        (Snippet::Rule, ""),
    ];
    for (kind, placeholder) in cases {
        let (body, range) = text::snippet(kind);
        assert!(
            body.starts_with('\n') && body.ends_with("\n\n"),
            "{kind:?} should start and end with a blank line: {body:?}"
        );
        assert_eq!(&body[range], placeholder, "{kind:?}");
    }
}
