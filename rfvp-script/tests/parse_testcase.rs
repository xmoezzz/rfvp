use std::fs;

use anyhow::Result;

use rfvp_nls::{Decoder, Encoding};
use rfvp_script::{parse_hcb, ControlFlow, SyscallHost, SyscallResult, Variant, Vm, VmContext};

#[derive(Default)]
struct YieldOnSyscallHost;

impl SyscallHost for YieldOnSyscallHost {
    type Handle = u32;

    fn resolve(&mut self, _name: &[u8], _arg_count: u8) -> Result<Self::Handle> {
        Ok(0)
    }

    fn call(
        &mut self,
        _handle: Self::Handle,
        _args: &[Variant],
        _ctx: &mut VmContext<'_>,
    ) -> Result<SyscallResult> {
        Ok(SyscallResult {
            ret: Variant::NIL,
            control: ControlFlow::Yield,
        })
    }
}

#[test]
fn parse_and_step_until_first_syscall() -> Result<()> {
    let path = "testcase/test.hcb";
    let Ok(bytes) = fs::read(path) else {
        // Skip in CI if testcase isn't present.
        println!("skipping missing testcase: {}", path);
        return Ok(());
    };

    let file = parse_hcb(&bytes)?;
    let d = Decoder::new(Encoding::ShiftJis);
    println!("parsed header: {:?}", file.title_str(&d));
    let mut host = YieldOnSyscallHost::default();
    let mut vm = Vm::new(file, &mut host)?;

    let out = vm.run_for(&mut host, 50_000)?;
    assert!(
        matches!(
            out.outcome,
            rfvp_script::StepOutcome::Yield
                | rfvp_script::StepOutcome::Halt
                | rfvp_script::StepOutcome::Continue
        ),
        "unexpected outcome: {:?}",
        out.outcome
    );
    Ok(())
}
