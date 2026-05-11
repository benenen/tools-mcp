# Tools MCP Phase 13: MCP Apps UI for SQL + HTTP tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to execute this plan task-by-task.

**Status:** Drafted 2026-05-11. **Not started** — waiting on the three Open Questions at the bottom before kicking off Task 1.

**Goal:** Wire MCP Apps (embedded interactive UI) into four MCP tools — `mysql_exec`, `pgsql_exec`, `clickhouse_exec`, `http_exec` — so that supporting clients (Claude Code, Claude Desktop, Cursor, etc.) render query results / HTTP responses as a sandboxed iframe alongside the existing JSON text output. Clients without MCP Apps support degrade gracefully to the current text-only behavior.

**Background:** MCP Apps is an official MCP extension (published 2026-01-26 at modelcontextprotocol.io/extensions/apps) that lets a server return a `Resource` with `mimeType: text/html` and a `ui://...` URI; supporting clients render it in a sandboxed iframe with bidirectional postMessage. tools4a currently returns every `ExecutionResult` as serialized JSON text — wide tables, HTTP responses, and JSON-nested cells are hard to scan in that form. Adding a UI layer is purely additive (does not change the typed `Service` / `McpTool` contracts in any crate).

**Why these four tools (and not redis/mongo/ssh):**
- **mysql / pgsql / clickhouse:** Highest-frequency use; columnar result sets benefit most from a sortable HTML table.
- **http:** Response is intrinsically heterogeneous (status + headers + typed body); a three-pane viewer with content-type-aware body rendering is a step-change over flattened key-value text.
- **redis / mongo / ssh:** Deferred to a later phase — each needs a different UI shape (type-aware Redis viewer, BSON tree, ANSI terminal) and the value/effort ratio is lower than the four above.

---

## Architecture

**Embedded UI route, not declarative.** We embed an HTML resource directly in each `CallToolResult` (per-call UI), rather than declaring a static `_meta.ui.resourceUri` on the tool descriptor. Rationale: the UI's content depends entirely on the call's result data (the rendered table is the data); a static UI would need to round-trip back to MCP via a callback to fetch results, adding complexity for no benefit at this stage. Static / callback-style UIs (e.g. a "tools4a control panel" or interactive schema explorer) are deferred to a future phase if a use case emerges.

**Self-contained HTML.** Each UI resource is a single HTML document with inline `<style>` and inline `<script>`. No external CDN, no remote fonts, no remote JS. Clients render MCP App resources under a strict sandbox CSP that may block cross-origin loads, so self-containment is the safe default.

**Data injection via `<script type="application/json">`.** Render-time data (the `ExecutionResult` content) is serialized as JSON inside a `<script id="data" type="application/json">…</script>` block. The page's JS reads `document.getElementById('data').textContent` and renders. This avoids HTML-escape complexity for arbitrary string values in cells (no quote/angle-bracket interpolation into the DOM at all).

**Two-content fallback.** Each `CallToolResult` keeps two items in `content`:
1. `Content::text(json)` — existing JSON-pretty-printed `ExecutionResult` (model-readable, unchanged).
2. `Content::resource(ResourceContents::TextResourceContents { uri: "ui://tools4a/<svc>/...", mime_type: "text/html", text: <html>, meta: None })` — UI for the human.

Clients without MCP Apps support ignore unknown content kinds and see only the text item — same as today. The model continues to reason over the JSON text; the iframe is purely for the user.

**No executor changes.** The `tools4a-{mysql,pgsql,clickhouse,http}` leaf crates are not touched. All UI logic lives in the bin's `src/mcp/ui/` module. Each renderer is a pure function `&ExecutionResult -> String`. This keeps the leaf crates dep-free of the presentation layer and means CLI mode is entirely unaffected.

**HTTP renderer reuses existing ExecutionResult shape.** `tools4a-http::executor::response_to_result` already encodes the response as `columns = ["field", "value"]` with rows `[status_code | status | header.<name> | body]`. The HTTP UI renderer pattern-matches on these field names — no changes needed to the HTTP executor.

