use crate::mutator::mutator::{EmptyDatabase, Mutator};
use crate::runner::runner::Runner;
use cairo_rs::types::program::Program;
use felt::Felt252;
use std::process::exit;
use std::sync::{Arc, Mutex};

use super::{corpus_crash::CrashFile, corpus_input::InputFile};
//use super::dict::Dict;
use super::stats::*;

use crate::custom_rand::rng::Rng;
use crate::json::json_parser::Function;
use crate::runner::cairo_runner;
use thiserror::Error;

#[derive(Debug, PartialEq, Error)]
pub enum CairoworkerError {
    // TODO implem
}

pub struct CairoWorker {
    stats: Arc<Mutex<Statistics>>,
    worker_id: i32,
    program: Program,
    function: Function,
    seed: u64,
    input_file: Arc<Mutex<InputFile>>,
    crash_file: Arc<Mutex<CrashFile>>,
    iter: i64,
    //dict: Dict,
}

impl CairoWorker {
    pub fn new(
        stats: Arc<Mutex<Statistics>>,
        worker_id: i32,
        program: Program,
        function: Function,
        seed: u64,
        input_file: Arc<Mutex<InputFile>>,
        crash_file: Arc<Mutex<CrashFile>>,
        iter: i64,
        //dict: Dict,
    ) -> Self {
        CairoWorker {
            stats,
            worker_id,
            program,
            function,
            seed: seed,
            input_file,
            crash_file,
            iter,
            //dict,
        }
    }

    pub fn fuzz(self) {
        // Local stats database
        let mut local_stats = Statistics::default();
        // Create an RNG for this thread, seed is unique per thread
        // to prevent duplication of efforts
        let rng = Rng::seeded(self.seed);

        // Create a mutator
        let mut mutator = Mutator::new()
            .seed(self.seed)
            .max_input_size(self.function.num_args as usize);
        let cairo_runner = cairo_runner::RunnerCairo::new(&self.program);
        'next_case: loop {
            // clear previous data
            mutator.input.clear();
            if local_stats.input_len > 0 {
                let index: usize = rng.rand_usize() % local_stats.input_len;
                // pick from feedback corpora
                mutator
                    .input
                    .extend_from_slice(&local_stats.get_input_by_index(index));
            } else {
                mutator.input.extend_from_slice(&vec![
                    Felt252::from(b'\0');
                    self.function.num_args as usize
                ]);
            }

            // Corrupt it with 4 mutation passes
            //if self.dict.inputs.is_empty() {
            mutator.mutate(4, &EmptyDatabase);

            // not the good size, drop this input
            if mutator.input.len() != self.function.num_args as usize {
                println!(
                    "Corrupted input size {} != {}",
                    mutator.input.len(),
                    self.function.num_args
                );
                continue 'next_case;
            }

            // Wrap up the fuzz input in an `Arc`
            let fuzz_input = Arc::new(mutator.input.clone());
            //println!("Inputs =>>> {:?}", &mutator.input);
            // run the cairo vm
            match cairo_runner
                .clone()
                .runner(&self.function.name, &mutator.input)
            {
                Ok(traces) => {
                    let vec_trace: Vec<(u32, u32)> = traces.expect("Could not get traces");

                    // Mutex locking is limited to this scope
                    {
                        let stats = self.stats.lock().expect("Failed to get mutex");
                        if self.iter > 0 && self.iter < stats.fuzz_cases as i64 {
                            return;
                        }
                        // verify if new input has been found by other fuzzers
                        // if so, update our statistics
                        if local_stats.input_len != stats.input_len {
                            local_stats.coverage_db = stats.coverage_db.clone();
                            local_stats.input_len = stats.input_len;
                            local_stats.input_db = stats.input_db.clone();
                            local_stats.input_list = stats.input_list.clone();
                            local_stats.crash_db = stats.crash_db.clone();
                        }
                    }

                    // Mutex locking is limited to this scope
                    {
                        // Check if this coverage entry is something we've never seen before
                        if !local_stats.coverage_db.contains_key(&vec_trace) {
                            // Coverage entry is new, save the fuzz input in the input database
                            local_stats.input_db.insert(fuzz_input.clone());

                            // Update the module+offset in the coverage database to reflect that this input caused this coverage to occur
                            local_stats
                                .coverage_db
                                .insert(vec_trace.clone(), fuzz_input.clone());

                            // Get access to global stats
                            let mut stats = self.stats.lock().expect("Failed to get mutex");

                            if !stats.coverage_db.contains_key(&vec_trace) {
                                // Save input to global input database
                                if stats.input_db.insert(fuzz_input.clone()) {
                                    // Copy in the input list
                                    stats.input_list.push(fuzz_input.clone());
                                    stats.input_len += 1;
                                    // Copy locally
                                    let mut input_file_lock =
                                        self.input_file.lock().expect("Failed to get mutex");
                                    input_file_lock.inputs.push(fuzz_input.to_vec());
                                    input_file_lock.dump_json();
                                }
                                // Save coverage to global coverage database
                                stats
                                    .coverage_db
                                    .insert(vec_trace.clone(), fuzz_input.clone());
                            }
                        }
                    }
                }
                Err(e) => {
                    // Mutex locking is limited to this scope
                    {
                        // Get access to global stats
                        let mut stats = self.stats.lock().expect("Failed to get mutex");

                        // Update crash counters
                        local_stats.crashes += 1;
                        stats.crashes += 1;

                        // Check if this case ended due to a crash
                        // Add the crashing input to the input databases
                        local_stats.input_db.insert(fuzz_input.clone());
                        if stats.input_db.insert(fuzz_input.clone()) {
                            stats.input_list.push(fuzz_input.clone());
                            stats.input_len += 1;
                        }
                        // Add the crash input to the local crash database
                        local_stats.crash_db.insert(fuzz_input.clone());

                        // Add the crash input to the shared crash database
                        if stats.crash_db.insert(fuzz_input.clone()) {
                            // add input to the crash corpus
                            // New crashing input, we dump the crash on the disk
                            let mut crash_file_lock =
                                self.crash_file.lock().expect("Failed to get mutex");
                            crash_file_lock.crashes.push(fuzz_input.to_vec());
                            crash_file_lock.dump_json();

                            println!(
                                "WORKER {} -- INPUT => {:?} -- ERROR \"{:?}\"",
                                self.worker_id, &mutator.input, e
                            );

                            exit(0)
                        }
                    }
                }
            }

            // TODO - only update every 1k exec to prevent lock
            let counter_update = 1000;
            if local_stats.fuzz_cases % counter_update == 1 {
                // Get access to global stats
                let mut stats = self.stats.lock().expect("Failed to get mutex");
                // Update fuzz case count
                stats.fuzz_cases += counter_update;
            }
            local_stats.fuzz_cases += 1;
        }
    }
}
