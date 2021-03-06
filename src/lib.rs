mod flushing_writer;
mod iterator_ext;
mod messages;
mod options;
mod process;

use anyhow::{Context, Result};
use cargo_metadata::Message;
use flushing_writer::FlushingWriter;
use messages::{process_messages, ParsedMessages};
use options::Options;
use std::{
    env,
    io::{self, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
};

const CARGO_EXECUTABLE: &str = "cargo";
const CARGO_ENV_VAR: &str = "CARGO";
const NO_EXIT_CODE: i32 = 127;

const ADDITIONAL_ENVIRONMENT_VARIABLES: &str =
    "Additional environment variables:\n    CARGO_MSG_LIMIT     Limit compiler messages number (0 \
     means no limit, which is default)\n    CARGO_TIME_LIMIT    Execution time limit in seconds \
     after encountering first compiling error (0 means no limit, 1 is default)\n    \
     CARGO_ASC           Show compiler messages in \
     ascending order (false is default)\n    CARGO_FORCE_WARN    Show warnings even if errors \
     still exist (false is default)\n    CARGO_DEPS_WARN     Show dependencies' warnings (false is default)";

#[doc(hidden)]
pub fn run_cargo_filtered(cargo_command: &str) -> Result<i32> {
    let parsed_args = Options::from_args_and_vars(cargo_command)?;
    let cargo_path = env::var(CARGO_ENV_VAR)
        .map(PathBuf::from)
        .ok()
        .unwrap_or_else(|| PathBuf::from(CARGO_EXECUTABLE));
    let mut command = Command::new(cargo_path)
        .args(parsed_args.cargo_args.clone())
        .stdout(Stdio::piped())
        .spawn()?;

    let cargo_pid = command.id();
    ctrlc::set_handler(move || {
        process::kill(cargo_pid);
    })?;

    let mut stdout_reader = BufReader::new(command.stdout.take().context("cannot read stdout")?);
    let mut stdout_writer = FlushingWriter::new(io::stdout());

    let help = parsed_args.help;

    if !help {
        let parsed_messages = ParsedMessages::parse(&mut stdout_reader, cargo_pid, &parsed_args)?;
        let processed_messages = process_messages(parsed_messages, &parsed_args)?;
        if parsed_args.json_message_format {
            for message in processed_messages {
                println!("{}", serde_json::to_string(&message)?);
            }
        } else {
            for message in processed_messages.filter_map(|message| match message {
                Message::CompilerMessage(compiler_message) => compiler_message.message.rendered,
                _ => None,
            }) {
                print!("{}", message);
            }
        }
    }

    io::copy(&mut stdout_reader, &mut stdout_writer)?;

    if help {
        println!("\n{}", ADDITIONAL_ENVIRONMENT_VARIABLES);
    }

    let exit_code = command.wait()?.code().unwrap_or(NO_EXIT_CODE);
    Ok(exit_code)
}

#[doc(hidden)]
#[macro_export]
macro_rules! run_command {
    ($command:expr) => {
        fn main() -> anyhow::Result<()> {
            std::process::exit(cargo_limit::run_cargo_filtered($command)?);
        }
    };
}
