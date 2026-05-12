//! Minimal HTML-entity escape for dynamic string interpolation into
//! inline `<script>` / `<style>` / attribute contexts. The renderers in
//! this module pass arbitrary cell strings to the browser as JSON inside
//! a `<script type="application/json">` block, so the only callers of
//! this helper are for short static-ish fields like the `svc` badge.

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string() {
        assert_eq!(html_escape(""), "");
    }

    #[test]
    fn plain_ascii_unchanged() {
        assert_eq!(html_escape("hello world 123"), "hello world 123");
    }

    #[test]
    fn ampersand() {
        assert_eq!(html_escape("&"), "&amp;");
    }

    #[test]
    fn less_than() {
        assert_eq!(html_escape("<"), "&lt;");
    }

    #[test]
    fn greater_than() {
        assert_eq!(html_escape(">"), "&gt;");
    }

    #[test]
    fn double_quote() {
        assert_eq!(html_escape("\""), "&quot;");
    }

    #[test]
    fn single_quote() {
        assert_eq!(html_escape("'"), "&#39;");
    }

    #[test]
    fn mixed_input() {
        assert_eq!(
            html_escape("<script>alert('x & y')</script>"),
            "&lt;script&gt;alert(&#39;x &amp; y&#39;)&lt;/script&gt;"
        );
    }

    #[test]
    fn non_ascii_unchanged() {
        assert_eq!(html_escape("café 日本語"), "café 日本語");
    }
}
