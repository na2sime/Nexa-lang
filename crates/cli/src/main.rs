mod application;
mod interfaces;

#[tokio::main]
async fn main() {
    interfaces::cli::run().await;
}