---

## Tech Stack

No new deps. We use what `rmcp = "1.6"` already exposes:
- `rmcp::model::Content::text(...)` / `Content::resource(...)`
- `rmcp::model::ResourceContents::TextResourceContents { uri, mime_type, text, meta }`

UI itself: plain HTML5 + CSS + vanilla JS. No frameworks, no CDN.

---

## Out of scope (future phases or deferred to v2 within this phase)

**Out of phase 13 entirely:**
- Redis / Mongo / SSH UI (each needs a different shape; revisit per-service).
- Static / callback-style UIs (`_meta.ui.resourceUri` on tool descriptors).
- Tool-level dashboards (e.g. a "tools4a status" UI that summarizes recent calls).
- Cross-call state (diffing two query results, time-series accumulation).

**Deferred from v2 of this phase (do later, not now):**
- SQL: client-side search box, CSV export button, auto-detect-and-pretty-print JSON columns.
- HTTP: "Copy as curl" button.
- Result-set summarization: trimming the JSON text part for very large rowsets ("returned 5000 rows, see UI for full data"). This is the highest-value v2 add — it converts UI from "pretty wrapper" to "context-window saver" — but defer until we see real cases where token cost is biting.
- Per-tool `_meta.ui.preferredFrameSize` annotations. The renderers produce responsive HTML; client-side sizing should be acceptable for v1.

---

## File Structure

```
src/mcp/
├── mod.rs               # add `pub mod ui;`
├── server.rs            # add into_sql_call_result() + into_http_call_result(); wire mysql/pgsql/clickhouse/http
└── ui/                                    (NEW)
    ├── mod.rs           # pub use sql_table::render_sql; pub use http_response::render_http;
    ├── escape.rs        # html_escape(s: &str) -> String (single-purpose helper for inline <script>/<style> safe interpolation of dynamic strings like svc name)
    ├── sql_table.rs     # render_sql(svc: &str, result: &ExecutionResult) -> String + #[cfg(test)] mod tests
    └── http_response.rs # render_http(result: &ExecutionResult) -> String + #[cfg(test)] mod tests
```

No changes to:
- `crates/tools4a-core/` — `ExecutionResult` and trait surfaces unchanged.
- `crates/tools4a-{mysql,pgsql,clickhouse,http}/` — orchestrators / executors / MCP params unchanged.
- `src/cli/` — CLI output untouched (CliFormatter still renders the same comfy-table).
- `src/output/` — same.

---

## Decisions

1. **Embedded (per-call) UI over declarative UI.** See Architecture.
2. **Keep JSON text content first, UI resource second.** Old clients see text; new clients see both and pick the UI for display. The model always reads the text.
3. **Self-contained HTML; no CDN.** Inline CSS + JS only.
4. **Data via `<script type="application/json">` block.** No interpolation of arbitrary cell strings into HTML/JS — only the `svc` name (constant: `mysql` / `pgsql` / `clickhouse` / `http`) is interpolated into HTML and that value is hard-coded.
5. **HTTP UI consumes the existing flat `ExecutionResult`.** No executor changes.
6. **UI lives in the bin crate (`src/mcp/ui/`), not in leaf crates.** Presentation stays out of reusable libs.
7. **URI scheme:** `ui://tools4a/<svc>/<kind>` — e.g. `ui://tools4a/mysql/result`, `ui://tools4a/http/response`.
8. **`mime_type`** = `"text/html"` exactly (no charset suffix; UI HTML declares `<meta charset="utf-8">` itself).
9. **`meta`** on the resource: `None` for v1. Revisit if a client needs `preferredFrameSize` / permissions / csp hints.

---

## SQL table UI — features in v1

