use crate::terminal::cell::Cell;

/// Detect URLs in a row of terminal cells.
/// Returns `(col_start, col_end_exclusive, url_string)` tuples.
///
/// Works on column indices (one cell = one column) so multi-byte
/// characters in non-URL cells never cause indexing issues.
pub fn detect_urls(row: &[Cell]) -> Vec<(usize, usize, String)> {
    let len = row.len();
    let mut results = Vec::new();
    let mut i = 0;

    while i < len {
        // Collect ASCII chars starting at `i` to check for URL prefixes.
        let ch = row[i].ch;
        let (prefix_len, added_scheme) = if ch == 'h' && starts_with_at(row, i, "https://") {
            (8, "")
        } else if ch == 'h' && starts_with_at(row, i, "http://") {
            (7, "")
        } else if ch == 'w' && starts_with_at(row, i, "www.") {
            (4, "https://")
        } else {
            i += 1;
            continue;
        };

        let start = i;

        // Extend past the prefix to collect valid URL characters
        let mut end = start;
        while end < len && is_url_char(row[end].ch) {
            end += 1;
        }

        // Strip trailing punctuation that's likely not part of the URL
        while end > start {
            let ch = row[end - 1].ch;
            if matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"') {
                end -= 1;
            } else if ch == ')' {
                // Keep ) if there's a matching ( in the URL
                if cells_contain(row, start, end, '(') {
                    break;
                }
                end -= 1;
            } else {
                break;
            }
        }

        // Must be longer than just the prefix
        if end <= start + prefix_len {
            i = end.max(start + 1);
            continue;
        }

        // Build the URL string from cell characters
        let url_text: String = row[start..end].iter().map(|c| c.ch).collect();

        // Require at least one dot after the scheme for it to look like a real URL
        let after_scheme = &url_text[if url_text.starts_with("https://") {
            8
        } else if url_text.starts_with("http://") {
            7
        } else {
            4 // www.
        }..];

        if after_scheme.contains('.') && after_scheme.len() > 1 {
            let full_url = if added_scheme.is_empty() {
                url_text
            } else {
                format!("{}{}", added_scheme, url_text)
            };
            results.push((start, end, full_url));
        }

        i = end;
    }

    results
}

/// Check if cell characters starting at `col` match `pattern` (ASCII only).
fn starts_with_at(row: &[Cell], col: usize, pattern: &str) -> bool {
    if col + pattern.len() > row.len() {
        return false;
    }
    pattern.bytes().enumerate().all(|(j, b)| {
        let ch = row[col + j].ch;
        ch.is_ascii() && ch as u8 == b
    })
}

/// Check if any cell in `row[start..end]` contains `target`.
fn cells_contain(row: &[Cell], start: usize, end: usize, target: char) -> bool {
    row[start..end].iter().any(|c| c.ch == target)
}

fn is_url_char(ch: char) -> bool {
    ch.is_ascii() && matches!(ch as u8,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'-' | b'.' | b'_' | b'~' | b':' | b'/' | b'?' | b'#'
        | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'('
        | b')' | b'*' | b'+' | b',' | b';' | b'=' | b'%'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(s: &str) -> Vec<Cell> {
        s.chars().map(|ch| Cell::new(ch, Default::default())).collect()
    }

    #[test]
    fn detect_https_url() {
        let row = make_row("visit https://example.com/path end");
        let urls = detect_urls(&row);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].2, "https://example.com/path");
    }

    #[test]
    fn detect_http_url() {
        let row = make_row("http://foo.bar/baz");
        let urls = detect_urls(&row);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].2, "http://foo.bar/baz");
    }

    #[test]
    fn detect_www_prefix() {
        let row = make_row("go to www.example.com now");
        let urls = detect_urls(&row);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].2.starts_with("https://www.example.com"));
    }

    #[test]
    fn trailing_punctuation_stripped() {
        let row = make_row("see https://example.com/page.");
        let urls = detect_urls(&row);
        assert_eq!(urls[0].2, "https://example.com/page");
    }

    #[test]
    fn balanced_parens_kept() {
        let row = make_row("https://en.wikipedia.org/wiki/Rust_(programming_language)");
        let urls = detect_urls(&row);
        assert_eq!(urls[0].2, "https://en.wikipedia.org/wiki/Rust_(programming_language)");
    }

    #[test]
    fn unbalanced_paren_stripped() {
        let row = make_row("(https://example.com/path)");
        let urls = detect_urls(&row);
        // The opening paren is not part of the URL, the closing one should be stripped
        assert_eq!(urls[0].2, "https://example.com/path");
    }

    #[test]
    fn prefix_only_rejected() {
        let row = make_row("https:// nothing");
        let urls = detect_urls(&row);
        assert!(urls.is_empty());
    }

    #[test]
    fn must_have_dot_after_scheme() {
        let row = make_row("https://localhost/path");
        let urls = detect_urls(&row);
        assert!(urls.is_empty());
    }

    #[test]
    fn empty_row_no_urls() {
        let row = make_row("");
        let urls = detect_urls(&row);
        assert!(urls.is_empty());
    }

    #[test]
    fn multiple_urls_in_one_row() {
        let row = make_row("https://a.com https://b.org/x");
        let urls = detect_urls(&row);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].2, "https://a.com");
        assert_eq!(urls[1].2, "https://b.org/x");
    }

    #[test]
    fn column_positions_correct() {
        let row = make_row("XX https://x.com YY");
        let urls = detect_urls(&row);
        assert_eq!(urls[0].0, 3);  // start col
        assert_eq!(urls[0].1, 16); // end col (exclusive) — "https://x.com" is 13 chars
    }
}
