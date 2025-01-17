//! The exception module contains all the exception kinds and the function to handle exceptions.

use crate::{
    cpu::{Cpu, Mode},
    csr::*,
};

/// All the exception kinds.
#[derive(Debug)]
pub enum Exception {
    /// With the addition of the C extension, no instructions can raise
    /// instruction-address-misaligned exceptions.
    InstructionAddressMisaligned,
    InstructionAccessFault,
    IllegalInstruction,
    Breakpoint,
    LoadAddressMisaligned,
    LoadAccessFault,
    StoreAMOAddressMisaligned,
    StoreAMOAccessFault,
    EnvironmentCallFromUMode,
    EnvironmentCallFromSMode,
    EnvironmentCallFromMMode,
    InstructionPageFault,
    LoadPageFault,
    StoreAMOPageFault,
}

/// All the trap kinds.
#[derive(Debug)]
pub enum Trap {
    /// The trap is visible to, and handled by, software running inside the execution
    /// environment.
    Contained,
    /// The trap is a synchronous exception that is an explicit call to the execution
    /// environment requesting an action on behalf of software inside the execution environment.
    Requested,
    /// The trap is handled transparently by the execution environment and execution
    /// resumes normally after the trap is handled.
    Invisible,
    /// The trap represents a fatal failure and causes the execution environment to terminate
    /// execution.
    Fatal,
}

impl Exception {
    fn exception_code(&self) -> u64 {
        match self {
            Exception::InstructionAddressMisaligned => 0,
            Exception::InstructionAccessFault => 1,
            Exception::IllegalInstruction => 2,
            Exception::Breakpoint => 3,
            Exception::LoadAddressMisaligned => 4,
            Exception::LoadAccessFault => 5,
            Exception::StoreAMOAddressMisaligned => 6,
            Exception::StoreAMOAccessFault => 7,
            Exception::EnvironmentCallFromUMode => 8,
            Exception::EnvironmentCallFromSMode => 9,
            Exception::EnvironmentCallFromMMode => 11,
            Exception::InstructionPageFault => 12,
            Exception::LoadPageFault => 13,
            Exception::StoreAMOPageFault => 15,
        }
    }

