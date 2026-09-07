use ac_server::config::Settings;
use config::{Config, Environment};

fn main() {
    let builder = Config::builder().add_source(Environment::with_prefix("AC").separator("_"));

    let cfg = builder.build().unwrap();
    println!("Config dump: {:?}", cfg);
    println!("Parsed: {:?}", cfg.try_deserialize::<Settings>());
}
