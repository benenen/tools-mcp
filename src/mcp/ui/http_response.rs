//! HTTP response renderer. Consumes the flat `ExecutionResult` shape
//! produced by `tools4a-http::executor::response_to_result` (columns =
//! `["field", "value"]`; rows = `status_code`, `status`, `header.*`,
//! `body`) and emits a self-contained HTML document with a status
//! badge, a collapsible headers panel, and a content-type-aware body
//! panel.

use tools4a_core::ExecutionResult;

use super::escape::html_escape;

/// Render an HTTP `ExecutionResult` as a self-contained HTML document.
pub fn render_http(result: &ExecutionResult) -> String {
    let parsed = ParsedHttp::from_result(result);
    let status_class = status_class_for(parsed.status_code);
    let status_line_html = html_escape(&parsed.status_line);
    let status_code_html = html_escape(&parsed.status_code_str);

    let headers_section = render_headers(&parsed.headers);
    let body_section = render_body(&parsed);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>tools4a http response</title>
<style>
:root {{
  --fg: #1f2328;
  --muted: #6e7781;
  --bg: #ffffff;
  --panel: #f6f8fa;
  --border: #d0d7de;
  --accent: #0969da;
  --status-2xx: #1a7f37;
  --status-3xx: #bf8700;
  --status-4xx: #d1742f;
  --status-5xx: #cf222e;
  --status-other: #6e7781;
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  padding: 12px;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  font-size: 13px;
  color: var(--fg);
  background: var(--bg);
}}
.topbar {{ display: flex; align-items: center; gap: 10px; margin-bottom: 10px; }}
.status-badge {{
  display: inline-block;
  padding: 2px 10px;
  border-radius: 10px;
  color: #fff;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
}}
.status-2xx {{ background: var(--status-2xx); }}
.status-3xx {{ background: var(--status-3xx); }}
.status-4xx {{ background: var(--status-4xx); }}
.status-5xx {{ background: var(--status-5xx); }}
.status-other {{ background: var(--status-other); }}
.status-line {{ font-weight: 600; }}
section {{
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-bottom: 10px;
  overflow: hidden;
}}
section > summary {{
  cursor: pointer;
  user-select: none;
  padding: 8px 12px;
  background: var(--panel);
  font-weight: 600;
}}
.section-body {{ padding: 8px 12px; }}
.headers-table {{ width: 100%; border-collapse: collapse; }}
.headers-table th, .headers-table td {{
  padding: 4px 8px;
  text-align: left;
  vertical-align: top;
  border-bottom: 1px solid var(--border);
  word-break: break-all;
}}
.headers-table th {{ color: var(--muted); font-weight: 600; width: 30%; }}
pre.body-raw {{
  margin: 0;
  padding: 8px 12px;
  background: var(--panel);
  border-top: 1px solid var(--border);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 60vh;
  overflow: auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}}
.body-binary {{
  padding: 12px;
  background: var(--panel);
  border-top: 1px solid var(--border);
  color: var(--muted);
  font-style: italic;
}}
.parse-warn {{
  display: inline-block;
  margin-left: 8px;
  padding: 1px 6px;
  border-radius: 8px;
  background: #fff8c5;
  color: #54470c;
  font-size: 11px;
  font-weight: 600;
}}
.preview-controls {{
  padding: 6px 12px;
  background: var(--panel);
  border-top: 1px solid var(--border);
}}
.preview-controls button {{
  font: inherit;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
}}
.preview-frame {{
  width: 100%;
  height: 60vh;
  border: 0;
  border-top: 1px solid var(--border);
  display: none;
  background: #fff;
}}
.preview-frame.shown {{ display: block; }}
.json-tree {{
  margin: 0;
  padding: 8px 12px;
  background: var(--panel);
  border-top: 1px solid var(--border);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  max-height: 60vh;
  overflow: auto;
}}
.json-tree details {{ margin-left: 1em; }}
.json-tree summary {{ cursor: pointer; user-select: none; }}
.json-key {{ color: #0550ae; }}
.json-string {{ color: #1a7f37; }}
.json-number {{ color: #cf222e; }}
.json-bool {{ color: #8250df; }}
.json-null {{ color: var(--muted); font-style: italic; }}
</style>
</head>
<body>
<div class="topbar">
  <span class="status-badge {status_class}">{status_code_html}</span>
  <span class="status-line">{status_line_html}</span>
</div>
{headers_section}
{body_section}
</body>
</html>
"#
    )
}

struct ParsedHttp<'a> {
    status_code: u16,
    status_code_str: String,
    status_line: String,
    headers: Vec<(&'a str, &'a str)>,
    body: &'a str,
    content_type: Option<&'a str>,
}

impl<'a> ParsedHttp<'a> {
    fn from_result(result: &'a ExecutionResult) -> Self {
        let mut status_code = 0u16;
        let mut status_code_str = String::new();
        let mut status_line = String::new();
        let mut headers: Vec<(&str, &str)> = Vec::new();
        let mut body: &str = "";
        let mut content_type: Option<&str> = None;

        for row in &result.rows {
            if row.len() < 2 {
                continue;
            }
            let field = row[0].as_str();
            let value = row[1].as_str();
            match field {
                "status_code" => {
                    status_code = value.parse().unwrap_or(0);
                    status_code_str = value.to_string();
                }
                "status" => status_line = value.to_string(),
                "body" => body = value,
                _ if field.starts_with("header.") => {
                    let name = &field["header.".len()..];
                    headers.push((name, value));
                    if name.eq_ignore_ascii_case("content-type") {
                        content_type = Some(value);
                    }
                }
                _ => {}
            }
        }

        Self {
            status_code,
            status_code_str,
            status_line,
            headers,
            body,
            content_type,
        }
    }
}

fn status_class_for(code: u16) -> &'static str {
    match code {
        200..=299 => "status-2xx",
        300..=399 => "status-3xx",
        400..=499 => "status-4xx",
        500..=599 => "status-5xx",
        _ => "status-other",
    }
}

fn render_headers(headers: &[(&str, &str)]) -> String {
    if headers.is_empty() {
        return r#"<section><summary>Headers (0)</summary></section>"#.to_string();
    }
    let count = headers.len();
    let open_attr = if count <= 8 { " open" } else { "" };
    let rows: String = headers
        .iter()
        .map(|(name, value)| {
            format!(
                "<tr><th>{}</th><td>{}</td></tr>",
                html_escape(name),
                html_escape(value)
            )
        })
        .collect();
    format!(
        r#"<details{open_attr}><summary>Headers ({count})</summary>
<div class="section-body">
<table class="headers-table"><tbody>{rows}</tbody></table>
</div>
</details>"#
    )
}

fn render_body(parsed: &ParsedHttp<'_>) -> String {
    let body_kind = classify_body(parsed);
    match body_kind {
        BodyKind::Json => render_body_json(parsed.body),
        BodyKind::Html => render_body_html(parsed.body),
        BodyKind::Binary => render_body_binary(parsed.body),
        BodyKind::Text => render_body_text(parsed.body),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum BodyKind {
    Json,
    Html,
    Binary,
    Text,
}

fn classify_body(parsed: &ParsedHttp<'_>) -> BodyKind {
    if is_binary_placeholder(parsed.body) {
        return BodyKind::Binary;
    }
    match parsed.content_type {
        Some(ct) => {
            let lc = ct.to_ascii_lowercase();
            if lc.starts_with("application/json") || lc.contains("+json") {
                BodyKind::Json
            } else if lc.starts_with("text/html") {
                BodyKind::Html
            } else {
                BodyKind::Text
            }
        }
        None => BodyKind::Text,
    }
}

fn is_binary_placeholder(body: &str) -> bool {
    // Matches the literal placeholder emitted by
    // `tools4a-http::executor` for non-UTF-8 response bodies:
    // `<{N} bytes (non-UTF-8 body)>`.
    body.starts_with('<') && body.ends_with(" bytes (non-UTF-8 body)>")
}

fn render_body_json(body: &str) -> String {
    // Try to parse server-side so the parse-failure badge can be
    // rendered in static HTML (matches what the tests check).
    let parse_ok = serde_json::from_str::<serde_json::Value>(body).is_ok();
    let safe_body = body.replace("</", "<\\/");
    let parse_warn = if parse_ok {
        ""
    } else {
        r#" <span class="parse-warn">JSON parse failed</span>"#
    };
    format!(
        r#"<section>
<summary>Body{parse_warn}</summary>
<div id="json-tree" class="json-tree"></div>
<script id="body-data" type="application/json">{safe_body}</script>
<script>
(function () {{
  const raw = document.getElementById('body-data').textContent;
  const container = document.getElementById('json-tree');
  let value;
  try {{ value = JSON.parse(raw); }}
  catch (e) {{
    const pre = document.createElement('pre');
    pre.className = 'body-raw';
    pre.textContent = raw;
    container.replaceWith(pre);
    return;
  }}
  function node(v) {{
    if (v === null) {{
      const s = document.createElement('span');
      s.className = 'json-null';
      s.textContent = 'null';
      return s;
    }}
    if (typeof v === 'string') {{
      const s = document.createElement('span');
      s.className = 'json-string';
      s.textContent = JSON.stringify(v);
      return s;
    }}
    if (typeof v === 'number') {{
      const s = document.createElement('span');
      s.className = 'json-number';
      s.textContent = String(v);
      return s;
    }}
    if (typeof v === 'boolean') {{
      const s = document.createElement('span');
      s.className = 'json-bool';
      s.textContent = String(v);
      return s;
    }}
    if (Array.isArray(v)) {{
      const d = document.createElement('details');
      d.open = true;
      const sum = document.createElement('summary');
      sum.textContent = '[' + v.length + ']';
      d.appendChild(sum);
      v.forEach((item, i) => {{
        const row = document.createElement('div');
        const k = document.createElement('span');
        k.className = 'json-key';
        k.textContent = i + ': ';
        row.appendChild(k);
        row.appendChild(node(item));
        d.appendChild(row);
      }});
      return d;
    }}
    if (typeof v === 'object') {{
      const keys = Object.keys(v);
      const d = document.createElement('details');
      d.open = true;
      const sum = document.createElement('summary');
      sum.textContent = '{{' + keys.length + '}}';
      d.appendChild(sum);
      keys.forEach(key => {{
        const row = document.createElement('div');
        const k = document.createElement('span');
        k.className = 'json-key';
        k.textContent = JSON.stringify(key) + ': ';
        row.appendChild(k);
        row.appendChild(node(v[key]));
        d.appendChild(row);
      }});
      return d;
    }}
    const s = document.createElement('span');
    s.textContent = String(v);
    return s;
  }}
  container.appendChild(node(value));
}})();
</script>
</section>"#
    )
}

fn render_body_html(body: &str) -> String {
    let safe_body = body.replace("</", "<\\/");
    let body_escaped = html_escape(body);
    format!(
        r#"<section>
<summary>Body (HTML)</summary>
<pre class="body-raw">{body_escaped}</pre>
<div class="preview-controls">
  <button id="render-preview">Render preview</button>
</div>
<iframe id="preview-frame" class="preview-frame" sandbox=""></iframe>
<script id="html-body" type="text/plain">{safe_body}</script>
<script>
(function () {{
  const btn = document.getElementById('render-preview');
  const frame = document.getElementById('preview-frame');
  const html = document.getElementById('html-body').textContent;
  btn.addEventListener('click', () => {{
    if (frame.classList.contains('shown')) {{
      frame.classList.remove('shown');
      frame.removeAttribute('srcdoc');
      btn.textContent = 'Render preview';
    }} else {{
      frame.setAttribute('srcdoc', html);
      frame.classList.add('shown');
      btn.textContent = 'Hide preview';
    }}
  }});
}})();
</script>
</section>"#
    )
}

fn render_body_binary(body: &str) -> String {
    // Display the operator-friendly summary line ("Binary content (N
    // bytes)") instead of the raw placeholder text.
    let bytes = parse_binary_bytes(body).unwrap_or(0);
    format!(
        r#"<section>
<summary>Body (binary)</summary>
<div class="body-binary">Binary content ({bytes} bytes) — no preview.</div>
</section>"#
    )
}

fn parse_binary_bytes(body: &str) -> Option<u64> {
    // `<{N} bytes (non-UTF-8 body)>` — extract N.
    let inner = body.strip_prefix('<')?.strip_suffix('>')?;
    let n_str = inner.split_whitespace().next()?;
    n_str.parse().ok()
}

fn render_body_text(body: &str) -> String {
    let body_escaped = html_escape(body);
    format!(
        r#"<section>
<summary>Body</summary>
<pre class="body-raw">{body_escaped}</pre>
</section>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_result(
        status_code: &str,
        status: &str,
        headers: Vec<(&str, &str)>,
        body: &str,
    ) -> ExecutionResult {
        let mut rows: Vec<Vec<String>> = vec![
            vec!["status_code".into(), status_code.into()],
            vec!["status".into(), status.into()],
        ];
        for (n, v) in headers {
            rows.push(vec![format!("header.{n}"), v.to_string()]);
        }
        rows.push(vec!["body".into(), body.into()]);
        let affected = rows.len() as u64;
        ExecutionResult::new(vec!["field".into(), "value".into()], rows, affected)
    }

    #[test]
    fn json_body_renders_tree_with_matching_data_block() {
        let body = r#"{"ok":true,"items":[1,2,3]}"#;
        let result = http_result(
            "200",
            "200 OK",
            vec![("content-type", "application/json; charset=utf-8")],
            body,
        );
        let html = render_http(&result);
        assert!(html.contains("id=\"json-tree\""));
        assert!(html.contains("id=\"body-data\""));
        // Body data block holds the verbatim JSON (with </ escaped, but
        // that doesn't apply here since the body contains no </).
        assert!(html.contains(body));
        // No parse-failure badge for valid JSON.
        assert!(!html.contains("JSON parse failed"));
    }

    #[test]
    fn invalid_json_body_renders_parse_failed_badge() {
        let body = r#"{not valid json"#;
        let result = http_result(
            "200",
            "200 OK",
            vec![("content-type", "application/json")],
            body,
        );
        let html = render_http(&result);
        assert!(html.contains("JSON parse failed"));
    }

    #[test]
    fn html_body_renders_preview_button_and_raw_pre() {
        let body = "<html><body>hello</body></html>";
        let result = http_result(
            "200",
            "200 OK",
            vec![("content-type", "text/html; charset=utf-8")],
            body,
        );
        let html = render_http(&result);
        assert!(html.contains("Render preview"));
        assert!(html.contains("class=\"body-raw\""));
        assert!(html.contains("&lt;html&gt;"));
    }

    #[test]
    fn binary_body_placeholder_renders_binary_notice() {
        let body = "<123 bytes (non-UTF-8 body)>";
        let result = http_result("200", "200 OK", vec![("content-type", "image/png")], body);
        let html = render_http(&result);
        assert!(html.contains("Binary content"));
        assert!(html.contains("123 bytes"));
        assert!(!html.contains("Render preview"));
    }

    #[test]
    fn status_2xx_3xx_4xx_5xx_get_distinct_badge_classes() {
        for (code, class) in [
            ("200", "status-2xx"),
            ("301", "status-3xx"),
            ("404", "status-4xx"),
            ("500", "status-5xx"),
        ] {
            let html = render_http(&http_result(code, "X", vec![], "ok"));
            assert!(
                html.contains(class),
                "expected class {class} for status {code}"
            );
        }
    }

    #[test]
    fn missing_content_type_falls_through_to_raw_text() {
        let body = "plain raw text";
        let result = http_result("200", "200 OK", vec![], body);
        let html = render_http(&result);
        assert!(html.contains("class=\"body-raw\""));
        assert!(html.contains(body));
        assert!(!html.contains("Render preview"));
        // No JSON-tree container is emitted for the raw-text branch
        // (the CSS rules use the class name but no element does).
        assert!(!html.contains(r#"id="json-tree""#));
        assert!(!html.contains(r#"id="body-data""#));
    }

    #[test]
    fn headers_panel_lists_all_headers() {
        let result = http_result(
            "200",
            "200 OK",
            vec![("x-custom", "abc"), ("content-type", "text/plain")],
            "hi",
        );
        let html = render_http(&result);
        assert!(html.contains(">x-custom<"));
        assert!(html.contains(">abc<"));
        assert!(html.contains("Headers (2)"));
    }

    #[test]
    fn status_class_for_unusual_code_is_status_other() {
        assert_eq!(status_class_for(100), "status-other");
        assert_eq!(status_class_for(699), "status-other");
    }
}
