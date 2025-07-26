use std::arch::asm;

fn main() {
    let mut reg: [u32; 4] = [0; 4];
    
    // Call CPUID instruction with EAX=0 to get vendor string
    unsafe {
        asm!(
            "cpuid",
            in("eax") 0u32,
            out("eax") reg[3],
            out("ebx") reg[0],
            out("ecx") reg[2],
            out("edx") reg[1],
            options(nomem, nostack)
        );
    }
    
    // Convert the registers to a string (vendor ID)
    let vendor_string = unsafe {
        std::str::from_utf8_unchecked(&[
            (reg[0] & 0xff) as u8,
            ((reg[0] >> 8) & 0xff) as u8,
            ((reg[0] >> 16) & 0xff) as u8,
            ((reg[0] >> 24) & 0xff) as u8,
            (reg[1] & 0xff) as u8,
            ((reg[1] >> 8) & 0xff) as u8,
            ((reg[1] >> 16) & 0xff) as u8,
            ((reg[1] >> 24) & 0xff) as u8,
            (reg[2] & 0xff) as u8,
            ((reg[2] >> 8) & 0xff) as u8,
            ((reg[2] >> 16) & 0xff) as u8,
            ((reg[2] >> 24) & 0xff) as u8,
        ])
    };
    
    println!("{}", vendor_string);
} 