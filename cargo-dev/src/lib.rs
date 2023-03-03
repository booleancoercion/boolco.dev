use clap::ValueEnum;
use const_format::concatcp;
use serde::{Deserialize, Serialize};

use std::fs::{self, OpenOptions};
use std::{fmt::Write as _, io::Write as _, path::Path, process::Command};

const BUILD_ZIP: &str = "build.zip";
const CACHE_DIR: &str = "target/cargo-dev";
const CACHE_FILE: &str = concatcp!(CACHE_DIR, "/cache.toml");
const HOST: &str = "bool@boolco.dev";

trait CommandExecBool {
    fn exec_bool(&mut self) -> bool;
}

impl CommandExecBool for Command {
    fn exec_bool(&mut self) -> bool {
        self.status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum PackMode {
    Static,

    #[clap(alias = "binary")]
    Bin,

    Both,
}

impl PackMode {
    pub fn bin(&self) -> bool {
        match self {
            PackMode::Bin => true,
            PackMode::Static => false,
            PackMode::Both => true,
        }
    }

    pub fn r#static(&self) -> bool {
        match self {
            PackMode::Bin => false,
            PackMode::Static => true,
            PackMode::Both => true,
        }
    }
}

pub fn build() -> bool {
    eprintln!("cargo-dev: building website");
    Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu")
        .env("TARGET_CC", "x86_64-unknown-linux-gnu-gcc")
        .env("TARGET_AR", "x86_64-unknown-linux-gnu-ar")
        .exec_bool()
}

pub fn pack(mode: PackMode) -> bool {
    if !build() {
        return false;
    }
    eprintln!("cargo-dev: packing website");

    if let Err(why) = fs::remove_file(BUILD_ZIP) {
        if why.kind() != std::io::ErrorKind::NotFound {
            eprintln!("cargo-dev: couldn't remove {BUILD_ZIP}: {why}");
            return false;
        }
    }

    if mode.bin() {
        let success = Command::new("zip")
            .arg("-j")
            .arg(BUILD_ZIP)
            .arg("target/x86_64-unknown-linux-gnu/release/website")
            .exec_bool();

        if !success {
            return false;
        }
    }

    if mode.r#static() {
        Command::new("zip")
            .arg("-r")
            .arg(BUILD_ZIP)
            .arg("static")
            .arg("res")
            .exec_bool()
    } else {
        true
    }
}

/// Cached options for the send command.
#[derive(Serialize, Deserialize, Default)]
struct SshOptions {
    key: Option<String>,
    port: Option<u16>,
}

impl SshOptions {
    /// Read the cached options from disk. If any provided option is Some, it will be used instead.
    fn read_from_file<P: AsRef<Path>>(path: P, key: Option<String>, port: Option<u16>) -> Self {
        let mut cache: Self = fs::read(path)
            .ok()
            .and_then(|bytes| toml::from_slice(&bytes).ok())
            .unwrap_or_default();

        if let Some(key) = key {
            cache.key = Some(key);
        }

        if let Some(port) = port {
            cache.port = Some(port);
        }

        cache
    }
}

fn get_ssh_options(key: Option<String>, port: Option<u16>) -> SshOptions {
    let options = SshOptions::read_from_file(CACHE_FILE, key, port);

    // save possibly modified options to disk
    let _ = fs::create_dir(CACHE_DIR);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(CACHE_FILE)
    {
        let _ = writeln!(&mut file, "{}", toml::to_string_pretty(&options).unwrap());
    }

    options
}

fn upload_zip(options: &SshOptions) -> bool {
    eprintln!("cargo-dev: uploading package");
    let mut command = Command::new("scp");
    if let Some(key) = &options.key {
        command.arg("-i").arg(key);
    }
    if let Some(port) = &options.port {
        command.arg("-P").arg(port.to_string());
    }

    command
        .arg(BUILD_ZIP)
        .arg(concatcp!(HOST, ":boolco.dev/", BUILD_ZIP))
        .exec_bool()
}

fn default_ssh(options: &SshOptions) -> Command {
    let mut command = Command::new("ssh");
    if let Some(key) = &options.key {
        command.arg("-i").arg(key);
    }
    if let Some(port) = &options.port {
        command.arg("-P").arg(port.to_string());
    }

    command.arg(HOST);

    command
}

pub fn send(key: Option<String>, mode: PackMode, deploy: bool, port: Option<u16>) -> bool {
    if !pack(mode) {
        return false;
    }

    eprintln!("cargo-dev: sending package");

    let options = get_ssh_options(key, port);

    // upload the zip file
    if !upload_zip(&options) {
        return false;
    }

    // process zip file on server
    let mut command = default_ssh(&options);

    let mut buffer = String::from("cd boolco.dev\n"); // stores the command that will be run on the server
    if deploy {
        writeln!(&mut buffer, "./stop.sh").unwrap();
    }
    if mode.r#static() {
        // no need to do the same for the binary as it will be overwritten
        writeln!(&mut buffer, "rm -rf static").unwrap();
        writeln!(&mut buffer, "rm -rf res").unwrap();
    }
    writeln!(&mut buffer, "unzip -o {BUILD_ZIP}").unwrap();
    if deploy {
        writeln!(&mut buffer, "./start.sh").unwrap();
    }

    command.arg(buffer).exec_bool()
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum PowerOptions {
    Start,
    Stop,
    Both,
}

impl PowerOptions {
    pub fn start(&self) -> bool {
        match self {
            PowerOptions::Start => true,
            PowerOptions::Stop => false,
            PowerOptions::Both => true,
        }
    }

    pub fn stop(&self) -> bool {
        match self {
            PowerOptions::Start => false,
            PowerOptions::Stop => true,
            PowerOptions::Both => true,
        }
    }
}

pub fn power(key: Option<String>, port: Option<u16>, power_options: PowerOptions) -> bool {
    let options = get_ssh_options(key, port);
    let mut command = default_ssh(&options);

    let mut buffer = String::from("cd boolco.dev\n");
    if power_options.stop() {
        writeln!(&mut buffer, "./stop.sh").unwrap();
    }
    if power_options.start() {
        writeln!(&mut buffer, "./start.sh").unwrap();
    }

    command.arg(buffer).exec_bool()
}
