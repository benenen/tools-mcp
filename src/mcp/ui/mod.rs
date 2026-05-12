//! MCP Apps UI renderers. Each `render_*` function is pure: it takes an
//! `ExecutionResult` (and minimal context like the service name) and
//! returns a self-contained HTML document suitable for embedding in a
//! `CallToolResult` as a `ui://tools4a/<svc>/<kind>` resource. No CDN, no
//! external fonts, no remote JS — clients render the HTML in a strict
//! sandbox iframe.

pub mod escape;
