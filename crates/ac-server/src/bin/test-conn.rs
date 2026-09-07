use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() {
    let url = "postgresql://postgres:dummy@database-1.cluster-ce5gog82gxo1.us-east-1.rds.amazonaws.com:5432/aishelter?sslmode=require";
    println!("Connecting to {}...", url);
    match PgPoolOptions::new().max_connections(10).connect(url).await {
        Ok(_) => println!("Connected successfully!"),
        Err(e) => println!("Connection failed: {:?}", e),
    }
}
