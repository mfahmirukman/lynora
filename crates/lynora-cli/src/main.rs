use clap::{Parser, Subcommand, ValueEnum};
use lynora_core::{
    prepare_request, send_graphql, send_rest, Collection, Environment, GraphQlBody, GraphQlRequest,
    Protocol, RequestDocument, RestResponse,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "lynora", version, about = "Lynora API workbench CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run all runnable requests in a collection directory
    Run {
        /// Path to a Lynora collection folder (contains lynora.json)
        collection: PathBuf,
        /// Environment JSON file or environment name under --env-dir
        #[arg(long)]
        env: Option<String>,
        /// Directory of environment JSON files (default: <collection>/../environments unused; pass file with --env-file)
        #[arg(long)]
        env_file: Option<PathBuf>,
        /// Report format
        #[arg(long, default_value = "text")]
        format: ReportFormat,
        /// Fail the run if any request lacks expectStatus (strict CI mode)
        #[arg(long, default_value_t = false)]
        require_expect: bool,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum ReportFormat {
    Text,
    Json,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StepResult {
    name: String,
    method: String,
    url: String,
    protocol: String,
    ok: bool,
    status: Option<u16>,
    expected_status: Option<u16>,
    duration_ms: Option<u128>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    collection: String,
    passed: usize,
    failed: usize,
    skipped: usize,
    steps: Vec<StepResult>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            collection,
            env,
            env_file,
            format,
            require_expect,
        } => match run_collection(&collection, env.as_deref(), env_file.as_deref(), require_expect).await
        {
            Ok(report) => {
                print_report(&report, format);
                if report.failed > 0 {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
    }
}

fn print_report(report: &RunReport, format: ReportFormat) {
    match format {
        ReportFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report).unwrap_or_default());
        }
        ReportFormat::Text => {
            println!("Collection: {}", report.collection);
            for step in &report.steps {
                let mark = if step.ok { "PASS" } else { "FAIL" };
                let status = step
                    .status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".into());
                println!(
                    "[{mark}] {} {} -> status {status} ({} ms){}",
                    step.method,
                    step.name,
                    step.duration_ms.unwrap_or(0),
                    step.error
                        .as_ref()
                        .map(|e| format!(" — {e}"))
                        .unwrap_or_default()
                );
            }
            println!(
                "Summary: {} passed, {} failed, {} skipped",
                report.passed, report.failed, report.skipped
            );
        }
    }
}

async fn run_collection(
    path: &Path,
    env_name: Option<&str>,
    env_file: Option<&Path>,
    require_expect: bool,
) -> Result<RunReport, String> {
    let col = Collection::load(path).map_err(|e| e.to_string())?;
    let vars = load_vars(env_name, env_file)?;

    let mut steps = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;

    for req in &col.requests {
        match req.protocol {
            Protocol::Websocket | Protocol::Sse | Protocol::Grpc => {
                skipped += 1;
                steps.push(StepResult {
                    name: req.name.clone(),
                    method: req.method.clone(),
                    url: req.url.clone(),
                    protocol: format!("{:?}", req.protocol).to_lowercase(),
                    ok: true,
                    status: None,
                    expected_status: req.expect_status,
                    duration_ms: None,
                    error: Some("skipped in CLI (use desktop for this protocol)".into()),
                });
                continue;
            }
            Protocol::Rest | Protocol::Graphql => {}
        }

        if require_expect && req.expect_status.is_none() {
            failed += 1;
            steps.push(StepResult {
                name: req.name.clone(),
                method: req.method.clone(),
                url: req.url.clone(),
                protocol: "rest".into(),
                ok: false,
                status: None,
                expected_status: None,
                duration_ms: None,
                error: Some("missing expectStatus (--require-expect)".into()),
            });
            continue;
        }

        match execute_request(req, &vars).await {
            Ok(resp) => {
                let ok = match req.expect_status {
                    Some(expected) => resp.status == expected,
                    None => true,
                };
                if ok {
                    passed += 1;
                } else {
                    failed += 1;
                }
                steps.push(StepResult {
                    name: req.name.clone(),
                    method: req.method.clone(),
                    url: req.url.clone(),
                    protocol: match req.protocol {
                        Protocol::Graphql => "graphql".into(),
                        _ => "rest".into(),
                    },
                    ok,
                    status: Some(resp.status),
                    expected_status: req.expect_status,
                    duration_ms: Some(resp.duration_ms),
                    error: if ok {
                        None
                    } else {
                        Some(format!(
                            "expected status {}, got {}",
                            req.expect_status.unwrap_or(0),
                            resp.status
                        ))
                    },
                });
            }
            Err(e) => {
                failed += 1;
                steps.push(StepResult {
                    name: req.name.clone(),
                    method: req.method.clone(),
                    url: req.url.clone(),
                    protocol: format!("{:?}", req.protocol).to_lowercase(),
                    ok: false,
                    status: None,
                    expected_status: req.expect_status,
                    duration_ms: None,
                    error: Some(e),
                });
            }
        }
    }

    Ok(RunReport {
        collection: col.meta.name,
        passed,
        failed,
        skipped,
        steps,
    })
}

fn load_vars(
    env_name: Option<&str>,
    env_file: Option<&Path>,
) -> Result<HashMap<String, String>, String> {
    if let Some(path) = env_file {
        let env = Environment::load(path).map_err(|e| e.to_string())?;
        return Ok(env.values);
    }
    if let Some(name) = env_name {
        // Treat as path if it ends with .json, else error asking for --env-file
        let path = PathBuf::from(name);
        if path.exists() {
            let env = Environment::load(&path).map_err(|e| e.to_string())?;
            return Ok(env.values);
        }
        return Err(format!(
            "environment '{name}' not found; pass --env-file path/to/env.json"
        ));
    }
    Ok(HashMap::new())
}

async fn execute_request(
    req: &RequestDocument,
    vars: &HashMap<String, String>,
) -> Result<RestResponse, String> {
    match req.protocol {
        Protocol::Graphql => {
            let prepared = prepare_request(req, vars).map_err(|e| e.to_string())?;
            let mut gql = req.graphql.clone().unwrap_or(GraphQlBody {
                query: req.body.clone().unwrap_or_default(),
                variables: None,
                operation_name: None,
            });
            if let Some(v) = gql.variables.as_ref() {
                gql.variables = Some(lynora_core::expand(v, vars).map_err(|e| e.to_string())?);
            }
            gql.query = lynora_core::expand(&gql.query, vars).map_err(|e| e.to_string())?;
            send_graphql(GraphQlRequest {
                url: prepared.url,
                headers: prepared.headers,
                body: gql,
            })
            .await
            .map_err(|e| e.to_string())
        }
        Protocol::Rest => {
            let prepared = prepare_request(req, vars).map_err(|e| e.to_string())?;
            send_rest(prepared).await.map_err(|e| e.to_string())
        }
        _ => Err("unsupported protocol".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lynora_core::{Header, Protocol, RequestDocument};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use uuid::Uuid;

    fn spawn_ok_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = b"{\"ok\":true}";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn run_passes_with_expect_status() {
        let url = spawn_ok_server();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("col");
        let mut col = Collection::create(&root, "CI").unwrap();
        let req = RequestDocument {
            id: Uuid::new_v4().to_string(),
            name: "health".into(),
            method: "GET".into(),
            url,
            headers: vec![Header {
                key: "Accept".into(),
                value: "application/json".into(),
                enabled: true,
            }],
            body: None,
            protocol: Protocol::Rest,
            auth: None,
            graphql: None,
            grpc: None,
            expect_status: Some(200),
            websocket: None,
            sse: None,
        };
        col.save_request(&req).unwrap();
        let report = run_collection(&root, None, None, false).await.unwrap();
        assert_eq!(report.failed, 0);
        assert_eq!(report.passed, 1);
    }
}
