use std::{
    fs::{self, File},
    process,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    cli::config::Config,
    fuzzer::cairo_worker::CairoWorker,
    fuzzer::dict::Dict,
    json::json_parser::{parse_json, parse_starknet_json, Function},
};

use super::{corpus_crash::CrashFile, corpus_input::InputFile, stats::Statistics};
use cairo_rs::types::program::Program;
use felt::Felt252;
use rand::Rng;
use starknet_rs::services::api::contract_classes::deprecated_contract_class::ContractClass;
use std::io::Write;

#[derive(Clone)]
pub struct Fuzzer {
    /// Shared fuzzing statistics between threads
    pub stats: Arc<Mutex<Statistics>>,
    /// Number of cores/threads
    pub cores: i32,
    /// Contract JSON path
    pub contract_file: String,
    /// Contract JSON content
    pub contract_content: String,
    /// Program for cairo-rs
    pub program: Option<Program>,
    /// Contract_class for starknet-rs
    pub contract_class: Option<ContractClass>,
    /// Contract function to fuzz
    pub function: Function,
    /// Store local/on-disk logs
    pub logs: bool,
    /// Replay mode
    pub replay: bool,
    /// Corpus minimization
    pub minimizer: bool,
    /// Seed number
    pub seed: u64,
    /// Workspace to use
    pub workspace: String,
    /// Inputs file path
    pub input_file: Arc<Mutex<InputFile>>,
    /// Crashes file path
    pub crash_file: Arc<Mutex<CrashFile>>,
    /// Number of second the fuzzing session will last
    pub run_time: Option<u64>,
    /// Starting time of the fuzzer
    pub start_time: Instant,
    /// Running workers
    pub running_workers: u64,
    /// Starknet or cairo contract
    pub starknet: bool,
    /// Number of iterations to run
    pub iter: i64,
    /// Usage of property testing
    pub proptesting: bool,
    /// Dictionnary struct
    pub dict: Dict,
}

impl Fuzzer {
    /// Create the fuzzer using the given Config struct
    pub fn new(config: &Config) -> Self {
        let stats = Arc::new(Mutex::new(Statistics::default()));
        // Set seed if provided or generate a new seed using `SystemTime`
        let seed = match config.seed {
            Some(val) => val,
            None => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get actual time")
                .as_millis() as u64,
        };
        println!("\t\t\t\t\t\t\tSeed: {}", seed);

        // Read contract JSON artifact and get its content
        let contents = fs::read_to_string(&config.contract_file)
            .expect("Should have been able to read the file");

        // TODO - remove when support multiple txs
        let function = match parse_json(&contents, &config.function_name) {
            Some(func) => func,
            None => match parse_starknet_json(&contents, &config.function_name) {
                Some(func) => func,
                None => {
                    eprintln!("Error: Could not parse json file");
                    process::exit(1)
                }
            },
        };
        // Load inputs from the input file if provided
        let mut inputs = InputFile::load_from_folder(&config.input_folder, &config.workspace);
        println!("\t\t\t\t\t\t\tInputs loaded {}", inputs.inputs.len());

        let dict = match &config.dict.is_empty() {
            true => Dict { inputs: Vec::new() },
            false => Dict::read_dict(&config.dict),
        };

        let nbr_args = function.num_args;
        for val in &dict.inputs {
            let mut value_vec: Vec<Felt252> = Vec::new();
            value_vec.push(val.clone()); // to ensure that all values of the dict will be in the inputs vector
            for _ in 0..nbr_args - 1 {
                value_vec
                    .push(dict.inputs[rand::thread_rng().gen_range(0..dict.inputs.len())].clone());
            }
            inputs.inputs.push(value_vec);
        }

        // Load existing inputs in shared database
        if inputs.inputs.len() > 0 {
            let mut stats_db = stats.lock().expect("Failed to lock stats mutex");
            for input in &inputs.inputs {
                if stats_db.input_db.insert(Arc::new(input.clone())) {
                    stats_db.input_list.push(Arc::new(input.clone()));
                    stats_db.input_len += 1;
                }
            }
        }

        // Load crashes from the crash file if provided
        let crashes: CrashFile =
            match config.crash_file.is_empty() && config.crash_folder.is_empty() {
                true => CrashFile::new_from_function(&function, &config.workspace),
                false => match config.input_folder.is_empty() {
                    true => CrashFile::load_from_file(&config.input_file, &config.workspace),
                    false => CrashFile::load_from_folder(&config.input_folder, &config.workspace),
                },
            };

        // Load existing crashes in shared database
        if crashes.crashes.len() > 0 {
            let mut stats_db = stats.lock().expect("Failed to lock stats mutex");
            for input in &crashes.crashes {
                stats_db.crash_db.insert(Arc::new(input.clone()));
                stats_db.crashes += 1;
            }
        }

        let program = if !function._starknet {
            Some(
                Program::from_bytes(&contents.as_bytes(), Some(&function.name))
                    .expect("Failed to deserialize Program"),
            )
        } else {
            None
        };
        let contract_class = if function._starknet {
            Some(ContractClass::from_str(contents.as_str()).expect("could not get contractclass"))
        } else {
            None
        };

        // Setup the mutex for the inputs corpus and crash corpus
        let inputs = Arc::new(Mutex::new(inputs));
        let crashes = Arc::new(Mutex::new(crashes));

        // Setup the fuzzer
        Fuzzer {
            stats,
            cores: config.cores,
            logs: config.logs,
            run_time: config.run_time,
            replay: config.replay,
            minimizer: config.minimizer,
            contract_file: config.contract_file.clone(),
            contract_content: contents,
            program,
            dict,
            contract_class,
            function: function.clone(),
            start_time: Instant::now(),
            seed,
            input_file: inputs,
            crash_file: crashes,
            workspace: config.workspace.clone(),
            running_workers: 0,
            starknet: function._starknet,
            iter: config.iter,
            proptesting: config.proptesting,
        }
    }

