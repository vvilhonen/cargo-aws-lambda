# cargo aws-lambda

[![crates.io](https://img.shields.io/crates/v/cargo-aws-lambda.svg)](https://crates.io/crates/cargo-aws-lambda)

Node.js and other dependency free cargo subcommand for cross-compiling, packaging and deploying code quickly to AWS Lambda. Uses [`softprops/lambda-rust:latest`](https://github.com/softprops/lambda-rust) docker image and hence requires docker in `PATH`, but that's all you need.

## Installation

Run `cargo install cargo-aws-lambda`.

## Usage

Go to your project directory and run `cargo aws-lambda <ARN> <BIN>` to deploy the code to AWS Lambda, where `ARN` is the full ARN of the Lambda function (e.g. `arn:aws:lambda:eu-north-1:1234:function:MyLambdaFunc`) and `BIN` the name of the binary (e.g. `mylambdafunc`, if you have `src/bin/mylambdafunc.rs` with a `main` function in your project).

You can find full project examples in the [examples](./examples/) directory.

The credentials are searched by Rusoto as described in [here](https://github.com/rusoto/rusoto/blob/master/AWS-CREDENTIALS.md). If you have [AWS CLI](https://aws.amazon.com/cli/) configured, most likely everything works without additional configuration. If you want to pass AWS access key and secret as parameters, you can do it at your own risk with the `--access-key` and `--secret-key` parameters. In this case, the other processes running in the system can sniff the credentials easily.

All available configuration options can be listed with the `--help` switch.

## Problems?

On windows you must enable the [shared drives](https://docs.docker.com/docker-for-windows/#shared-drives) feature for the drive your project is located in.

## How it works?

It mounts your project's directory and your `~/.cargo/registry` for the AWS Lambda rust docker image and builds it there for an architecture and system matching the target. After building and stripping symbols out of the executable, everything is packed into a zip file. The zip file is then deployed to the AWS Lambda function ARN given by you. Deploying also instructs AWS to publish the deployed version. Build artifacts generated in docker can be found in your project's `target/lambda/release` directory.