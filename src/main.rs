use juvix_cairo_vm::*;

fn main() -> Result<(), Error> {
    match run_cli(std::env::args()) {
        Err(Error::Cli(err)) => err.exit(),
        other => other,
    }
}
