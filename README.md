![CI](https://github.com/anoma/juvix-cairo-vm/actions/workflows/rust.yml/badge.svg?event=push)

A custom CLI wrapper for the [Cairo VM](https://github.com/lambdaclass/cairo-vm) with [Juvix](https://github.com/anoma/juvix) support. The code is based on [`cairo-vm-cli`](https://github.com/lambdaclass/cairo-vm/tree/main/cairo-vm-cli).

The easiest way to provide the right options to run Juvix programs compiled to Cairo VM bytecode is to use this script:
[run_cairo_vm.sh](https://github.com/anoma/juvix/blob/main/scripts/run_cairo_vm.sh)

Once you have both `juvix-cairo-vm` and `run_cairo_vm.sh` on path:
- compile `Program.juvix` to `Program.json` with `juvix compile cairo Program.juvix`,
- run with `run_cairo_vm.sh Program.json` or `run_cairo_vm.sh Program.json --program_input input.json`