    /// Update CSRs and the program counter depending on an exception.
    pub fn take_trap(&self, cpu: &mut Cpu) -> Trap {
        // 1.2 Privilege Levels
        // "Traps that increase privilege level are termed vertical traps, while traps that remain
        // at the same privilege level are termed horizontal traps."

        let exception_pc = cpu.pc.wrapping_sub(4);
        cpu.prev_mode = cpu.mode;

        let cause = self.exception_code();

        // 3.1.8 Machine Trap Delegation Registers (medeleg and mideleg)
        // "By default, all traps at any privilege level are handled in machine mode"
        // "To increase performance, implementations can provide individual read/write bits within
        // medeleg and mideleg to indicate that certain exceptions and interrupts should be
        // processed directly by a lower privilege level."
        //
        // "medeleg has a bit position allocated for every synchronous exception shown in Table 3.6
        // on page 37, with the index of the bit position equal to the value returned in the mcause
        // register (i.e., setting bit 8 allows user-mode environment calls to be delegated to a
        // lower-privilege trap handler)."
        if cpu.mode <= Mode::Supervisor && (((cpu.state.read(MEDELEG) & 0xffff) >> cause) & 1) == 1
        {
            // Handle the trap in S-mode.
            cpu.mode = Mode::Supervisor;

            // Set the program counter to the supervisor trap-handler base address (stvec).
            cpu.pc = (cpu.state.read(STVEC) & !1) as u64;

            // 4.1.9 Supervisor Exception Program Counter (sepc)
            // "The low bit of sepc (sepc[0]) is always zero."
            // "When a trap is taken into S-mode, sepc is written with the virtual address of
            // the instruction that was interrupted or that encountered the exception.
            // Otherwise, sepc is never written by the implementation, though it may be
            // explicitly written by software."
            cpu.state.write(SEPC, exception_pc & !1);

            // 4.1.10 Supervisor Cause Register (scause)
            // "When a trap is taken into S-mode, scause is written with a code indicating
            // the event that caused the trap.  Otherwise, scause is never written by the
            // implementation, though it may be explicitly written by software."
            cpu.state.write(SCAUSE, cause);

            // 4.1.11 Supervisor Trap Value (stval) Register
            // "When a trap is taken into S-mode, stval is written with exception-specific
            // information to assist software in handling the trap. Otherwise, stval is never
            // written by the implementation, though it may be explicitly written by software."
            // "When a hardware breakpoint is triggered, or an instruction-fetch, load, or
            // store address-misaligned, access, or page-fault exception occurs, stval is
            // written with the faulting virtual address. On an illegal instruction trap,
            // stval may be written with the first XLEN or ILEN bits of the faulting
            // instruction as described below. For other exceptions, stval is set to zero."
            cpu.state.write(STVAL, 0);

            // Set a previous interrupt-enable bit for supervisor mode (SPIE, 5) to the value
            // of a global interrupt-enable bit for supervisor mode (SIE, 1).
            cpu.state
                .write_bit(SSTATUS, 5, cpu.state.read_bit(SSTATUS, 1));
            // Set a global interrupt-enable bit for supervisor mode (SIE, 1) to 0.
            cpu.state.write_bit(SSTATUS, 1, 0);
            // 4.1.1 Supervisor Status Register (sstatus)
            // "When a trap is taken, SPP is set to 0 if the trap originated from user mode, or
            // 1 otherwise."
            match cpu.prev_mode {
                Mode::User => cpu.state.write_bit(SSTATUS, 8, 0),
                _ => cpu.state.write_bit(SSTATUS, 8, 1),
            }
        } else {
            // Handle the trap in M-mode.
            cpu.mode = Mode::Machine;

            // Set the program counter to the machine trap-handler base address (mtvec).
            cpu.pc = (cpu.state.read(MTVEC) & !1) as u64;

            // 3.1.15 Machine Exception Program Counter (mepc)
            // "The low bit of mepc (mepc[0]) is always zero."
            // "When a trap is taken into M-mode, mepc is written with the virtual address of
            // the instruction that was interrupted or that encountered the exception.
            // Otherwise, mepc is never written by the implementation, though it may be
            // explicitly written by software."
            cpu.state.write(MEPC, exception_pc & !1);

            // 3.1.16 Machine Cause Register (mcause)
            // "When a trap is taken into M-mode, mcause is written with a code indicating
            // the event that caused the trap. Otherwise, mcause is never written by the
            // implementation, though it may be explicitly written by software."
            cpu.state.write(MCAUSE, cause);

            // 3.1.17 Machine Trap Value (mtval) Register
            // "When a trap is taken into M-mode, mtval is either set to zero or written with
            // exception-specific information to assist software in handling the trap.
            // Otherwise, mtval is never written by the implementation, though it may be
            // explicitly written by software."
            // "When a hardware breakpoint is triggered, or an instruction-fetch, load, or
            // store address-misaligned, access, or page-fault exception occurs, mtval is
            // written with the faulting virtual address. On an illegal instruction trap,
            // mtval may be written with the first XLEN or ILEN bits of the faulting
            // instruction as described below. For other traps, mtval is set to zero."
            cpu.state.write(MTVAL, 0);

            // Set a previous interrupt-enable bit for supervisor mode (MPIE, 7) to the value
            // of a global interrupt-enable bit for supervisor mode (MIE, 3).
            cpu.state
                .write_bit(MSTATUS, 7, cpu.state.read_bit(MSTATUS, 3));
            // Set a global interrupt-enable bit for supervisor mode (MIE, 3) to 0.
            cpu.state.write_bit(MSTATUS, 3, 0);
            // When a trap is taken from privilege mode y into privilege mode x, xPIE is set
            // to the value of x IE; x IE is set to 0; and xPP is set to y.
            match cpu.prev_mode {
                Mode::User => cpu.state.write_bits(MSTATUS, 11..13, 0b00),
                Mode::Supervisor => cpu.state.write_bits(MSTATUS, 11..13, 0b01),
                Mode::Machine => cpu.state.write_bits(MSTATUS, 11..13, 0b11),
                _ => panic!("previous privilege mode is invalid"),
            }
        }

        match self {
            Exception::InstructionAddressMisaligned | Exception::InstructionAccessFault => {
                Trap::Fatal
            }
            Exception::IllegalInstruction => Trap::Invisible,
            Exception::Breakpoint => Trap::Requested,
            Exception::LoadAddressMisaligned
            | Exception::LoadAccessFault
            | Exception::StoreAMOAddressMisaligned
            | Exception::StoreAMOAccessFault => Trap::Fatal,
            Exception::EnvironmentCallFromUMode
            | Exception::EnvironmentCallFromSMode
            | Exception::EnvironmentCallFromMMode => Trap::Requested,
            Exception::InstructionPageFault
            | Exception::LoadPageFault
            | Exception::StoreAMOPageFault => Trap::Invisible,
        }
    }
}
