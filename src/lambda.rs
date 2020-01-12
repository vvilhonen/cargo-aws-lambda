use rusoto_core::{HttpClient, Region, DefaultCredentialsProvider};
use crate::Opt;
use rusoto_lambda::LambdaClient;
use rusoto_core::credential::StaticProvider;
use std::str::FromStr;

pub(crate) fn create_client(opt: &Opt, region: &str) -> LambdaClient {
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