use anyhow::Result;
use colored::*;
use rustyline::{DefaultEditor, Result as RustylineResult};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::config::ReplConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplCommand {
    // Game Operations
    Challenge { amount: u64 },
    Accept { challenge_id: String },
    List,
    Status,
    
    // Token Management
    Balance,
    Mint { amount: u64 },
    Unlock { token_id: String },
    
    // System Operations
    Help,
    Config { key: String, value: String },
    
    // Backup Operations
    Backup { file_path: String },
    Restore { file_path: String },
    VerifyKeys,
    
    Quit,
}

#[derive(Debug, Clone)]
pub struct GameStatus {
    pub current_status: String,
    pub active_games_count: usize,
    pub pending_challenges: usize,
    pub player_pubkey: String,
    pub mint_pubkey: String,
    pub game_token_balance: u64,
    pub reward_token_balance: u64,
}

pub struct ReplInterface {
    editor: DefaultEditor,
    command_sender: mpsc::UnboundedSender<ReplCommand>,
    status_receiver: mpsc::UnboundedReceiver<GameStatus>,
    config: ReplConfig,
    current_status: Option<GameStatus>,
}

impl ReplInterface {
    pub fn new(
        command_sender: mpsc::UnboundedSender<ReplCommand>,
        status_receiver: mpsc::UnboundedReceiver<GameStatus>,
        config: ReplConfig,
    ) -> Result<Self> {
        let mut editor = DefaultEditor::new()?;
        
        // Load history if configured
        if let Some(history_file) = &config.history_file {
            let _ = editor.load_history(history_file);
        }
        
        Ok(Self {
            editor,
            command_sender,
            status_receiver,
            config,
            current_status: None,
        })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        self.display_welcome();
        
        loop {
            // Check for status updates
            while let Ok(status) = self.status_receiver.try_recv() {
                self.current_status = Some(status);
            }
            
            // Display prompt with current status
            self.display_prompt();
            
            // Read user input
            let input = match self.read_input() {
                Ok(input) => input,
                Err(_) => {
                    println!("\nGoodbye!");
                    break;
                }
            };
            
            // Parse and handle command
            match self.parse_command(&input) {
                Ok(ReplCommand::Quit) => {
                    println!("Goodbye!");
                    break;
                }
                Ok(command) => {
                    if let Err(e) = self.command_sender.send(command) {
                        eprintln!("{}", format!("Error sending command: {}", e).red());
                    }
                }
                Err(e) => {
                    eprintln!("{}", format!("Error: {}", e).red());
                }
            }
        }
        
        // Save history if configured
        if let Some(history_file) = &self.config.history_file {
            let _ = self.editor.save_history(history_file);
        }
        
        Ok(())
    }
    
    fn display_welcome(&self) {
        println!("{}", "Welcome to Kirk Cards!".bright_blue().bold());
        println!("{}", "A CLI card game demonstrating the Kirk gaming protocol".cyan());
        println!("{}", "Type 'help' for available commands".dimmed());
        println!();
    }
    
    fn display_prompt(&self) {
        if let Some(status) = &self.current_status {
            println!("{}", format!("Status: {}", status.current_status).dimmed());
            println!("{}", format!(
                "Balances: {} Game tokens, {} Reward tokens", 
                status.game_token_balance.to_string().green(),
                status.reward_token_balance.to_string().yellow()
            ).dimmed());
        }
    }
    
    fn read_input(&mut self) -> RustylineResult<String> {
        self.editor.readline(&self.config.prompt)
    }
    
    fn parse_command(&self, input: &str) -> Result<ReplCommand> {
        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        
        match parts.as_slice() {
            ["challenge", amount] => Ok(ReplCommand::Challenge { 
                amount: amount.parse()? 
            }),
            ["challenge"] => Ok(ReplCommand::Challenge { 
                amount: 100 // Default amount
            }),
            ["accept", id] => Ok(ReplCommand::Accept { 
                challenge_id: id.to_string() 
            }),
            ["list"] => Ok(ReplCommand::List),
            ["status"] => Ok(ReplCommand::Status),
            ["balance"] => Ok(ReplCommand::Balance),
            ["mint", amount] => Ok(ReplCommand::Mint { 
                amount: amount.parse()? 
            }),
            ["mint"] => Ok(ReplCommand::Mint { 
                amount: 100 // Default amount
            }),
            ["unlock", token_id] => Ok(ReplCommand::Unlock { 
                token_id: token_id.to_string() 
            }),
            ["help"] => Ok(ReplCommand::Help),
            ["config", key, value] => Ok(ReplCommand::Config { 
                key: key.to_string(), 
                value: value.to_string() 
            }),
            ["backup", file_path] => Ok(ReplCommand::Backup { 
                file_path: file_path.to_string() 
            }),
            ["restore", file_path] => Ok(ReplCommand::Restore { 
                file_path: file_path.to_string() 
            }),
            ["verify"] | ["verify-keys"] => Ok(ReplCommand::VerifyKeys),
            ["quit"] | ["exit"] => Ok(ReplCommand::Quit),
            [] => Err(anyhow::anyhow!("Empty command")),
            _ => Err(anyhow::anyhow!(
                "Unknown command: '{}'. Type 'help' for available commands.", 
                input.trim()
            )),
        }
    }
}