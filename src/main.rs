use rusoto_core::credential::StaticProvider;
use rusoto_core::{DefaultCredentialsProvider, HttpClient, Region};
use rusoto_lambda::{Lambda, LambdaClient, UpdateFunctionCodeRequest};
use std::fs::File;
use std::io::Read;
use std::path::{PathBuf, Path};
use std::process::Command;
use std::str::FromStr;
use std::{env, process};
use structopt::StructOpt;
use toml::Value;

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
    docker_image: String
}

fn main() {
    check_docker();

    let mut args = env::args().collect::<Vec<_>>();
    args.remove(1);
    let opt = Opt::from_iter(args);

    let zip_file = format!("{}.zip", opt.bin);
    let (region, func_name) = parse_arn_or_key(&opt.arn);
    let project_dir = env::current_dir().expect("Can't read cwd.");

    let mut zip_path = project_dir.clone();
    zip_path.extend(&["target", "lambda", "release", &zip_file]);

    println!(
        "Deploying {} to {:?} {}",
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

    let args = build_docker_args(project_dir.as_path(), cargo_registry.as_path(), opt.keep_debug_info, &opt.docker_image);

    println!("Running docker with args {}", args.join(" "));

    let result = Command::new("docker")
        .args(args)
        .env("BIN", &opt.bin)
        .status();

    if !result.map(|r| r.success()).unwrap_or(false) {
        eprintln!("Running docker failed, check output above");
        process::exit(1);
    }

    let zip_data = {
        let mut zip_file = File::open(zip_path).expect("Can't open zip path");
        let mut data = Vec::new();
        zip_file.read_to_end(&mut data).unwrap();
        bytes::Bytes::from(data)
    };

    let client = create_client(&opt, &region);
    let req = UpdateFunctionCodeRequest {
        dry_run: Some(false),
        function_name: func_name.to_owned(),
        publish: Some(true),
        zip_file: Some(zip_data),
        ..Default::default()
    };
    let res = client.update_function_code(req).sync();
    println!("{:#?}", res);
}

fn create_client(opt: &Opt, region: &str) -> LambdaClient {
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
        if let Ok(mut lambda_toml_file) =  File::open("Lambda.toml") {
            let cargo_toml: Value = {
                let mut data = String::new();
                lambda_toml_file.read_to_string(&mut data).expect("Can't read ./Lambda.toml");
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

fn build_docker_args(project_dir: &Path, cargo_registry: &Path, keep_debug_info: bool, docker_image: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        format!("{}:/code", project_dir.display()),
        "-v".into(),
        format!("{}:/root/.cargo/registry", cargo_registry.display()),
    ];

    if keep_debug_info {
        args.push("-e".into());
        args.push("DEBUGINFO=1".into());
    }

    args.push(docker_image.into());
    args
}