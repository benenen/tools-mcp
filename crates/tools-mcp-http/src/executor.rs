use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use reqwest::{Client, Method};
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::request::{HttpAuth, HttpRequestSpec};

pub struct HttpExecutor;

impl HttpExecutor {
    /// Send the request through `client` and map the response into an
    /// `ExecutionResult` with one row per { status, header.*, body }.
    pub async fn run(client: &Client, req: HttpRequestSpec) -> Result<ExecutionResult> {
        let method = parse_method(&req.method)?;
        let mut builder = client.request(method, &req.url);

        // Headers
        let mut header_map = HeaderMap::new();
        for (name, value) in &req.headers {
            let h_name: HeaderName = name.parse().map_err(|e| {
                Error::Config(format!("invalid header name '{name}': {e}"))
            })?;
            let h_value: HeaderValue = value.parse().map_err(|e| {
                Error::Config(format!("invalid header value for '{name}': {e}"))
            })?;
            header_map.append(h_name, h_value);
        }

        // Auth
        match &req.auth {
            HttpAuth::None => {}
            HttpAuth::Bearer(token) => {
                let val: HeaderValue = format!("Bearer {token}")
                    .parse()
                    .map_err(|e| Error::Config(format!("invalid bearer token: {e}")))?;
                header_map.insert(AUTHORIZATION, val);
            }
            HttpAuth::Basic { user, password } => {
                builder = builder.basic_auth(user, Some(password));
            }
        }

        builder = builder.headers(header_map);

        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let response = builder
            .send()
            .await
            .map_err(|e: reqwest::Error| Error::Service(format!("HTTP: {e}")))?;

        response_to_result(response).await
    }
}

fn parse_method(s: &str) -> Result<Method> {
    Method::from_bytes(s.to_uppercase().as_bytes())
        .map_err(|e| Error::Config(format!("invalid HTTP method '{s}': {e}")))
}

async fn response_to_result(response: reqwest::Response) -> Result<ExecutionResult> {
    let status = response.status();
    let status_line = format!(
        "{} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );

    // Snapshot headers before consuming the response for the body.
    let mut header_rows: Vec<(String, String)> = Vec::new();
    for (name, value) in response.headers().iter() {
        let v = value.to_str().unwrap_or("<non-utf8 header value>").to_string();
        header_rows.push((format!("header.{name}"), v));
    }

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e: reqwest::Error| Error::Service(format!("HTTP body: {e}")))?;
    let body_cell = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text.to_string(),
        Err(_) => format!("<{} bytes (non-UTF-8 body)>", body_bytes.len()),
    };

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(2 + header_rows.len() + 1);
    rows.push(vec!["status_code".to_string(), status.as_u16().to_string()]);
    rows.push(vec!["status".to_string(), status_line]);
    for (name, value) in header_rows {
        rows.push(vec![name, value]);
    }
    rows.push(vec!["body".to_string(), body_cell]);

    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_method_uppercases() {
        assert_eq!(parse_method("get").unwrap(), Method::GET);
        assert_eq!(parse_method("POST").unwrap(), Method::POST);
        assert_eq!(parse_method("PaTcH").unwrap(), Method::PATCH);
    }

    #[test]
    fn test_parse_method_rejects_garbage() {
        let err = parse_method("not a method").unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("invalid HTTP method")));
    }
}
