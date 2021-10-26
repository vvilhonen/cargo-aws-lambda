use std::fs::File;
use std::io::Read;
use std::path::{PathBuf};
use std::process::Command;
use std::{env, process};
use structopt::StructOpt;
use std::fmt::Display;
use util::CommandExt;
use rusoto_lambda::{UpdateFunctionCodeRequest, Lambda};

mod docker;
mod lambda;
mod logs;
mod util;

/// Packages and deploys your project binaries to AWS Lambda
#[derive(StructOpt, Debug)]
struct Opt {
    /// AWS Profile
    #[structopt(long)]
    profile: Option<String>,
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
    docker::check();

    let mut args = env::args().collect::<Vec<_>>();
    args.remove(1);
    let opt = Opt::from_iter(args);

    if opt.use_build_volume {
        docker::manage_build_volume();
    }

    let zip_file = format!("{}.zip", opt.bin);
    let (region, func_name) = util::parse_arn_or_key(&opt.arn);
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

    let args = docker::build_args(project_dir.as_path(), cargo_registry.as_path(), &opt);

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

    let client = lambda::create_client(&opt, &region);
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
            let logs_client = logs::create_client(&opt, &region);
            let func_name = res.function_name.unwrap_or("".into());
            if let Err(e) = logs::tail(&logs_client, &func_name) {
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
