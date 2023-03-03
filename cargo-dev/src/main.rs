use cargo_dev::{PackMode, PowerOptions};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    subcommand: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Builds the package for the target that the website runs on
    Build,

    /// Builds the release version, then packs everything into a zip file
    Pack {
        /// The mode to use when packing the files
        #[clap(short, long, value_enum, default_value = "both")]
        mode: PackMode,
    },

    /// Sends files to the server using the provided key file path and the chosen mode
    Send {
        /// Optional path to the used key file. This option is cached and will be used for the next time
        #[clap(short = 'i', long)]
        key: Option<String>,

        /// Same as in `cargo dev pack`
        #[clap(short, long, value_enum, default_value = "both")]
        mode: PackMode,

        /// Automatically stop the remote server before sending, and start it again after sending
        #[clap(short, long, action = clap::ArgAction::Set)]
        deploy: bool,

        /// The port number to use. This option is cached and will be used for the next time.
        /// If not specified and no port exists in cache, the default port will be used (22, unless your ssh config differs).
        #[clap(short, long)]
        port: Option<u16>,
    },

    /// Starts the remote server
    Start {
        /// Same as in `cargo dev send`
        #[clap(short = 'i', long)]
        key: Option<String>,

        /// Same as in `cargo dev send`
        #[clap(short, long)]
        port: Option<u16>,
    },

    /// Stops the remote server
    Stop {
        /// Same as in `cargo dev send`
        #[clap(short = 'i', long)]
        key: Option<String>,

        /// Same as in `cargo dev send`
        #[clap(short, long)]
        port: Option<u16>,
    },

    /// Restarts the remote server
    Restart {
        /// Same as `send`
        #[clap(short = 'i', long)]
        key: Option<String>,

        /// Same as `send`
        #[clap(short, long)]
        port: Option<u16>,
    },
}

fn main() {
    let Args {
        subcommand: command,
    } = Args::parse();

    let success = match command {
        Command::Build => cargo_dev::build(),
        Command::Pack { mode } => cargo_dev::pack(mode),
        Command::Send {
            key,
            mode,
            deploy,
            port,
        } => cargo_dev::send(key, mode, deploy, port),
        Command::Start { key, port } => cargo_dev::power(key, port, PowerOptions::Start),
        Command::Stop { key, port } => cargo_dev::power(key, port, PowerOptions::Stop),
        Command::Restart { key, port } => cargo_dev::power(key, port, PowerOptions::Both),
    };

    if !success {
        eprintln!("cargo-dev: couldn't execute command due to above errors");
        std::process::exit(1);
    }
}
