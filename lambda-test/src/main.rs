use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::TcpStream;

async fn handler(_event: LambdaEvent<Value>) -> Result<Value, Error> {
    println!("Testing TCP connection...");
    let host = "database-1.cluster-ce5gog82gxo1.us-east-1.rds.amazonaws.com:5432";
    match tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(host)).await {
        Ok(Ok(_)) => {
            println!("Connected successfully!");
            Ok(json!({"status": "connected"}))
        }
        Ok(Err(e)) => {
            println!("TCP connection failed: {:?}", e);
            Ok(json!({"status": "tcp_error", "error": e.to_string()}))
        }
        Err(_) => {
            println!("TCP connection timed out after 10s!");
            Ok(json!({"status": "timeout"}))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    lambda_runtime::run(service_fn(handler)).await
}
