use crate::cairo_vm::cairo_runner::runner;
use crate::cairo_vm::cairo_types::Felt;
use crate::fuzzer::stats::*;
use std::sync::Arc;

use super::fuzzer::Fuzzer;

pub fn replay(worker_id: usize, fuzzing_data: Arc<Fuzzer>, inputs: Vec<Vec<Felt>>) {
    // Local stats database
    let stats = &fuzzing_data.stats;
    let mut local_stats = Statistics::default();
    let contents = &fuzzing_data.contract_content;
    let function = &fuzzing_data.function;
    for input in inputs {
        let fuzz_input = Arc::new(input.clone());
        match runner(&contents, &function.name, &input.clone()) {
            Ok(traces) => {
                let mut vec_trace: Vec<(u32, u32)> = vec![];
                for trace in traces.unwrap() {
                    vec_trace.push((
                        trace.0.offset.try_into().unwrap(),
                        trace.1.offset.try_into().unwrap(),
                    ));
                }

                // Mutex locking is limited to this scope
                {
                    let stats = stats.lock().unwrap();
                    // verify if new input has been found by other fuzzers
                    // if so, update our statistics
                    if local_stats.input_db.len() != stats.input_db.len() {
                        local_stats.coverage_db = stats.coverage_db.clone();
                        local_stats.input_db = stats.input_db.clone();
                        local_stats.crash_db = stats.crash_db.clone();
                    }
                }

                // Check if this coverage entry is something we've never seen before
                if !local_stats.coverage_db.contains_key(&vec_trace) {
                    // Coverage entry is new, save the fuzz input in the input database
                    local_stats.input_db.insert(fuzz_input.clone());

                    // Update the module+offset in the coverage database to reflect that this input caused this coverage to occur
                    local_stats
                        .coverage_db
                        .insert(vec_trace.clone(), fuzz_input.clone());

                    // Mutex locking is limited to this scope
                    {
                        // Get access to global stats
                        let mut stats = stats.lock().unwrap();

                        if !stats.coverage_db.contains_key(&vec_trace) {
                            // Save input to global input database
                            if stats.input_db.insert(fuzz_input.clone()) {
                                stats.input_list.push(fuzz_input.clone());
                                stats.input_len += 1;
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
                    let mut stats = stats.lock().unwrap();
                    local_stats.crashes += 1;
                    stats.crashes += 1;
                    // Check if this case ended due to a crash
                    // Add the crashing input to the input databases
                    local_stats.input_db.insert(fuzz_input.clone());
                    if stats.input_db.insert(fuzz_input.clone()) {
                        stats.input_list.push(fuzz_input.clone());
                        stats.input_len += 1;
                    }
                    // Add the crash name and corresponding fuzz input to the crash database
                    local_stats.crash_db.insert(fuzz_input.clone());
                    if stats.crash_db.insert(fuzz_input.clone()) {
                        // add input to the crash corpus
                        println!(
                            "WORKER {} -- INPUT => {:?} -- ERROR \"{:?}\"",
                            worker_id, &input, e
                        );
                    }
                }
            }
        }

        // Get access to global stats
        let mut stats = stats.lock().unwrap();
        // Update fuzz case count
        stats.fuzz_cases += 1;
        local_stats.fuzz_cases += 1;
    }
    {
        // Update the threads_finished when the worker executes all the corpus chunk
        let mut stats = stats.lock().unwrap();
        stats.threads_finished += 1;
    }
}
