//! Complete game flow demonstration
//! 
//! This binary demonstrates a complete game from start to finish,
//! showing how C values are decoded into game pieces and how the
//! entire protocol works together.

mod games;
use games::demonstrate_complete_game_flow;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    demonstrate_complete_game_flow().await?;
    Ok(())
}