//! Read-only classification for SQL queries and Mongo commands.
//!
//! Used by `MysqlOrchestrator` / `PgsqlOrchestrator` / `MongoOrchestrator`
//! to gate write operations behind an explicit `allow_write` opt-in.
//!
//! For SQL services, callers ALSO enforce a DB-level read-only session
//! when `allow_write` is false — so a misclassified write here will
//! still be rejected by the database itself.

use serde_json::Value;

/// SQL statement keywords treated as read-only. Matched case-insensitively
/// against the first non-comment, non-whitespace token of the query.
/// `WITH` is allowed because the DB-level read-only session catches any
/// DML hidden inside the CTE.
const SQL_READ_KEYWORDS: &[&str] = &[
    "SELECT", "SHOW", "EXPLAIN", "DESCRIBE", "DESC", "USE", "WITH", "VALUES", "TABLE",
];

/// Mongo `runCommand` names treated as read-only. `aggregate` is
/// special-cased — pipelines containing `$out` or `$merge` are writes.
const MONGO_READ_COMMANDS: &[&str] = &[
    "find",
    "aggregate",
    "count",
    "distinct",
    "listCollections",
    "listDatabases",
    "listIndexes",
    "dbStats",
    "collStats",
    "serverStatus",
    "serverInfo",
    "ping",
    "hello",
    "isMaster",
    "ismaster",
    "buildInfo",
    "getParameter",
    "currentOp",
    "top",
    "hostInfo",
    "connPoolStats",
    "connectionStatus",
    "getCmdLineOpts",
    "listCommands",
    "dataSize",
    "explain",
    "getLog",
];

/// Skip leading whitespace and SQL comments (line `--…\n` and block `/*…*/`),
/// then return the first alphabetic token uppercased. None for an
/// effectively-empty query.
fn first_sql_keyword(query: &str) -> Option<String> {
    let mut s = query;
    loop {
        let trimmed = s.trim_start();
        if let Some(after) = trimmed.strip_prefix("--") {
            s = after.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
            continue;
        }
        if let Some(after) = trimmed.strip_prefix("/*") {
            match after.split_once("*/") {
                Some((_, rest)) => {
                    s = rest;
                    continue;
                }
                None => return None,
            }
        }
        s = trimmed;
        break;
    }
    let token: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if token.is_empty() {
        None
    } else {
        Some(token.to_ascii_uppercase())
    }
}

/// True if `query`'s first keyword is in the SQL read-only whitelist.
/// Empty queries return true (the DB will produce its own error).
pub fn is_readonly_sql(query: &str) -> bool {
    match first_sql_keyword(query) {
        Some(kw) => SQL_READ_KEYWORDS.iter().any(|r| **r == kw),
        None => true,
    }
}

