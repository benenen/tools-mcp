//! SQL table renderer. Produces a self-contained HTML document that
//! shows columns + rows as a sortable table, with NULL / empty-string
//! styling, long-cell truncation, an affected_rows banner for write
//! results, an optional warnings banner, and a service-name badge.

use tools4a_core::ExecutionResult;

use super::escape::html_escape;

/// Render a SQL `ExecutionResult` as a self-contained HTML document.
///
/// `svc` is the short service name (`"mysql"` / `"pgsql"` /
/// `"clickhouse"`) shown as a badge in the header strip.
pub fn render_sql(svc: &str, result: &ExecutionResult) -> String {
    let svc_escaped = html_escape(svc);
    let data_json = serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string());
    // Inside `<script type="application/json">`, the only sequence
    // that ends the block is `</...>`. Escape it harmlessly.
    let data_safe = data_json.replace("</", "<\\/");

    let warnings_banner = if result.warnings.is_empty() {
        String::new()
    } else {
        let items = result
            .warnings
            .iter()
            .map(|w| format!("<li>{}</li>", html_escape(w)))
            .collect::<String>();
        format!(r#"<div class="warnings"><strong>Warnings</strong><ul>{items}</ul></div>"#)
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>tools4a {svc_escaped} result</title>
<style>
:root {{
  --fg: #1f2328;
  --muted: #6e7781;
  --bg: #ffffff;
  --row-alt: #f6f8fa;
  --border: #d0d7de;
  --accent: #0969da;
  --warn-bg: #fff8c5;
  --warn-fg: #54470c;
  --warn-border: #d4a72c;
  --null-fg: #8b949e;
  --empty-bg: #f6f8fa;
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
.topbar {{
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}}
.badge {{
  display: inline-block;
  padding: 2px 8px;
  border-radius: 10px;
  background: var(--accent);
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}}
.summary {{ color: var(--muted); font-size: 12px; }}
.warnings {{
  background: var(--warn-bg);
  color: var(--warn-fg);
  border: 1px solid var(--warn-border);
  border-radius: 6px;
  padding: 8px 12px;
  margin-bottom: 10px;
}}
.warnings ul {{ margin: 4px 0 0 18px; padding: 0; }}
.affected {{
  padding: 12px;
  background: var(--row-alt);
  border: 1px solid var(--border);
  border-radius: 6px;
}}
.table-wrap {{
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 6px;
}}
table {{
  border-collapse: collapse;
  width: 100%;
  font-variant-numeric: tabular-nums;
}}
th, td {{
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  text-align: left;
  vertical-align: top;
  white-space: pre-wrap;
  word-break: break-word;
}}
th {{
  background: var(--row-alt);
  position: sticky;
  top: 0;
  cursor: pointer;
  user-select: none;
  font-weight: 600;
}}
th .sort-indicator {{
  display: inline-block;
  width: 1em;
  color: var(--muted);
}}
tbody tr:nth-child(even) td {{ background: var(--row-alt); }}
.null {{ color: var(--null-fg); font-style: italic; }}
.empty {{
  display: inline-block;
  min-width: 1em;
  min-height: 1em;
  background: var(--empty-bg);
  border-radius: 3px;
}}
.trunc {{ cursor: pointer; }}
.trunc::after {{ content: " …"; color: var(--muted); }}
.expanded .trunc::after {{ content: ""; }}
</style>
</head>
<body>
<div class="topbar">
  <span class="badge">{svc_escaped}</span>
  <span class="summary" id="summary"></span>
</div>
{warnings_banner}
<div id="content"></div>
<script id="data" type="application/json">{data_safe}</script>
<script>
(function () {{
  const data = JSON.parse(document.getElementById('data').textContent);
  const columns = data.columns || [];
  const rows = data.rows || [];
  const affected = data.affected_rows || 0;
  const summary = document.getElementById('summary');
  const content = document.getElementById('content');
  const MAX_LEN = 200;

  function clearNode(node) {{
    while (node.firstChild) node.removeChild(node.firstChild);
  }}

  if (columns.length === 0 || rows.length === 0) {{
    summary.textContent = rows.length + ' rows · ' + columns.length + ' cols';
    const div = document.createElement('div');
    div.className = 'affected';
    div.textContent = 'affected_rows: ' + affected;
    content.appendChild(div);
    return;
  }}
  summary.textContent = rows.length + ' rows · ' + columns.length + ' cols · affected_rows: ' + affected;

  let sortCol = -1;
  let sortDir = 0; // 0 = none, 1 = asc, -1 = desc
  const indices = rows.map((_, i) => i);

  function renderCell(td, value) {{
    if (value === 'NULL') {{
      const span = document.createElement('span');
      span.className = 'null';
      span.textContent = 'NULL';
      td.appendChild(span);
      return;
    }}
    if (value === '') {{
      const span = document.createElement('span');
      span.className = 'empty';
      span.title = '(empty string)';
      td.appendChild(span);
      return;
    }}
    if (value.length > MAX_LEN) {{
      td.classList.add('trunc');
      td.textContent = value.slice(0, MAX_LEN);
      td.addEventListener('click', () => {{
        if (td.classList.contains('expanded')) {{
          td.classList.remove('expanded');
          td.classList.add('trunc');
          td.textContent = value.slice(0, MAX_LEN);
        }} else {{
          td.classList.remove('trunc');
          td.classList.add('expanded');
          td.textContent = value;
        }}
      }});
      return;
    }}
    td.textContent = value;
  }}

  function compareCells(a, b) {{
    if (a === b) return 0;
    if (a === 'NULL') return -1;
    if (b === 'NULL') return 1;
    const na = Number(a);
    const nb = Number(b);
    if (!Number.isNaN(na) && !Number.isNaN(nb) && a !== '' && b !== '') {{
      return na - nb;
    }}
    return a < b ? -1 : 1;
  }}

  function applySort() {{
    if (sortCol < 0 || sortDir === 0) {{
      indices.sort((x, y) => x - y);
      return;
    }}
    indices.sort((x, y) => {{
      const c = compareCells(rows[x][sortCol] ?? '', rows[y][sortCol] ?? '');
      return c * sortDir;
    }});
  }}

  function render() {{
    const wrap = document.createElement('div');
    wrap.className = 'table-wrap';
    const table = document.createElement('table');
    const thead = document.createElement('thead');
    const trh = document.createElement('tr');
    columns.forEach((col, i) => {{
      const th = document.createElement('th');
      const label = document.createElement('span');
      label.textContent = col;
      const ind = document.createElement('span');
      ind.className = 'sort-indicator';
      if (i === sortCol && sortDir === 1) ind.textContent = ' ▲';
      else if (i === sortCol && sortDir === -1) ind.textContent = ' ▼';
      else ind.textContent = '';
      th.appendChild(label);
      th.appendChild(ind);
      th.addEventListener('click', () => {{
        if (sortCol !== i) {{ sortCol = i; sortDir = 1; }}
        else if (sortDir === 1) {{ sortDir = -1; }}
        else if (sortDir === -1) {{ sortCol = -1; sortDir = 0; }}
        else {{ sortDir = 1; }}
        applySort();
        rerender();
      }});
      trh.appendChild(th);
    }});
    thead.appendChild(trh);
    table.appendChild(thead);
    const tbody = document.createElement('tbody');
    indices.forEach(idx => {{
      const tr = document.createElement('tr');
      const row = rows[idx];
      columns.forEach((_, ci) => {{
        const td = document.createElement('td');
        renderCell(td, row[ci] ?? '');
        tr.appendChild(td);
      }});
      tbody.appendChild(tr);
    }});
    table.appendChild(tbody);
    wrap.appendChild(table);
    return wrap;
  }}

  function rerender() {{
    clearNode(content);
    content.appendChild(render());
  }}

  rerender();
}})();
</script>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> ExecutionResult {
        ExecutionResult::new(
            vec!["id".into(), "name".into()],
            vec![
                vec!["1".into(), "alice".into()],
                vec!["2".into(), "bob".into()],
            ],
            2,
        )
    }

    #[test]
    fn empty_result_shows_affected_rows_banner() {
        let result = ExecutionResult::new(vec![], vec![], 7);
        let html = render_sql("mysql", &result);
        // affected_rows value is carried in the embedded JSON, and the JS
        // branch that handles the empty-result case stamps the `affected`
        // class on a div. Both must be present in the rendered output.
        assert!(html.contains("\"affected_rows\":7"));
        assert!(html.contains("'affected'"));
        assert!(html.contains("affected_rows: '"));
    }

    #[test]
    fn standard_result_contains_all_column_names() {
        let result = sample_result();
        let html = render_sql("pgsql", &result);
        assert!(html.contains("\"id\""));
        assert!(html.contains("\"name\""));
    }

    #[test]
    fn data_script_holds_serializable_json() {
        let result = sample_result();
        let html = render_sql("mysql", &result);
        let start_tag = "<script id=\"data\" type=\"application/json\">";
        let start = html.find(start_tag).expect("data script start") + start_tag.len();
        let end = html[start..].find("</script>").expect("data script end");
        let json = &html[start..start + end];
        // Reverse the </ -> <\/ escape we apply when embedding.
        let json = json.replace("<\\/", "</");
        let parsed: ExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.columns, result.columns);
        assert_eq!(parsed.rows, result.rows);
        assert_eq!(parsed.affected_rows, result.affected_rows);
    }

    #[test]
    fn warning_present_renders_banner() {
        let mut result = sample_result();
        result.push_warning("timeout clamped from 60s to 30s");
        let html = render_sql("mysql", &result);
        assert!(html.contains("class=\"warnings\""));
        assert!(html.contains("timeout clamped from 60s to 30s"));
    }

    #[test]
    fn no_warning_means_no_warning_banner() {
        let html = render_sql("mysql", &sample_result());
        assert!(!html.contains("class=\"warnings\""));
    }

    #[test]
    fn svc_badge_present_and_matches_input() {
        let html = render_sql("clickhouse", &sample_result());
        assert!(html.contains(">clickhouse<"));
        assert!(html.contains("class=\"badge\""));
    }

    #[test]
    fn svc_is_html_escaped() {
        let html = render_sql("<svc>", &sample_result());
        assert!(html.contains("&lt;svc&gt;"));
    }
}
