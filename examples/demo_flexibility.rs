//! Framework flexibility demonstration
//! 
//! This binary shows how the same framework can support different
//! game types with different mechanics and C value interpretations.

mod games;
use games::demonstrate_framework_flexibility;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    demonstrate_framework_flexibility()?;
    Ok(())
}