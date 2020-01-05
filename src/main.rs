use rusoto_core::credential::StaticProvider;
use rusoto_core::{DefaultCredentialsProvider, HttpClient, Region};
use rusoto_lambda::{Lambda, LambdaClient, UpdateFunctionCodeRequest};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::{env, process};
use structopt::StructOpt;
use toml::Value;

mod util;

use rusoto_logs::{CloudWatchLogs, CloudWatchLogsClient, FilterLogEventsRequest};
use std::collections::HashSet;
use std::fmt::Display;
use std::time::{Duration, SystemTime};
use util::CommandExt;

/// Packages and deploys your project binaries to AWS Lambda
#[derive(StructOpt, Debug)]
struct Opt {
    /// AWS Access Key
    #[structopt(long)]
    access_key: Option<String>,
    /// AWS Secret Key
    #[structopt(long)]
    secret_key: Option<String>,
    /// Full ARN of the function to deploy or its configuration key in table [arns] in Lambda.toml
    /// (e.g. arn:aws:lambda:eu-north-1:1234:function:MyLambdaFunc)
    #[structopt(name = "FUNCTION_ARN")]
    arn: String,
    /// Project binary to deploy
    /// (e.g. `mylambdafunc`, if you have src/bin/mylambdafunc.rs with a main function in your project)
    #[structopt(name = "BIN")]
    bin: String,
    /// Retain debug info in executable (for backtraces etc.)
    #[structopt(long)]
    keep_debug_info: bool,
    /// Override docker image with your own
    #[structopt(long, default_value = "softprops/lambda-rust:latest")]
    docker_image: String,
    /// Dry-run (compile and deploy in dry-run mode)
    #[structopt(long)]
    dry_run: bool,
    /// Use managed persistent build volume (speeds things up on windows hosts)
    #[structopt(long)]
    use_build_volume: bool,
    /// Pass environment variables to the container (for eg. -e RUSTFLAGS=-Ztime-passes)
    #[structopt(short, long)]
    env: Vec<String>,
    /// Tail function's cloudwatch logs
    #[structopt(long)]
    tail_logs: bool,
}

fn main() {
    check_docker();

    let mut args = env::args().collect::<Vec<_>>();
    args.remove(1);
    let opt = Opt::from_iter(args);

    if opt.use_build_volume {
        manage_build_volume();
    }

    let zip_file = format!("{}.zip", opt.bin);
    let (region, func_name) = parse_arn_or_key(&opt.arn);
    let project_dir = env::current_dir().expect("Can't read cwd.");

    let mut zip_path = project_dir.clone();
    zip_path.extend(&["target", "lambda", "release", &zip_file]);

    println!(
        "Preparing to deploy {} to {:?} {}",
        zip_path.display(),
        region,
        func_name
    );

    let cargo_path = PathBuf::from(env::var("CARGO_HOME").expect("Missing CARGO_HOME"));
    let cargo_registry = {
        let mut cargo_path = cargo_path.clone();
        cargo_path.push("registry");
        cargo_path
    };

    let args = build_docker_args(project_dir.as_path(), cargo_registry.as_path(), &opt);

    println!("Running docker with args {}", args.join(" "));

    let success = Command::new("docker")
        .args(args)
        .env("BIN", &opt.bin)
        .status_bool();

    if !success {
        eprintln!("Running docker failed, check output above");
        process::exit(1);
    }

    let zip_data = {
        let mut zip_file = File::open(zip_path).expect("Can't open zip path");
        let mut data = Vec::new();
        zip_file.read_to_end(&mut data).unwrap();
        bytes::Bytes::from(data)
    };

    let client = create_lambda_client(&opt, &region);
    let req = UpdateFunctionCodeRequest {
        dry_run: Some(opt.dry_run),
        function_name: func_name.to_owned(),
        publish: Some(!opt.dry_run),
        zip_file: Some(zip_data),
        ..Default::default()
    };
    let res = client.update_function_code(req).sync();
    if let Ok(res) = res {
        fn disp<D: Display>(x: Option<D>) -> String {
            x.map(|x| format!("{}", x)).unwrap_or("N/A".to_owned())
        }
        println!("\n===== Deploy successful =====");
        println!("Function:      {}", disp(res.function_name.as_ref()));
        println!("Handler        {}", disp(res.handler));
        println!("Version:       {}", disp(res.version));
        println!("SHA-256:       {}", disp(res.code_sha_256));
        println!("Last Modified: {}", disp(res.last_modified));
        println!("Runtime:       {}", disp(res.runtime));
        println!("Mem limit:     {} MB", disp(res.memory_size));
        println!("Time limit:    {} s", disp(res.timeout));
        println!("ARN:           {}", disp(res.function_arn));
        println!("Role:          {}", disp(res.role));

        if opt.tail_logs {
            println!("\n===== Tailing logs =====");
            let logs_client = create_logs_client(&opt, &region);
            let func_name = res.function_name.unwrap_or("".into());
            if let Err(e) = tail_logs(&func_name, &logs_client) {
                eprintln!("Failed to tail logs:\n{:?}", e);
                ::std::process::exit(1);
            }
        }
    } else {
        eprintln!("\n===== Deploy FAILED =====");
        eprintln!("{:#?}", res);
        ::std::process::exit(1);
    }
}

