use std::process::Command;
use std::ffi::OsStr;
use std::path::Path;
use crate::Opt;
use crate::util::CommandExt;
use std::process;

pub(crate) fn build_args(project_dir: &Path, cargo_registry: &Path, opt: &Opt) -> Vec<String> {
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

pub fn manage_build_volume() {
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

pub fn check() {
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

fn build_volume_name() -> String {
    let cwd = std::env::current_dir().expect("Can't get cwd");
    let basename = cwd
        .file_name()
        .and_then(OsStr::to_str)
        .expect("Can't get basename from cwd");
    format!("rust-build-volume-{}", basename)
}