use lambda_runtime::lambda;

fn main() {
    simple_logger::init_with_level(log::Level::Info).expect("Failed to init logger");
    lambda!(aws_lambda_hello::handler);
}
