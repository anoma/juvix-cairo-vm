#![forbid(unsafe_code)]
use bincode::enc::write::Writer;
use cairo_vm::air_public_input::PublicInputError;
use cairo_vm::cairo_run::{self, EncodeTraceError};
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::trace_errors::TraceError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use clap::{Parser, ValueHint};
use juvix_hint_processor::hint_processor::JuvixHintProcessor;
use program_input::ProgramInput;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[cfg(feature = "with_mimalloc")]
use mimalloc::MiMalloc;

#[cfg(feature = "with_mimalloc")]
#[global_allocator]
static ALLOC: MiMalloc = MiMalloc;

pub mod program_input;

mod juvix_hint_processor;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(value_parser, value_hint=ValueHint::FilePath)]
    pub filename: PathBuf,
    #[clap(long = "program_input", value_parser, value_hint=ValueHint::FilePath)]
    pub program_input: Option<PathBuf>,
    #[clap(long = "trace_file", value_parser)]
    pub trace_file: Option<PathBuf>,
    #[structopt(long = "print_output")]
    pub print_output: bool,
    #[structopt(long = "entrypoint", default_value = "main")]
    pub entrypoint: String,
    #[structopt(long = "memory_file")]
    pub memory_file: Option<PathBuf>,
    #[clap(long = "layout", default_value = "plain", value_parser=validate_layout)]
    pub layout: String,
    #[structopt(long = "proof_mode")]
    pub proof_mode: bool,
    #[structopt(long = "secure_run")]
    pub secure_run: Option<bool>,
    #[clap(long = "air_public_input", requires = "proof_mode")]
    pub air_public_input: Option<String>,
    #[clap(
        long = "air_private_input",
        requires_all = ["proof_mode", "trace_file", "memory_file"]
    )]
    pub air_private_input: Option<String>,
    #[clap(
        long = "cairo_pie_output",
        // We need to add these air_private_input & air_public_input or else
        // passing cairo_pie_output + either of these without proof_mode will not fail
        conflicts_with_all = ["proof_mode", "air_private_input", "air_public_input"]
    )]
    pub cairo_pie_output: Option<String>,
    #[structopt(long = "allow_missing_builtins")]
    pub allow_missing_builtins: Option<bool>,
}

