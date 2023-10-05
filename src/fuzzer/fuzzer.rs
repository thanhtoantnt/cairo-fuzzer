use std::{
    fs::{self},
    process,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::json::json_parser::parse_json;
use crate::{cli::config::Config, fuzzer::cairo_worker::CairoWorker, json::json_parser::Function};

use super::{corpus_input::InputFile, stats::Statistics};
use cairo_rs::types::program::Program;

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
    // pub crash_file: Arc<Mutex<CrashFile>>,
    pub running_workers: u64,
    pub iter: i64,
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
        let inputs = InputFile::load_from_folder(&config.input_folder, &config.workspace);
        println!("\t\t\t\t\t\t\tInputs loaded {}", inputs.inputs.len());

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

        // let crashes: CrashFile =
        //     match config.crash_file.is_empty() && config.crash_folder.is_empty() {
        //         true => CrashFile::new_from_function(&function, &config.workspace),
        //         false => match config.input_folder.is_empty() {
        //             true => CrashFile::load_from_file(&config.input_file, &config.workspace),
        //             false => CrashFile::load_from_folder(&config.input_folder, &config.workspace),
        //         },
        //     };

        // if crashes.crashes.len() > 0 {
        //     let mut stats_db = stats.lock().expect("Failed to lock stats mutex");
        //     for input in &crashes.crashes {
        //         stats_db.crash_db.insert(Arc::new(input.clone()));
        //         stats_db.crashes += 1;
        //     }
        // }

        let program = Some(
            Program::from_bytes(&contents.as_bytes(), Some(&function.name))
                .expect("Failed to deserialize Program"),
        );

        // Setup the mutex for the inputs corpus and crash corpus
        let inputs = Arc::new(Mutex::new(inputs));
        // let crashes = Arc::new(Mutex::new(crashes));

        // Setup the fuzzer
        Fuzzer {
            stats,
            cores: config.cores,
            contract_file: config.contract_file.clone(),
            contract_content: contents,
            program,
            function: function.clone(),
            seed,
            input_file: inputs,
            // crash_file: crashes,
            workspace: config.workspace.clone(),
            running_workers: 0,
            iter: config.iter,
        }
    }

    /// Fuzz
    pub fn fuzz(&mut self) {
        for i in 0..self.cores {
            let stats = self.stats.clone();
            let function = self.function.clone();
            let input_file = self.input_file.clone();
            // let crash_file = self.crash_file.clone();
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
                    // crash_file,
                    iter,
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
            std::thread::sleep(Duration::from_millis(1000));
            let stats = self.stats.lock().expect("Failed to lock stats mutex");
            let fuzz_case = stats.fuzz_cases;
            print!(
                "{:9} fuzz cases | {:12.2} fcps | \
                            {:6} inputs | {:6} crashes [{:6} unique]\n",
                fuzz_case,
                stats.coverage_db.len(),
                stats.input_len,
                stats.crashes,
                stats.crash_db.len()
            );
        }
    }
}
