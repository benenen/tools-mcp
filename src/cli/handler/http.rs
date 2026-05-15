//! `tools4a http ...` dispatch.

use super::shared::{cli_to_tunnel_config, load_max_timeout_secs, print_warnings};
use crate::cli::Cli;
use crate::output::CliFormatter;
use tools4a_core::{Error, Result, Service};
use tools4a_http::{HttpAuth, HttpOrchestrator, HttpRequestSpec};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    cli: &Cli,
    method: String,
    url: String,
    headers: Vec<String>,
    data: Option<String>,
    data_file: Option<std::path::PathBuf>,
    json: bool,
    bearer: Option<String>,
    basic: Option<String>,
    insecure: bool,
    include_headers: bool,
) -> Result<()> {
    let mut header_pairs: Vec<(String, String)> = Vec::new();
    for raw in headers {
        let (name, value) = raw.split_once(':').ok_or_else(|| {
            Error::Config(format!(
                "--header '{raw}' must be 'Name: Value' (missing ':')"
            ))
        })?;
        header_pairs.push((name.trim().to_string(), value.trim().to_string()));
    }
    if json {
        header_pairs.push(("Content-Type".to_string(), "application/json".to_string()));
    }

    let body: Option<Vec<u8>> = match (data, data_file) {
        (Some(s), None) => Some(s.into_bytes()),
        (None, Some(path)) => {
            let bytes = std::fs::read(&path).map_err(|e| {
                Error::Config(format!("cannot read --data-file '{}': {e}", path.display()))
            })?;
            Some(bytes)
        }
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
    };

    let auth = match (bearer, basic) {
        (Some(token), None) => HttpAuth::Bearer(token),
        (None, Some(creds)) => {
            let (user, password) = creds
                .split_once(':')
                .ok_or_else(|| Error::Config("--basic must be 'user:password'".to_string()))?;
            HttpAuth::Basic {
                user: user.to_string(),
                password: password.to_string(),
            }
        }
        (None, None) => HttpAuth::None,
        (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
    };

    let max_timeout_secs = load_max_timeout_secs()?;
    let req = HttpRequestSpec {
        method,
        url,
        headers: header_pairs,
        body,
        auth,
        insecure,
        timeout_secs: cli.timeout,
        max_timeout_secs,
    };

    let tunnel_config = cli_to_tunnel_config(cli)?;
    let result = HttpOrchestrator::execute(req, tunnel_config).await?;
    print_warnings(&result);

    if include_headers {
        println!("{}", CliFormatter::format(&result));
    } else if let Some(body_row) = result.rows.last() {
        // Default: print just the body row (the last row, by construction).
        if body_row.len() >= 2 && body_row[0] == "body" {
            println!("{}", body_row[1]);
        } else {
            // Fallback if row layout drifts: print the whole table.
            println!("{}", CliFormatter::format(&result));
        }
    }
    Ok(())
}