fn validate_layout(value: &str) -> Result<String, String> {
    match value {
        "plain"
        | "small"
        | "dex"
        | "recursive"
        | "starknet"
        | "starknet_with_keccak"
        | "recursive_large_output"
        | "all_cairo"
        | "all_solidity"
        | "dynamic" => Ok(value.to_string()),
        _ => Err(format!("{value} is not a valid layout")),
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid arguments")]
    Cli(#[from] clap::Error),
    #[error("Failed to interact with the file system")]
    IO(#[from] std::io::Error),
    #[error("The cairo program execution failed")]
    Runner(#[from] CairoRunError),
    #[error(transparent)]
    EncodeTrace(#[from] EncodeTraceError),
    #[error(transparent)]
    VirtualMachine(#[from] VirtualMachineError),
    #[error(transparent)]
    Trace(#[from] TraceError),
    #[error(transparent)]
    PublicInput(#[from] PublicInputError),
    #[error(transparent)]
    PrivateInput(#[from] serde_json::Error),
}

struct FileWriter {
    buf_writer: io::BufWriter<std::fs::File>,
    bytes_written: usize,
}

impl Writer for FileWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        self.buf_writer
            .write_all(bytes)
            .map_err(|e| bincode::error::EncodeError::Io {
                inner: e,
                index: self.bytes_written,
            })?;

        self.bytes_written += bytes.len();

        Ok(())
    }
}

impl FileWriter {
    fn new(buf_writer: io::BufWriter<std::fs::File>) -> Self {
        Self {
            buf_writer,
            bytes_written: 0,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf_writer.flush()
    }
}

// The anoma_cairo_vm_runner is used in Anoma to return output, trace, and memory.
pub fn anoma_cairo_vm_runner(
    program_content: &[u8],
    program_input: ProgramInput,
) -> Result<(String, Vec<u8>, Vec<u8>), Error> {
    let mut hint_executor = JuvixHintProcessor::new(program_input);

    let cairo_run_config = cairo_run::CairoRunConfig {
        trace_enabled: true,
        relocate_mem: true,
        proof_mode: true,
        layout: "all_cairo",
        ..Default::default()
    };

    let (cairo_runner, mut vm) =
        cairo_run::cairo_run(program_content, &cairo_run_config, &mut hint_executor)?;

    let mut output_buffer = "".to_string();
    vm.write_output(&mut output_buffer)?;

    let trace = {
        let relocated_trace = cairo_runner
            .relocated_trace
            .as_ref()
            .ok_or(Error::Trace(TraceError::TraceNotRelocated))?;
        let mut output: Vec<u8> = Vec::with_capacity(3 * 1024 * 1024);
        for entry in relocated_trace.iter() {
            output.extend_from_slice(&(entry.ap as u64).to_le_bytes());
            output.extend_from_slice(&(entry.fp as u64).to_le_bytes());
            output.extend_from_slice(&(entry.pc as u64).to_le_bytes());
        }
        output
    };

    let memory = {
        let mut output: Vec<u8> = Vec::with_capacity(1024 * 1024);
        for (i, entry) in cairo_runner.relocated_memory.iter().enumerate() {
            match entry {
                None => continue,
                Some(unwrapped_memory_cell) => {
                    output.extend_from_slice(&(i as u64).to_le_bytes());
                    output.extend_from_slice(&unwrapped_memory_cell.to_bytes_le());
                }
            }
        }
        output
    };

    Ok((output_buffer, trace, memory))
}

// Returns the program output
pub fn run(args: Args, program_input: ProgramInput) -> Result<String, Error> {
    let trace_enabled = args.trace_file.is_some() || args.air_public_input.is_some();
    let mut hint_executor = JuvixHintProcessor::new(program_input);
    let cairo_run_config = cairo_run::CairoRunConfig {
        entrypoint: &args.entrypoint,
        trace_enabled,
        relocate_mem: args.memory_file.is_some() || args.air_public_input.is_some(),
        layout: &args.layout,
        proof_mode: args.proof_mode,
        secure_run: args.secure_run,
        allow_missing_builtins: args.allow_missing_builtins,
        ..Default::default()
    };

    let program_content = std::fs::read(args.filename).map_err(Error::IO)?;

    let (cairo_runner, mut vm) =
        cairo_run::cairo_run(&program_content, &cairo_run_config, &mut hint_executor)?;

    let mut output_buffer = "".to_string();
    vm.write_output(&mut output_buffer)?;

    if let Some(ref trace_path) = args.trace_file {
        let relocated_trace = cairo_runner
            .relocated_trace
            .as_ref()
            .ok_or(Error::Trace(TraceError::TraceNotRelocated))?;

        let trace_file = std::fs::File::create(trace_path)?;
        let mut trace_writer =
            FileWriter::new(io::BufWriter::with_capacity(3 * 1024 * 1024, trace_file));

        cairo_run::write_encoded_trace(relocated_trace, &mut trace_writer)?;
        trace_writer.flush()?;
    }

    if let Some(ref memory_path) = args.memory_file {
        let memory_file = std::fs::File::create(memory_path)?;
        let mut memory_writer =
            FileWriter::new(io::BufWriter::with_capacity(5 * 1024 * 1024, memory_file));

        cairo_run::write_encoded_memory(&cairo_runner.relocated_memory, &mut memory_writer)?;
        memory_writer.flush()?;
    }

    if let Some(file_path) = args.air_public_input {
        let json = cairo_runner.get_air_public_input(&vm)?.serialize_json()?;
        std::fs::write(file_path, json)?;
    }

    if let (Some(file_path), Some(ref trace_file), Some(ref memory_file)) =
        (args.air_private_input, args.trace_file, args.memory_file)
    {
        // Get absolute paths of trace_file & memory_file
        let trace_path = trace_file
            .as_path()
            .canonicalize()
            .unwrap_or(trace_file.clone())
            .to_string_lossy()
            .to_string();
        let memory_path = memory_file
            .as_path()
            .canonicalize()
            .unwrap_or(memory_file.clone())
            .to_string_lossy()
            .to_string();

        let json = cairo_runner
            .get_air_private_input(&vm)
            .to_serializable(trace_path, memory_path)
            .serialize_json()
            .map_err(PublicInputError::Serde)?;
        std::fs::write(file_path, json)?;
    }

    if let Some(ref file_name) = args.cairo_pie_output {
        let file_path = Path::new(file_name);
        cairo_runner
            .get_cairo_pie(&vm)
            .map_err(CairoRunError::Runner)?
            .write_zip_file(file_path)?
    }

    Ok(output_buffer)
}

pub fn run_cli(args: impl Iterator<Item = String>) -> Result<(), Error> {
    let args = Args::try_parse_from(args)?;
    let program_input;
    if let Some(ref file) = args.program_input {
        program_input = ProgramInput::from_json(std::fs::read_to_string(file)?.as_str())?;
    } else {
        program_input = ProgramInput::new(HashMap::new());
    }
    let print_output = args.print_output;
    match run(args, program_input) {
        Ok(output) => {
            if print_output {
                print!("{output}");
            }
            Ok(())
        }
        Err(Error::Runner(error)) => {
            eprintln!("{error}");
            Err(Error::Runner(error))
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::too_many_arguments)]
    use super::*;
    use assert_matches::assert_matches;
    use rstest::rstest;

    #[rstest]
    #[case([].as_slice())]
    #[case(["juvix-cairo-vm"].as_slice())]
    fn test_run_missing_mandatory_args(#[case] args: &[&str]) {
        let args = args.iter().cloned().map(String::from);
        assert_matches!(run_cli(args), Err(Error::Cli(_)));
    }

    #[rstest]
    #[case(["juvix-cairo-vm", "--layout", "broken_layout", "../tests/fibonacci.json"].as_slice())]
    fn test_run_invalid_args(#[case] args: &[&str]) {
        let args = args.iter().cloned().map(String::from);
        assert_matches!(run_cli(args), Err(Error::Cli(_)));
    }

    #[rstest]
    #[case(["juvix-cairo-vm", "tests/fibonacci.json", "--air_private_input", "/dev/null", "--proof_mode", "--memory_file", "/dev/null"].as_slice())]
    fn test_run_air_private_input_no_trace(#[case] args: &[&str]) {
        let args = args.iter().cloned().map(String::from);
        assert_matches!(run_cli(args), Err(Error::Cli(_)));
    }

    #[rstest]
    #[case(["juvix-cairo-vm", "tests/fibonacci.json", "--air_private_input", "/dev/null", "--proof_mode", "--trace_file", "/dev/null"].as_slice())]
    fn test_run_air_private_input_no_memory(#[case] args: &[&str]) {
        let args = args.iter().cloned().map(String::from);
        assert_matches!(run_cli(args), Err(Error::Cli(_)));
    }

    #[rstest]
    #[case(["juvix-cairo-vm", "tests/fibonacci.json", "--air_private_input", "/dev/null", "--trace_file", "/dev/null", "--memory_file", "/dev/null"].as_slice())]
    fn test_run_air_private_input_no_proof(#[case] args: &[&str]) {
        let args = args.iter().cloned().map(String::from);
        assert_matches!(run_cli(args), Err(Error::Cli(_)));
    }

    #[rstest]
    fn test_run_ok(
        #[values(None,
                 Some("plain"),
                 Some("small"),
                 Some("dex"),
                 Some("starknet"),
                 Some("starknet_with_keccak"),
                 Some("recursive_large_output"),
                 Some("all_cairo"),
                 Some("all_solidity"),
                 //FIXME: dynamic layout leads to _very_ slow execution
                 //Some("dynamic"),
        )]
        layout: Option<&str>,
        #[values(false, true)] memory_file: bool,
        #[values(false, true)] mut trace_file: bool,
        #[values(false, true)] proof_mode: bool,
        #[values(false, true)] print_output: bool,
        #[values(false, true)] entrypoint: bool,
        #[values(false, true)] air_public_input: bool,
        #[values(false, true)] air_private_input: bool,
        #[values(false, true)] cairo_pie_output: bool,
    ) {
        let mut args = vec!["juvix-cairo-vm".to_string()];
        if let Some(layout) = layout {
            args.extend_from_slice(&["--layout".to_string(), layout.to_string()]);
        }
        if air_public_input {
            args.extend_from_slice(&["--air_public_input".to_string(), "/dev/null".to_string()]);
        }
        if air_private_input {
            args.extend_from_slice(&["--air_private_input".to_string(), "/dev/null".to_string()]);
        }
        if cairo_pie_output {
            args.extend_from_slice(&["--cairo_pie_output".to_string(), "/dev/null".to_string()]);
        }
        if proof_mode {
            trace_file = true;
            args.extend_from_slice(&["--proof_mode".to_string()]);
        }
        if entrypoint {
            args.extend_from_slice(&["--entrypoint".to_string(), "main".to_string()]);
        }
        if memory_file {
            args.extend_from_slice(&["--memory_file".to_string(), "/dev/null".to_string()]);
        }
        if trace_file {
            args.extend_from_slice(&["--trace_file".to_string(), "/dev/null".to_string()]);
        }
        if print_output {
            args.extend_from_slice(&["--print_output".to_string()]);
        }

        args.push("tests/proof_programs/fibonacci.json".to_string());
        if air_public_input && !proof_mode
            || (air_private_input && (!proof_mode || !trace_file || !memory_file))
            || cairo_pie_output && proof_mode
        {
            assert_matches!(run_cli(args.into_iter()), Err(_));
        } else {
            assert_matches!(run_cli(args.into_iter()), Ok(_));
        }
    }

    #[test]
    fn test_run_missing_program() {
        let args = ["juvix-cairo-vm", "missing/program.json"]
            .into_iter()
            .map(String::from);
        assert_matches!(run_cli(args), Err(Error::IO(_)));
    }

    #[rstest]
    #[case("tests/manually_compiled/invalid_even_length_hex.json")]
    #[case("tests/manually_compiled/invalid_memory.json")]
    #[case("tests/manually_compiled/invalid_odd_length_hex.json")]
    #[case("tests/manually_compiled/no_data_program.json")]
    #[case("tests/manually_compiled/no_main_program.json")]
    fn test_run_bad_file(#[case] program: &str) {
        let args = ["juvix-cairo-vm", program].into_iter().map(String::from);
        assert_matches!(run_cli(args), Err(Error::Runner(_)));
    }

    #[test]
    fn test_valid_layouts() {
        let valid_layouts = vec![
            "plain",
            "small",
            "dex",
            "starknet",
            "starknet_with_keccak",
            "recursive_large_output",
            "all_cairo",
            "all_solidity",
        ];

        for layout in valid_layouts {
            assert_eq!(validate_layout(layout), Ok(layout.to_string()));
        }
    }

    #[test]
    fn test_invalid_layout() {
        let invalid_layout = "invalid layout name";
        assert!(validate_layout(invalid_layout).is_err());
    }

    #[rstest]
    #[case("tests/input1.json", "tests/input1_input.json")]
    fn test_input_positive(#[case] program: &str, #[case] input: &str) {
        let args = [
            "juvix-cairo-vm",
            program,
            "--program_input",
            input,
            "--proof_mode",
            "--layout",
            "small",
        ]
        .into_iter()
        .map(String::from);
        assert_matches!(run_cli(args), Ok(()));
    }

    #[rstest]
    #[case("tests/input1.json", "tests/input1_bad_input.json")]
    fn test_input_negative(#[case] program: &str, #[case] input: &str) {
        let args = [
            "juvix-cairo-vm",
            program,
            "--program_input",
            input,
            "--proof_mode",
            "--layout",
            "small",
        ]
        .into_iter()
        .map(String::from);
        assert_matches!(run_cli(args), Err(Error::Runner(_)));
    }

    #[rstest]
    #[case("tests/input2.json", "tests/input2_input.json", "83\n")]
    fn test_input_output_positive(
        #[case] program: &str,
        #[case] input: &str,
        #[case] output: &str,
    ) {
        let args_cli = [
            "juvix-cairo-vm",
            program,
            "--program_input",
            input,
            "--proof_mode",
            "--layout",
            "small",
        ]
        .into_iter()
        .map(String::from);
        let program_input =
            ProgramInput::from_json(std::fs::read_to_string(input).unwrap().as_str()).unwrap();
        let args = Args::try_parse_from(args_cli).unwrap();
        assert_eq!(run(args, program_input).unwrap(), output);
    }

    #[rstest]
    #[case("tests/ec_random.json")]
    fn test_run_positive(#[case] program: &str) {
        let args_cli = [
            "juvix-cairo-vm",
            program,
            "--proof_mode",
            "--layout",
            "small",
        ]
        .into_iter()
        .map(String::from);
        assert_matches!(run_cli(args_cli), Ok(()));
    }
}
