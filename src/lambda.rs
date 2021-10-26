use rusoto_core::{HttpClient, Region};
use crate::Opt;
use rusoto_lambda::LambdaClient;
use rusoto_core::credential::{ChainProvider, ProfileProvider, StaticProvider};
use std::str::FromStr;

pub(crate) fn create_client(opt: &Opt, region: &str) -> LambdaClient {
    let dispatcher = HttpClient::new().expect("failed to create request dispatcher");
    let region = Region::from_str(region).unwrap();

    match (&opt.access_key, &opt.secret_key, &opt.profile) {
        (Some(access_key), Some(secret_key), _) => {
            let creds = StaticProvider::new_minimal(access_key.to_owned(), secret_key.to_owned());
            LambdaClient::new_with(dispatcher, creds, region)
        },
        (_, _, Some(profile)) => {
            let mut creds = ProfileProvider::new().unwrap();
            creds.set_profile(profile.to_owned());
            LambdaClient::new_with(dispatcher, creds, region)
        },
        _ => {
            let creds = ChainProvider::new();
            LambdaClient::new_with(dispatcher, creds, region)
        }
    }
}