// ematix-probe CLI.
//
// Phase 0 ships only the binary skeleton with `--version`. Subcommands
// (`run`, `list`, `explain`, `doctor`) are added in subsequent phases —
// see PRD §7 and PI plan.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "ematix-probe",
    about = "Declarative testing automation: data probes + load probes.",
    version = ematix_probe_core::VERSION,
)]
struct Cli {}

fn main() {
    let _ = Cli::parse();
}
