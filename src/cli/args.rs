use clap::{self, Parser};

#[derive(Debug, Parser)]
pub struct Opt {
    #[arg(
        long,
        help = "Set the number of threads to run",
        name = "CORES",
        default_value = "1"
    )]
    pub cores: i32,

    #[arg(
        long,
        help = "Set the path of the JSON artifact to load",
        name = "CONTRACT",
        default_value = ""
    )]
    pub contract: String,

    #[arg(
        long,
        help = "Set the function to fuzz",
        name = "FUNCTION",
        default_value = ""
    )]
    pub function: String,

    #[arg(
        long,
        help = "Workspace of the fuzzer",
        name = "WORKSPACE",
        default_value = "fuzzer_workspace"
    )]
    pub workspace: String,

    #[arg(
        long,
        help = "Path to the inputs folder to load",
        name = "INPUTFOLDER",
        default_value = ""
    )]
    pub inputfolder: String,

    #[arg(
        long,
        help = "Path to the crashes folder to load",
        name = "CRASHFOLDER",
        default_value = ""
    )]
    pub crashfolder: String,

    #[arg(
        long,
        help = "Path to the inputs file to load",
        name = "INPUTFILE",
        default_value = ""
    )]
    pub inputfile: String,

    #[arg(
        long,
        help = "Path to the crashes file to load",
        name = "CRASHFILE",
        default_value = ""
    )]
    pub crashfile: String,

    #[arg(
        long,
        help = "Path to the dictionnary file to load",
        name = "DICT",
        default_value = ""
    )]
    pub dict: String,

    #[arg(
        long,
        help = "Enable fuzzer logs in file",
        name = "LOGS",
        default_value = "false"
    )]
    pub logs: bool,

    #[arg(
        long,
        help = "Set a custom seed (only applicable for 1 core run)",
        name = "SEED"
    )]
    pub seed: Option<u64>,

    #[arg(long, help = "Iteration Number", name = "ITER", default_value = "-1")]
    pub iter: i64,
}