/// Mongo's `runCommand` convention is "the first object key is the
/// command name." `serde_json` doesn't preserve key order without the
/// `preserve_order` feature, so we extract the first key directly from
/// the JSON text.
fn first_json_key(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'{' {
        return None;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;
    let start = i;
    while i < bytes.len() && bytes[i] != b'"' {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        i += 1;
    }
    if i > bytes.len() {
        return None;
    }
    std::str::from_utf8(&bytes[start..i]).ok()
}

/// True if the Mongo `runCommand` JSON document is a read.
/// Returns false on invalid JSON — we'd rather force `--allow-write`
/// than wave a malformed command through.
pub fn is_readonly_mongo(command_json: &str) -> bool {
    let cmd = match first_json_key(command_json) {
        Some(c) => c,
        None => return false,
    };
    if !MONGO_READ_COMMANDS.contains(&cmd) {
        return false;
    }
    if cmd == "aggregate" {
        // Parse to inspect pipeline stages for $out / $merge.
        let v: Value = match serde_json::from_str(command_json) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if let Some(pipeline) = v.get("pipeline").and_then(Value::as_array) {
            for stage in pipeline {
                if let Some(obj) = stage.as_object()
                    && (obj.contains_key("$out") || obj.contains_key("$merge"))
                {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_select_variants_are_readonly() {
        assert!(is_readonly_sql("SELECT 1"));
        assert!(is_readonly_sql("  select * from users"));
        assert!(is_readonly_sql("/* hi */ SELECT 1"));
        assert!(is_readonly_sql("-- hi\nSELECT 1"));
        assert!(is_readonly_sql("SHOW TABLES"));
        assert!(is_readonly_sql("EXPLAIN SELECT 1"));
        assert!(is_readonly_sql("DESCRIBE users"));
        assert!(is_readonly_sql("DESC users"));
        assert!(is_readonly_sql("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(is_readonly_sql("VALUES (1, 2)"));
        assert!(is_readonly_sql("TABLE foo"));
    }

    #[test]
    fn sql_writes_are_not_readonly() {
        assert!(!is_readonly_sql("INSERT INTO t VALUES (1)"));
        assert!(!is_readonly_sql("update t set x=1"));
        assert!(!is_readonly_sql("DELETE FROM t"));
        assert!(!is_readonly_sql("CREATE TABLE t (x int)"));
        assert!(!is_readonly_sql("DROP TABLE t"));
        assert!(!is_readonly_sql("TRUNCATE t"));
        assert!(!is_readonly_sql("ALTER TABLE t ADD x int"));
        assert!(!is_readonly_sql("REPLACE INTO t VALUES (1)"));
        assert!(!is_readonly_sql("GRANT ALL ON t TO u"));
        assert!(!is_readonly_sql("CALL p()"));
        assert!(!is_readonly_sql("SET autocommit=0"));
    }

    #[test]
    fn empty_sql_is_readonly() {
        assert!(is_readonly_sql(""));
        assert!(is_readonly_sql("   "));
        assert!(is_readonly_sql("-- only comment"));
    }

    #[test]
    fn mongo_reads_are_readonly() {
        assert!(is_readonly_mongo(r#"{"find":"users","filter":{}}"#));
        assert!(is_readonly_mongo(r#"{"count":"users"}"#));
        assert!(is_readonly_mongo(r#"{"listCollections":1}"#));
        assert!(is_readonly_mongo(r#"{"listDatabases":1}"#));
        assert!(is_readonly_mongo(r#"{"ping":1}"#));
        assert!(is_readonly_mongo(
            r#"{"aggregate":"users","pipeline":[{"$match":{}}]}"#
        ));
    }

    #[test]
    fn mongo_aggregate_with_out_or_merge_is_write() {
        assert!(!is_readonly_mongo(
            r#"{"aggregate":"users","pipeline":[{"$out":"out_coll"}]}"#
        ));
        assert!(!is_readonly_mongo(
            r#"{"aggregate":"users","pipeline":[{"$merge":{"into":"x"}}]}"#
        ));
    }

    #[test]
    fn mongo_writes_are_not_readonly() {
        assert!(!is_readonly_mongo(r#"{"insert":"t","documents":[]}"#));
        assert!(!is_readonly_mongo(
            r#"{"update":"t","updates":[{"q":{},"u":{}}]}"#
        ));
        assert!(!is_readonly_mongo(r#"{"delete":"t","deletes":[]}"#));
        assert!(!is_readonly_mongo(r#"{"drop":"t"}"#));
        assert!(!is_readonly_mongo(r#"{"create":"t"}"#));
        assert!(!is_readonly_mongo(r#"{"createIndexes":"t","indexes":[]}"#));
        assert!(!is_readonly_mongo(r#"{"findAndModify":"t"}"#));
    }

    #[test]
    fn mongo_invalid_json_is_not_readonly() {
        assert!(!is_readonly_mongo("not json"));
        assert!(!is_readonly_mongo(""));
        assert!(!is_readonly_mongo("[]"));
    }

    #[test]
    fn mongo_with_leading_whitespace_works() {
        assert!(is_readonly_mongo("  \n {\"find\":\"users\"}"));
    }
}
