use std::{
    fs::{self},
    process,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::json::json_parser::parse_json;
use crate::{
    cli::config::Config, fuzzer::cairo_worker::CairoWorker, fuzzer::dict::Dict,
    json::json_parser::Function,
};

use super::{corpus_crash::CrashFile, corpus_input::InputFile, stats::Statistics};
use cairo_rs::types::program::Program;
use felt::Felt252;
use rand::Rng;

#[derive(Clone)]
pub struct Fuzzer {
    pub stats: Arc<Mutex<Statistics>>,
    pub cores: i32,
    pub contract_file: String,
    pub contract_content: String,
    pub program: Option<Program>,
    pub function: Function,
    pub seed: u64,
    pub workspace: String,
    pub input_file: Arc<Mutex<InputFile>>,
    pub crash_file: Arc<Mutex<CrashFile>>,
    pub run_time: Option<u64>,
    pub start_time: Instant,
    pub running_workers: u64,
    pub iter: i64,
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

        let function = match parse_json(&contents, &config.function_name) {
            Some(func) => func,
            None => {
                eprintln!("Error: Could not parse json file");
                process::exit(1)
            }
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
            value_vec.push(val.clone());
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

        let program = Some(
            Program::from_bytes(&contents.as_bytes(), Some(&function.name))
                .expect("Failed to deserialize Program"),
        );

        // Setup the mutex for the inputs corpus and crash corpus
        let inputs = Arc::new(Mutex::new(inputs));
        let crashes = Arc::new(Mutex::new(crashes));

        // Setup the fuzzer
        Fuzzer {
            stats,
            cores: config.cores,
            run_time: config.run_time,
            contract_file: config.contract_file.clone(),
            contract_content: contents,
            program,
            dict,
            function: function.clone(),
            start_time: Instant::now(),
            seed,
            input_file: inputs,
            crash_file: crashes,
            workspace: config.workspace.clone(),
            running_workers: 0,
            iter: config.iter,
        }
    }

    /// Fuzz
    pub fn fuzz(&mut self) {
        // Running all the threads
        for i in 0..self.cores {
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
        loop {
            // wait 1 second
            std::thread::sleep(Duration::from_millis(1000));

            // Get uptime
            let uptime = (Instant::now() - self.start_time).as_secs_f64();

            // Get access to the global stats

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
            if let Some(run_time) = self.run_time {
                if uptime > run_time as f64 {
                    process::exit(0);
                }
            }
        }
    }
}
