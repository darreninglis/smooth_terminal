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
