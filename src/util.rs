use std::process::Command;
use std::process;
use std::fs::File;
use std::io::Read;
use toml::Value;

pub fn parse_arn_or_key(raw: &str) -> (String, String) {
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

pub trait CommandExt {
    fn status_bool(&mut self) -> bool;
}

impl CommandExt for Command {
    fn status_bool(&mut self) -> bool {
        let result = self.status();
        result.map(|r| r.success()).unwrap_or(false)
    }
}