- Columns + rows rendered as `<table>`.
- Long cells (>200 chars) truncated to `…` with a per-cell expand/collapse on click.
- Click a column header to sort ascending; click again descending; third click resets.
- `NULL` cells styled distinctly (italic grey "NULL") vs empty string (visible empty cell with subtle background).
- `affected_rows` displayed in a header strip when `rows` is empty (write-style results).
- `warnings` (if any) rendered as a yellow banner at the top.
- Service name (`mysql` / `pgsql` / `clickhouse`) shown as a small badge so the user knows which DB produced the result.

Deferred (v2): search box, CSV export, auto-JSON-column detection.

---

## HTTP response UI — features in v1

- **Top strip:** status badge colored by class (2xx green / 3xx amber / 4xx orange / 5xx red), full status line text next to it.
- **Headers panel:** collapsible (open by default if ≤8 headers, collapsed if more), shown as `<table>` with name/value.
- **Body panel:** decision rule on `header.content-type`:
  - `application/json*` → parsed and rendered as a collapsible JSON tree. Parse failure → fall back to raw with a "(JSON parse failed)" badge.
  - `text/html*` → raw HTML shown in a `<pre>` block by default; small "Render preview" button reveals a nested `<iframe sandbox>` with the HTML. (Sandbox iframe inside the outer MCP App sandbox iframe — defense in depth.)
  - `image/*` and other non-UTF-8 bodies → body cell is the placeholder `<N bytes (non-UTF-8 body)>` from the executor; UI shows "Binary content (N bytes)" with no preview.
  - `text/*` / unset → raw text in `<pre>`.
  - Anything else → raw text in `<pre>`.
- **No "copy as curl" button in v1.**

---

## Tasks

Each task is one focused commit with the standard `Co-Authored-By: Claude Opus 4.7 (1M context)` trailer. TDD: write the test, watch it fail, write the impl, watch it pass.

### Task 1 — Scaffold `src/mcp/ui` + escape helper

- Create `src/mcp/ui/{mod.rs, escape.rs}`.
- `escape.rs` exposes `pub fn html_escape(s: &str) -> String` — replaces `& < > " '` with HTML entities.
- Add `pub mod ui;` in `src/mcp/mod.rs`.
- Unit tests in `escape.rs`: empty string, plain ASCII, each escapable character, mixed input.

**Commit message:** `feat(mcp/ui): scaffold ui module with html_escape helper`

### Task 2 — SQL table HTML renderer + tests

- Create `src/mcp/ui/sql_table.rs`.
- `pub fn render_sql(svc: &str, result: &ExecutionResult) -> String`.
- Implements the v1 feature list above. Inline CSS + JS, data via `<script id="data" type="application/json">`.
- Unit tests:
  - Empty result (no columns, no rows, `affected_rows = N`) → HTML contains affected_rows banner.
  - Standard result → HTML contains all column names verbatim; HTML contains a `<script id="data">` whose JSON matches a re-serialized form of the result.
  - Warning present → warning banner HTML fragment present.
  - svc badge present and equals the passed `svc`.

**Commit message:** `feat(mcp/ui): SQL table renderer for embedded result UI`

### Task 3 — Wire mysql/pgsql/clickhouse to emit UI resource

- In `src/mcp/server.rs`:
  - Add `fn into_sql_call_result(svc: &'static str, res: tools4a_core::Result<ExecutionResult>) -> std::result::Result<CallToolResult, rmcp::ErrorData>` that returns `Content::text(json) + Content::resource(ui_html)` on success and `Content::text(err)` on error.
  - Change `mysql_exec` / `pgsql_exec` / `clickhouse_exec` to call `into_sql_call_result("mysql", ...)` / `("pgsql", ...)` / `("clickhouse", ...)`.
  - Keep `into_call_result` as the fallback for `redis_exec` / `mongo_exec` / `ssh_exec` (no change to those tools).
