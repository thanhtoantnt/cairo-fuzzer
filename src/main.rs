use clap::Parser;
use std::process;

mod cli;
mod custom_rand;
mod fuzzer;
mod json;
mod mutator;
mod runner;

use cli::args::Opt;
use cli::config::Config;
use fuzzer::fuzzer::Fuzzer;

use log::error;
fn main() {
    let opt = Opt::parse();
    let config = {
        if opt.contract.len() == 0 {
            error!("Fuzzer needs a contract path using --contract");
            process::exit(1);
        }
        if opt.function.len() == 0 {
            error!("Fuzzer needs a function name to fuzz using --function");
            process::exit(1);
        }

        Config {
            workspace: opt.workspace,
            contract_file: opt.contract,
            function_name: opt.function,
            input_file: opt.inputfile,
            crash_file: opt.crashfile,
            input_folder: opt.inputfolder,
            crash_folder: opt.crashfolder,
            dict: opt.dict,
            cores: opt.cores,
            seed: opt.seed,
            iter: opt.iter,
        }
    };

    let mut fuzzer = Fuzzer::new(&config);

    fuzzer.fuzz();
}