    /// Fuzz
    pub fn fuzz(&mut self) {
        // Running all the threads
        for i in 0..self.cores {
            // create dedicated statistics per thread
            let stats = self.stats.clone();
            let function = self.function.clone();
            let input_file = self.input_file.clone();
            let crash_file = self.crash_file.clone();
            let program = self.program.clone();
            let seed = self.seed + (i as u64);
            let iter = self.iter;
            std::thread::spawn(move || {
                let cairo_worker = CairoWorker::new(
                    stats,
                    i,
                    program.expect("Could not get Cairo Program (None)"),
                    function,
                    seed,
                    input_file,
                    crash_file,
                    iter,
                    //dict,
                );
                cairo_worker.fuzz();
            });
            self.running_workers += 1;
        }
        println!("\t\t\t\t\t\t\tRunning {} threads", self.running_workers);
        self.monitor();
    }

    /// Function to print stats of the running fuzzer
    fn monitor(&self) {
        let mut log = None;
        if self.logs {
            log = Some(File::create("fuzz_stats.txt").expect("Failed to lock stats mutex"));
        }

        // Monitoring loop
        loop {
            // wait 1 second
            std::thread::sleep(Duration::from_millis(1000));

            // Get uptime
            let uptime = (Instant::now() - self.start_time).as_secs_f64();

            // Get access to the global stats
            {
                let stats = self.stats.lock().expect("Failed to lock stats mutex");

                // number of executions
                let fuzz_case = stats.fuzz_cases;
                print!(
                    "{:12.2} uptime | {:9} fuzz cases | {:12.2} fcps | \
                            {:6} coverage | {:6} inputs | {:6} crashes [{:6} unique]\n",
                    uptime,
                    fuzz_case,
                    fuzz_case as f64 / uptime,
                    stats.coverage_db.len(),
                    stats.input_len,
                    stats.crashes,
                    stats.crash_db.len()
                );
                // Writing inside logging file
                if let Some(ref mut file) = log {
                    write!(
                        file,
                        "{:12.0} {:7} {:8} {:5} {:6} {:6}\n",
                        uptime,
                        fuzz_case,
                        stats.coverage_db.len(),
                        stats.input_len,
                        stats.crashes,
                        stats.crash_db.len()
                    )
                    .expect("Failed to write logs in log file");
                    file.flush().expect("Failed to flush the file");
                }

                // Only for replay: all thread are finished
                if (self.replay && stats.threads_finished == self.running_workers)
                    || (self.iter < fuzz_case as i64 && self.iter != -1)
                {
                    break;
                }
            }

            // time over, fuzzing session is finished
            if let Some(run_time) = self.run_time {
                if uptime > run_time as f64 {
                    process::exit(0);
                }
            }
        }
    }
}
