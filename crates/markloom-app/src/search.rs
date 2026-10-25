//! Fuzzy quick-open and workspace text search.

/// One line that contains the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: String,
    pub line: u32,
    pub text: String,
}

/// Rank Markdown paths by a subsequence query. Empty query returns every path.
pub fn quick_open<'a>(query: &str, paths: &'a [String]) -> Vec<&'a str> {
    let mut scored: Vec<(i32, &str)> = paths
        .iter()
        .filter_map(|path| fuzzy_score(query, path).map(|score| (score, path.as_str())))
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(right.1)));
    scored.into_iter().take(20).map(|(_, path)| path).collect()
}

/// Higher is a tighter match. `None` when `query` is not a subsequence.
pub fn fuzzy_score(query: &str, path: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let query: Vec<char> = query.chars().flat_map(|ch| ch.to_lowercase()).collect();
    let path: Vec<char> = path.chars().flat_map(|ch| ch.to_lowercase()).collect();
    let mut score = 1_000;
    let mut from = 0;
    let mut previous = None;
    for needle in query {
        let found = path.iter().skip(from).position(|ch| *ch == needle)?;
        let at = from + found;
        if previous == Some(at - 1) {
            score += 15;
        } else {
            score -= (at - from) as i32;
        }
        if at == 0 || path.get(at - 1).is_some_and(|ch| *ch == '/' || *ch == '-') {
            score += 10;
        }
        previous = Some(at);
        from = at + 1;
    }
    Some(score)
}

/// Case-insensitive substring search. Line numbers are 1-based.
pub fn search_files(query: &str, files: &[(String, String)]) -> Vec<SearchHit> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    for (path, text) in files {
        for (index, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                hits.push(SearchHit {
                    path: path.clone(),
                    line: (index as u32) + 1,
                    text: line.to_string(),
                });
            }
        }
    }
    hits
}
