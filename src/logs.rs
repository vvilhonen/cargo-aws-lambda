use rusoto_core::credential::StaticProvider;
use rusoto_core::{DefaultCredentialsProvider, HttpClient, Region};
use rusoto_logs::{CloudWatchLogs, CloudWatchLogsClient, FilterLogEventsRequest};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use crate::Opt;

pub(crate) fn create_client(opt: &Opt, region: &str) -> CloudWatchLogsClient {
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

pub fn tail(
    logs_client: &CloudWatchLogsClient,
    function_name: &str,
) -> Result<(), Box<dyn ::std::error::Error>> {
    let unix = || {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            - 5 * 60 * 1000
    };
    let user_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
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
                let ts = event.timestamp.unwrap_or(::std::i64::MAX);
                if !seen.contains(event.event_id.as_ref().unwrap()) && ts > user_time {
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
