use serde::{Deserialize, Serialize};

/// Config struct to use instead of command line
#[derive(Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub workspace: String,
    pub contract_file: String,
    pub function_name: String,
    pub input_file: String,
    pub crash_file: String,
    pub input_folder: String,
    pub crash_folder: String,
    pub dict: String,
    pub cores: i32,
    pub seed: Option<u64>,
    pub iter: i64,
}