fn create_lambda_client(opt: &Opt, region: &str) -> LambdaClient {
    let dispatcher = HttpClient::new().expect("failed to create request dispatcher");
    let region = Region::from_str(region).unwrap();

    match (&opt.access_key, &opt.secret_key) {
        (Some(access_key), Some(secret_key)) => {
            let creds = StaticProvider::new_minimal(access_key.to_owned(), secret_key.to_owned());
            LambdaClient::new_with(dispatcher, creds, region)
        }
        _ => {
            let creds =
                DefaultCredentialsProvider::new().expect("failed to create credentials provider");
            LambdaClient::new_with(dispatcher, creds, region)
        }
    }
}

fn create_logs_client(opt: &Opt, region: &str) -> CloudWatchLogsClient {
    let dispatcher = HttpClient::new().expect("failed to create request dispatcher");
    let region = Region::from_str(region).unwrap();

    match (&opt.access_key, &opt.secret_key) {
        (Some(access_key), Some(secret_key)) => {
            let creds = StaticProvider::new_minimal(access_key.to_owned(), secret_key.to_owned());
            CloudWatchLogsClient::new_with(dispatcher, creds, region)
        }
        _ => {
            let creds =
                DefaultCredentialsProvider::new().expect("failed to create credentials provider");
            CloudWatchLogsClient::new_with(dispatcher, creds, region)
        }
    }
}

fn check_docker() {
    let result = Command::new("docker").args(&["--version"]).output();
    match result {
        Ok(ref output) if output.status.success() => {}
        e => {
            eprintln!(
                "Docker missing, executing docker --version failed with {:?}",
                e
            );
            process::exit(1);
        }
    }
}

fn parse_arn_or_key(raw: &str) -> (String, String) {
    if raw.split(":").count() != 7 {
        if let Ok(mut lambda_toml_file) = File::open("Lambda.toml") {
            let cargo_toml: Value = {
                let mut data = String::new();
                lambda_toml_file
                    .read_to_string(&mut data)
                    .expect("Can't read ./Lambda.toml");
                toml::from_str(&data).expect("Can't parse ./Lambda.toml")
            };

            let arn = cargo_toml
                .get("arns")
                .and_then(|arns| arns.get(raw))
                .and_then(|v| v.as_str());

            if let Some(value) = arn {
                return parse_arn(value);
            }
        }
    }
    parse_arn(raw)
}

fn parse_arn(raw: &str) -> (String, String) {
    let arn: Vec<_> = raw.split(":").collect();
    if arn.len() != 7 {
        eprintln!("Unidentified ARN, should be like arn:aws:lambda:<region>:<account id>:function:<function name> or a key to Lambda.toml");
        process::exit(1);
    }

    let region = arn[3];
    let func_name = arn[6];
    (region.to_string(), func_name.to_string())
}

fn build_docker_args(project_dir: &Path, cargo_registry: &Path, opt: &Opt) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        format!("{}:/code", project_dir.display()),
    ];

    if opt.use_build_volume {
        args.push("-v".into());
        args.push(format!("{}:/build-volume", build_volume_name()));
        args.push("-v".into());
        args.push(format!("{}:/root/.cargo/registry", build_volume_name()));
    } else {
        args.push("-v".into());
        args.push(format!(
            "{}:/root/.cargo/registry",
            cargo_registry.display()
        ));
    }

    if opt.keep_debug_info {
        args.push("-e".into());
        args.push("DEBUGINFO=1".into());
    }

    for env in &opt.env {
        args.push("-e".into());
        args.push(env.clone());
    }

    args.push(opt.docker_image.clone());
    args
}

fn build_volume_name() -> String {
    let cwd = std::env::current_dir().expect("Can't get cwd");
    let basename = cwd
        .file_name()
        .and_then(OsStr::to_str)
        .expect("Can't get basename from cwd");
    format!("rust-build-volume-{}", basename)
}

fn manage_build_volume() {
    let name = build_volume_name();

    let success = Command::new("docker")
        .args(&["volume", "inspect", &name])
        .status_bool();

    if !success {
        println!("Didn't find build volume {}, creating it", name);
    } else {
        return;
    }

    let success = Command::new("docker")
        .args(&["volume", "create", &name])
        .status_bool();

    if !success {
        eprintln!("Failed to create docker build volume {}", name);
        ::std::process::exit(1);
    } else {
        println!("Created docker volume {}", name)
    }
}

fn tail_logs(
    function_name: &str,
    logs_client: &CloudWatchLogsClient,
) -> Result<(), Box<dyn ::std::error::Error>> {
    let unix = || {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            - 5 * 60 * 1000
    };
    let mut next_token = None;
    let mut start_time = Some(unix());
    let mut seen = HashSet::new();

    loop {
        let input = FilterLogEventsRequest {
            end_time: None,
            filter_pattern: None,
            limit: Some(10000),
            log_group_name: format!("/aws/lambda/{}", function_name),
            log_stream_name_prefix: None,
            log_stream_names: None,
            next_token: next_token.clone(),
            start_time,
        };

        let res = logs_client.filter_log_events(input).sync()?;

        if let Some(events) = res.events {
            for event in events {
                if !seen.contains(event.event_id.as_ref().unwrap()) {
                    print!("{}", event.message.unwrap());
                    seen.insert(event.event_id.unwrap().clone());
                }
            }
        }

        next_token = res.next_token;

        if next_token.is_none() {
            start_time = Some(unix());
        }
        ::std::thread::sleep(Duration::from_millis(3000));
    }
}
