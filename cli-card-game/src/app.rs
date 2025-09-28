use anyhow::Result;
use bevy::prelude::*;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::GameConfig;
use crate::keys::MasterKeyManager;
use crate::repl::{GameStatus, ReplCommand, ReplInterface};
use crate::resources::{GameStatusDisplay, ReplCommandQueue};

pub struct GameApp {
    bevy_app: App,
    config: GameConfig,
    command_receiver: mpsc::UnboundedReceiver<ReplCommand>,
    status_sender: mpsc::UnboundedSender<GameStatus>,
}

impl GameApp {
    pub async fn new(config: GameConfig) -> Result<Self> {
        // Create communication channels
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        let (status_sender, status_receiver) = mpsc::unbounded_channel();
        
        // Initialize master key manager
        let key_manager = MasterKeyManager::load_or_generate(config.master_seed_file.as_deref()).await?;
        
        // Create Bevy app with minimal plugins (no rendering/audio)
        let mut bevy_app = App::new();
        bevy_app.add_plugins(MinimalPlugins);
        
        // Insert configuration as resource
        bevy_app.insert_resource(config.clone());
        
        // Insert master key manager as resource
        bevy_app.insert_resource(key_manager);
        
        // Insert command queue resource
        bevy_app.insert_resource(ReplCommandQueue {
            commands: std::collections::VecDeque::new(),
        });
        
        // Insert status display resource
        bevy_app.insert_resource(GameStatusDisplay::default());
        
        // Add startup systems
        bevy_app.add_systems(Startup, (
            setup_logging,
            initialize_placeholder_systems,
        ));
        
        // Add update systems (placeholder for now)
        bevy_app.add_systems(Update, (
            process_commands_placeholder,
            update_status_display_placeholder,
        ));
        
        // Start REPL in separate task
        let repl_config = crate::config::ReplConfig::default();
        let mut repl = ReplInterface::new(command_sender, status_receiver, repl_config)?;
        
        tokio::spawn(async move {
            if let Err(e) = repl.run().await {
                eprintln!("REPL error: {}", e);
            }
        });
        
        Ok(Self {
            bevy_app,
            config,
            command_receiver,
            status_sender,
        })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        println!("Starting Kirk Cards CLI...");
        println!("Mint port: {}", self.config.mint_port);
        println!("Relay port: {}", self.config.relay_port);
        
        // Main game loop
        loop {
            // Process any incoming REPL commands
            while let Ok(command) = self.command_receiver.try_recv() {
                if let Some(mut queue) = self.bevy_app.world_mut().get_resource_mut::<ReplCommandQueue>() {
                    queue.commands.push_back(command);
                }
            }
            
            // Update Bevy ECS world
            self.bevy_app.update();
            
            // Send status updates to REPL
            if let Some(status_display) = self.bevy_app.world().get_resource::<GameStatusDisplay>() {
                let status = GameStatus {
                    current_status: status_display.current_status.clone(),
                    active_games_count: status_display.active_games_count,
                    pending_challenges: status_display.pending_challenges,
                    player_pubkey: status_display.player_pubkey.clone(),
                    mint_pubkey: status_display.mint_pubkey.clone(),
                    game_token_balance: status_display.game_token_balance,
                    reward_token_balance: status_display.reward_token_balance,
                };
                
                let _ = self.status_sender.send(status);
            }
            
            // Small delay to prevent busy waiting
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

// Resources are now defined in resources.rs module

// Placeholder systems (will be implemented in later tasks)
fn setup_logging() {
    println!("Setting up logging...");
}

fn initialize_placeholder_systems() {
    println!("Initializing placeholder systems...");
}

fn process_commands_placeholder(
    mut queue: ResMut<ReplCommandQueue>,
    mut status: ResMut<GameStatusDisplay>,
    key_manager: Res<MasterKeyManager>,
) {
    while let Some(command) = queue.commands.pop_front() {
        match command {
            ReplCommand::Help => {
                println!("Available commands:");
                println!("  challenge [amount] - Create a new challenge");
                println!("  accept <id>        - Accept a challenge");
                println!("  list               - List available challenges");
                println!("  status             - Show current status");
                println!("  balance            - Show token balances");
                println!("  mint [amount]      - Mint new Game tokens");
                println!("  unlock <token_id>  - Unlock Reward tokens");
                println!("  backup <file>      - Create backup of master seed");
                println!("  restore <file>     - Restore from backup file");
                println!("  verify-keys        - Verify key derivation integrity");
                println!("  help               - Show this help");
                println!("  quit               - Exit the application");
            }
            ReplCommand::Status => {
                status.current_status = "Ready for commands".to_string();
            }
            ReplCommand::Balance => {
                println!("Game tokens: {}", status.game_token_balance);
                println!("Reward tokens: {}", status.reward_token_balance);
            }
            ReplCommand::Backup { file_path } => {
                // Spawn async task to handle backup
                let key_manager_clone = key_manager.clone();
                let file_path_clone = file_path.clone();
                tokio::spawn(async move {
                    match key_manager_clone.save_backup_to_file(&file_path_clone).await {
                        Ok(()) => println!("✅ Backup saved to {}", file_path_clone),
                        Err(e) => eprintln!("❌ Failed to save backup: {}", e),
                    }
                });
            }
            ReplCommand::Restore { file_path } => {
                println!("⚠️  Restore functionality requires application restart");
                println!("   Use --seed-file {} when starting the application", file_path);
            }
            ReplCommand::VerifyKeys => {
                match key_manager.verify_key_derivation() {
                    Ok(()) => println!("✅ Key derivation verification passed"),
                    Err(e) => eprintln!("❌ Key derivation verification failed: {}", e),
                }
            }
            _ => {
                println!("Command received: {:?} (not yet implemented)", command);
            }
        }
    }
}

fn update_status_display_placeholder(
    mut status: ResMut<GameStatusDisplay>,
    key_manager: Res<MasterKeyManager>,
) {
    // Update status with actual keys from MasterKeyManager
    if status.current_status == "Starting..." {
        status.update_status("Ready");
        status.update_keys(
            &key_manager.get_player_keys().public_key().to_string(),
            &key_manager.get_mint_keys().public_key().to_string()
        );
    }
}