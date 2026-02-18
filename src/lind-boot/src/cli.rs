use clap::*;

#[derive(Debug, Parser, Clone)]
#[command(name = "lind-boot")]
pub struct CliOptions {
    /// todo: Increase logging verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Enable debug mode
    #[arg(long)]
    pub debug: bool,

    /// Allow executing precompiled .cwasm artifacts
    #[arg(long)]
    pub allow_precompile: bool,

    /// AOT-compile a .wasm file to a .cwasm artifact and exit (no runtime needed)
    #[arg(long)]
    pub precompile: bool,

    /// First item is WASM file (argv[0]), rest are program args (argv[1..])
    ///
    /// Example:
    ///   lind-wasm prog.wasm a b c
    #[arg(value_name = "WASM_FILE", required = true, num_args = 1.., trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Pass an environment variable to the program.
    ///
    /// The `--env FOO=BAR` form will set the environment variable named `FOO`
    /// to the value `BAR` for the guest program using WASI. The `--env FOO`
    /// form will set the environment variable named `FOO` to the same value it
    /// has in the calling process for the guest, or in other words it will
    /// cause the environment variable `FOO` to be inherited.
    #[arg(long = "env", number_of_values = 1, value_name = "NAME[=VAL]", value_parser = parse_env_var)]
    pub vars: Vec<(String, Option<String>)>,
}

pub fn parse_env_var(s: &str) -> Result<(String, Option<String>), String> {
    let mut parts = s.splitn(2, '=');
    Ok((
        parts.next().unwrap().to_string(),
        parts.next().map(|s| s.to_string()),
    ))
}

impl CliOptions {
    pub fn wasm_file(&self) -> &str {
        &self.args[0]
    }
}
