use ark_ff::fields::{Fp256, MontBackend, MontConfig};
use ark_ff::{Field, PrimeField};
use ark_std::UniformRand;
use cairo_vm::any_box;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use cairo_vm::vm::runners::cairo_runner::RunResources;
use cairo_vm::Felt252;
use cairo_vm::{
    hint_processor::hint_processor_definition::HintProcessorLogic,
    types::exec_scope::ExecutionScopes,
    vm::errors::vm_errors::VirtualMachineError,
    vm::{errors::hint_errors::HintError, vm_core::VirtualMachine},
};
use num_bigint::BigUint;
use std::any::Any;
use std::collections::HashMap;

use super::hint::Hint;
use crate::program_input::ProgramInput;

#[derive(MontConfig)]
#[modulus = "3618502788666131213697322783095070105623107215331596699973092056135872020481"]
#[generator = "3"]

/// Returns the Beta value of the Starkware elliptic curve.
struct FqConfig;
type Fq = Fp256<MontBackend<FqConfig, 4>>;

fn get_beta() -> Felt252 {
    Felt252::from_dec_str(
        "3141592653589793238462643383279502884197169399375105820974944592307816406665",
    )
    .unwrap()
}

/// Execution scope for constant memory allocation.
struct MemoryExecScope {
    /// The first free address in the segment.
    next_address: Relocatable,
}

pub struct JuvixHintProcessor {
    program_input: ProgramInput,
    run_resources: RunResources,
}

impl JuvixHintProcessor {
    pub fn new(program_input: ProgramInput) -> Self {
        Self {
            program_input,
            run_resources: RunResources::default(),
        }
    }
    // Runs a single Hint
    pub fn execute(
        &self,
        vm: &mut VirtualMachine,
        exec_scopes: &mut ExecutionScopes,
        hint: &Hint,
    ) -> Result<(), HintError> {
        match hint {
            Hint::Alloc(size) => self.alloc_constant_size(vm, exec_scopes, *size),

            Hint::Input(var) => self.read_program_input(vm, var),

            Hint::RandomEcPoint => self.random_ec_point(vm),
        }
    }

    fn alloc_constant_size(
        &self,
        vm: &mut VirtualMachine,
        exec_scopes: &mut ExecutionScopes,
        size: usize,
    ) -> Result<(), HintError> {
        let memory_exec_scope =
            match exec_scopes.get_mut_ref::<MemoryExecScope>("memory_exec_scope") {
                Ok(memory_exec_scope) => memory_exec_scope,
                Err(_) => {
                    exec_scopes.assign_or_update_variable(
                        "memory_exec_scope",
                        Box::new(MemoryExecScope {
                            next_address: vm.add_memory_segment(),
                        }),
                    );
                    exec_scopes.get_mut_ref::<MemoryExecScope>("memory_exec_scope")?
                }
            };

        vm.insert_value(vm.get_ap(), memory_exec_scope.next_address)?;
        memory_exec_scope.next_address.offset += size;

        Ok(())
    }

    fn read_program_input(&self, vm: &mut VirtualMachine, var: &String) -> Result<(), HintError> {
        vm.insert_value(vm.get_ap(), self.program_input.get(var.as_str()))
            .map_err(HintError::Memory)
    }

    fn random_ec_point(&self, vm: &mut VirtualMachine) -> Result<(), HintError> {
        let beta = Fq::from(get_beta().to_biguint());

        let mut rng = ark_std::test_rng();
        let (random_x, random_y_squared) = loop {
            let random_x = Fq::rand(&mut rng);
            let random_y_squared = random_x * random_x * random_x + random_x + beta;
            if random_y_squared.legendre().is_qr() {
                break (random_x, random_y_squared);
            }
        };

        let x_bigint: BigUint = random_x.into_bigint().into();
        let y_bigint: BigUint = random_y_squared
            .sqrt()
            .ok_or_else(|| {
                HintError::CustomHint("Failed to compute sqrt".to_string().into_boxed_str())
            })?
            .into_bigint()
            .into();

        let ap = vm.get_ap();
        vm.insert_value(ap, Felt252::from(&x_bigint))?;
        vm.insert_value((ap + 1)?, Felt252::from(&y_bigint))?;

        Ok(())
    }
}

impl HintProcessorLogic for JuvixHintProcessor {
    fn compile_hint(
        &self,
        //Block of hint code as String
        hint_code: &str,
        //Ap Tracking Data corresponding to the Hint
        _ap_tracking_data: &cairo_vm::serde::deserialize_program::ApTracking,
        //Map from variable name to reference id number
        //(may contain other variables aside from those used by the hint)
        _reference_ids: &HashMap<String, usize>,
        //List of all references (key corresponds to element of the previous dictionary)
        _references: &[HintReference],
    ) -> Result<Box<dyn Any>, VirtualMachineError> {
        let data = hint_code
            .parse::<Hint>()
            .map_err(|e| VirtualMachineError::CompileHintFail(e.message.into_boxed_str()))?;
        Ok(any_box!(data))
    }

    fn execute_hint(
        &mut self,
        //Proxy to VM, contains refrences to necessary data
        //+ MemoryProxy, which provides the necessary methods to manipulate memory
        vm: &mut VirtualMachine,
        //Proxy to ExecutionScopes, provides the necessary methods to manipulate the scopes and
        //access current scope variables
        exec_scopes: &mut ExecutionScopes,
        //Data structure that can be downcasted to the structure generated by compile_hint
        hint_data: &Box<dyn Any>,
        //Constant values extracted from the program specification.
        _constants: &HashMap<String, Felt252>,
    ) -> Result<(), HintError> {
        let hint: &Hint = hint_data.downcast_ref().ok_or(HintError::WrongHintData)?;
        self.execute(vm, exec_scopes, hint)
    }
}

impl ResourceTracker for JuvixHintProcessor {
    fn consumed(&self) -> bool {
        self.run_resources.consumed()
    }

    fn consume_step(&mut self) {
        self.run_resources.consume_step()
    }

    fn get_n_steps(&self) -> Option<usize> {
        self.run_resources.get_n_steps()
    }

    fn run_resources(&self) -> &RunResources {
        &self.run_resources
    }
}