- Integration test in `src/mcp/server.rs` (or a new `tests/mcp_ui.rs` if cleaner):
  - Build a synthetic `Ok(ExecutionResult { columns: ..., rows: ..., affected_rows: 0, warnings: vec![] })`, pass through `into_sql_call_result("mysql", ...)`, assert:
    - `CallToolResult.content.len() == 2`
    - `content[0]` is `Text` and parses as JSON with expected fields.
    - `content[1]` is `Resource` with `uri == "ui://tools4a/mysql/result"` and `mime_type == Some("text/html")`.
  - Error path: `Err(...)` produces `CallToolResult::error` with one text item (no UI for errors in v1).

**Commit message:** `feat(mcp): embed UI resource in mysql/pgsql/clickhouse tool results`

### Task 4 — HTTP response HTML renderer + tests

- Create `src/mcp/ui/http_response.rs`.
- `pub fn render_http(result: &ExecutionResult) -> String`.
- Pattern-match `result.rows` by the `field` column to extract status_code, status, headers (rows with `field` starting `header.`), body.
- Implement the v1 HTTP feature list above (status badge, headers panel, content-type-aware body).
- Unit tests:
  - JSON body + `content-type: application/json` → HTML contains a `<script>` placeholder for a JSON tree renderer; body data block matches.
  - HTML body + `content-type: text/html` → HTML contains a "Render preview" button + raw `<pre>` block.
  - Binary body placeholder (`<123 bytes (non-UTF-8 body)>`) → HTML contains "Binary content" wording, no preview.
  - Status 200 / 404 / 500 → badge class differs (assert via class name presence in HTML).
  - No `content-type` header → falls through to raw text branch.

**Commit message:** `feat(mcp/ui): HTTP response renderer with content-type-aware body`

### Task 5 — Wire `http_exec` to emit UI resource

- In `src/mcp/server.rs`:
  - Add `fn into_http_call_result(res: tools4a_core::Result<ExecutionResult>) -> std::result::Result<CallToolResult, rmcp::ErrorData>`.
  - Change `http_exec` to call `into_http_call_result(...)`.
- Integration test:
  - Synthetic HTTP `ExecutionResult` (status 200, content-type json, body `{"ok":true}`) through `into_http_call_result`.
  - Assert `content[1]` is `Resource` with `uri == "ui://tools4a/http/response"` and `mime_type == Some("text/html")`.

**Commit message:** `feat(mcp): embed UI resource in http_exec tool result`

---

## Open questions (resolve before starting Task 1)

These are intentionally pinned at the bottom so they get answered before code is written:

1. **Client renders `ui://` resources?** Confirm that the client(s) the user actually uses (Claude Code, and any others) renders an MCP App `Resource` content item with `text/html` mime in a sandbox iframe today. If not, this entire phase produces invisible HTML — usable test coverage but zero end-user value until clients catch up. Suggested mitigation: do Task 1 and a stripped-down Task 3 (mysql only, returning a "Hello from tools4a" HTML to start), let the user verify it appears in their client, then continue.

2. **HTML body "Render preview" button — in scope?** Nested sandbox iframe to render remote HTML inside the MCP App iframe. Adds value for previewing API responses that return HTML, but adds a small surface area (the inner sandbox is the user-agent's standard `<iframe sandbox>`, which is strong). If we drop this, Task 4's HTML branch just shows raw text — simpler.

3. **Token-saving JSON text summarization — in scope for v1 or v2?** When a SQL result has hundreds/thousands of rows, the `Content::text(json)` part still consumes context. We could truncate that to "N rows, M columns, first 20 rows shown" while the UI keeps the full data. This is the highest-value "real" benefit of MCP Apps for tools4a. If yes, Task 3 needs the truncation logic added; if no, defer to a later phase.

---

## Conventions reminder (from CLAUDE.md)

- Cargo edition `2024`. Don't downgrade.
- YAML crate is `serde_yml`. Don't touch.
- Tests come before implementation.
- One commit per task.
- `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer on each commit.
- No business logic in `src/mcp/server.rs` beyond the thin `#[tool]` handlers. UI rendering lives in `src/mcp/ui/`.
- CLI mode (`src/cli/`, `src/output/`) is not touched in this phase.
