use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::{mem, io,};
use crate::interface;
use crate::safeposix::vmmap::*;
use crate::constants::{PROT_READ, PROT_WRITE, MAP_PRIVATE};
use crate::threeiconstant;
use crate::syscall_table;

// Direct jump to the address. Transfer address to function pointer
// by using transmute inside unsafe block
// Ref: https://doc.rust-lang.org/std/mem/fn.transmute.html
// Function pointer type, used to save the address of the target function
pub type CallFunc = fn(
    target_cageid:u64,
    arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64,
) -> u64;

// Despite the fact that many calls use a callfunc, cage tuple, once I've 
// checked the cage, I can just convert the callfunc and ignore these u64s
#[derive(Debug, Clone)]
pub struct CageCallTable {
    defaultcallfunc: Option<HashMap<u64, CallFunc>>,             
    thiscalltable: HashMap<u64, CallFunc>,   // Callnum to jump address mapping
}

impl CageCallTable {
    pub fn new() -> Self {
        Self {
            defaultcallfunc: None,
            thiscalltable: HashMap::new(),
        }
    }

    pub fn register_handler(&mut self, callnum: u64, handler: CallFunc) {
        self.thiscalltable.insert(callnum, handler);
    }

    // This function will only be called when MATCHALL flag has been set in register_handler function
    // to initialize default 
    pub fn set_default_handler(&mut self, targetcage: u64) -> Result<(), Box<dyn std::error::Error>> {
        for (syscall_num, syscall_name) in syscall_table {
            unsafe {
                // Get function address
                let func: CallFunc = unsafe { mem::transmute(syscall_name as *const()); };
                // Insert 
                default_mapping.insert(syscall_num, *func);
            }
        }
        self.defaultcallfunc = Some(default_mapping);
        return Ok(());
    }
}

// Keys are the cage, the value is a HashMap with a key of the callnum
// and the values are a (addr, cage) tuple for the actual handlers...
// Added mutex to avoid race condition
lazy_static::lazy_static! {
    #[derive(Debug)]
    // <self_cageid, <callnum, (addr, dest_cageid)>
    pub static ref HANDLERTABLE: Mutex<HashMap<u64, Arc<Mutex<CageCallTable>>>> = Mutex::new(HashMap::new());
}

/************************ Dependency relationship between cage and grate ************************/
// Use a HashMap to store dependencies, where each cage ID maps to the list of grates it depends on.
// Use HashSet for better performance
lazy_static::lazy_static! {
    #[derive(Debug)]
    pub static ref DEPENDENCY_TABLE: Mutex<HashMap<u64, HashSet<u64>>> = Mutex::new(HashMap::new());
}

pub fn add_dependency(cage: u64, dependent_grate: u64) {
    let mut dependencies = DEPENDENCY_TABLE.lock().unwrap();
    dependencies.entry(cage).or_insert_with(HashSet::new).insert(dependent_grate);
}

pub fn rm_one_dependency(cage: u64, dependent_cage: u64) {
    let mut dependency_table = DEPENDENCY_TABLE.lock().unwrap();
    if let Some(dependents) = dependency_table.get_mut(&cage) {
        dependents.remove(&dependent_cage);
        if dependents.is_empty() {
            dependency_table.remove(&cage);
        }
    }
}

/***************************** register_handler *****************************/
pub fn register_handler(
    _callnum: u64,                  // Unused, kept for syscall convention
    targetcage: u64,                // Cage to modify
    targetcallnum: u64,             // Syscall number or match-all indicator
    _arg1cage: u64,                 // Unused 
    handlefunc: Option<CallFunc>,   // Function to register or None for deregister
    handlefunccage: u64,            // Deregister flag or additional information
    _arg3: u64,                     // Unused 
    _arg3cage: u64,                 // Unused 
    _arg4: u64,                     // Unused 
    _arg4cage: u64,                 // Unused 
    _arg5: u64,                     // Unused 
    _arg5cage: u64,                 // Unused 
    _arg6: u64,                     // Unused 
    _arg6cage: u64,                 // Unused 
) -> u64 {
    let mut handler_table = HANDLERTABLE.lock().unwrap();

    if let Some(cage_table) = handler_table.get_mut(&targetcage) {
        let mut cage_table = cage_table.lock().unwrap();

        if handlefunccage == threeiconstant::THREEI_DEREGISTER {
            // Deregister handlers
            if targetcallnum == threeiconstant::THREEI_MATCHALL {
                // Remove all handlers
                cage_table.thiscalltable.clear();
            } else {
                // Remove specific handler
                cage_table.thiscalltable.remove(&targetcallnum);
            }
        } else if let Some(handler) = handlefunc {
            // Register new handler
            if targetcallnum == threeiconstant::THREEI_MATCHALL {
                // Set as default handler
                cage_table.set_default_handler(targetcage);
            } else {
                // Set specific handler
                cage_table.register_handler(targetcallnum, handler);
            }
        } else {
            eprintln!("Invalid operation: Neither handler nor deregister flag provided.");
            return threeiconstant::ELINDAPIABORTED; // Error: Invalid input
        }
        0 
    } else {
        eprintln!("Target cage {:?} not found.", targetcage);
        return threeiconstant::ELINDAPIABORTED; // Error: Cage not found
    }
}

/***************************** trigger_harsh_cage_exit & harsh_cage_exit *****************************/
// Starts an unclean exit process for the target cage. Notifies threei and related grates to quickly block 
// new calls and clean up resources. This function cannot be called directly by user mode to ensure that it 
// is only triggered by the system kernel or trusted modules
pub fn trigger_harsh_cage_exit(targetcage:u64, exittype:u64) {
    let handler_table = HANDLERTABLE.lock().unwrap();

    if let Some(_cage_table) = handler_table.get(&targetcage) {
        // Perform clean by calling `harsh_cage_exit` 
        // Set callnum to special value 0
        harsh_cage_exit(
            0, 
            targetcage, 
            exittype, 
            0, 
            0, 
            0, 
            0, 
            0, 
            0, 
            0, 
            0, 
            0, 
            0, 
            0);
    } else {
        panic!("Cage {:?} does not exist.", targetcage);
        // return threeiconstant::ELINDAPIABORTED;
    }
}

// Perform resource cleanup, including removing the cage's information from the HANDLERTABLE. Mark the exited 
// cage as unavailable and return an error value to outstanding calls. Interception is supported, but references 
// to the exited cage's memory must be avoided.
pub fn harsh_cage_exit(
    callnum:u64,    // System call number (can be used if called as syscall)
    targetcage:u64, // Cage to cleanup
    exittype:u64,   // Exit type (e.g., fault, manual exit)
    _arg1cage:u64,
    _arg2:u64, 
    _arg2cage:u64,
    _arg3:u64, 
    _arg3cage:u64,
    _arg4:u64, 
    _arg4cage:u64,
    _arg5:u64, 
    _arg5cage:u64,
    _arg6:u64, 
    _arg6cage:u64, 
) -> u64 {
    let mut handler_table = HANDLERTABLE.lock().unwrap();
    let mut dependency_table = DEPENDENCY_TABLE.lock().unwrap();

    // Cleanup current grate/cage syscall table
    if handler_table.remove(&targetcage).is_some() {
        // Cleanup dependents grates/cages
        if let Some(dependent_grates) = dependency_table.remove(&targetcage) {
            for grate in dependent_grates {
                harsh_cage_exit(0, grate, exittype, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
            }
        } 

        if callnum == syscall_table::EXIT_SYSCALL {
            let result = make_syscall(
                callnum, 
                targetcage, 
                exittype, 
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

            if result != 0 {
                eprintln!(
                    "Failed to excute syscall for cage {:?} with call num {:?} - Result: {:?}",
                    targetcage, callnum, result
                );
                return threeiconstant::ELINDAPIABORTED;
            }
        } else {
            eprintln!(
                "Invalid callnum: {:?}",
                callnum
            );
            return threeiconstant::ELINDAPIABORTED;
        }

        0 // Success
    } else {
        eprintln!(
            "harsh_cage_exit called for non-existent cage: {}. Ignoring.",
            targetcage
        );
        return threeiconstant::ELINDAPIABORTED; // Error: Cage not found
    }
}

/***************************** copy_handler_table_to_cage *****************************/
pub fn copy_handler_table_to_cage(
    _callnum:u64, 
    targetcage:u64, 
    srccage:u64, 
    _arg1cage:u64,
    _arg2:u64, 
    _arg2cage:u64,
    _arg3:u64,
    _arg3cage:u64,
    _arg4:u64,
    _arg4cage:u64,
    _arg5:u64, 
    _arg5cage:u64,
    _arg6:u64, 
    _arg6cage:u64, 
) -> u64 {
    let mut handler_table = HANDLERTABLE.lock().unwrap();

    if let Some(src_table) = handler_table.get(&srccage) {
        let srctable = src_table.lock().unwrap().clone();
        if let Some(target_table) = handler_table.get(&targetcage) {
            // If target cage table exists, replace
            *target_table.lock().unwrap() = srctable;
        } else {
            // If not, create a new cage and insert 
            handler_table.insert(targetcage, Arc::new(Mutex::new(CageCallTable::new())));
            handler_table
                .get(&targetcage)
                .unwrap()
                .lock()
                .unwrap()
                .thiscalltable = srctable.thiscalltable.clone();
        }
        return 0;
    } 

    1
}

/***************************** copy_data_between_cages *****************************/
// Validate the memory range for both source (`srcaddr -> srcaddr + srclen`) and destination (`destaddr -> destaddr + destlen`) 
// using the corresponding `vmmap` functions in RawPOSIX.
//
// First, check if the source range is valid and properly mapped.
// Then, check if the destination range is valid:
//  - If any part of the destination range is unmapped, attempt to map it using the appropriate `vmmap` function.
//  - If the destination range becomes valid and satisfies the required permissions after mapping, proceed to 
//      perform the copy operation.
// Otherwise, abort the operation if the mapping fails or permissions are insufficient.
pub fn copy_data_between_cages(
    callnum:u64, 
    targetcage:u64, 
    srcaddr:u64, 
    srccage:u64,
    destaddr:u64, 
    destcage:u64,
    len:u64, 
    _arg3cage:u64,
    copytype:u64, 
    _arg4cage:u64,
    _arg5:u64, 
    _arg5cage:u64,
    _arg6:u64, 
    _arg6cage:u64
) -> u64 {
    // Check address validity and permissions 
    // Validate source address
     if !_validate_addr(srccage, srcaddr, len, PROT_READ).unwrap_or(false) {
        eprintln!("Source address is invalid.");
        return threeiconstant::ELINDAPIABORTED; // Error: Invalid address
    }

    // Validate destination address, and we will try to map if we don't the memory region 
    // unmapping
    if !_validate_addr(destcage, destaddr, len, PROT_WRITE).unwrap_or(false) {
        if !_attemp_dest_mapping(destcage, destaddr, len, PROT_WRITE).unwrap_or(false) {
            eprintln!("Failed to map destination address.");
            return threeiconstant::ELINDAPIABORTED; // Error: Mapping Failed
        }
    }

    // TODO:
    //  - Do we need to consider the permission relationship between cages..? 
    //      ie: only parent cage can perfrom copy..?
    // if !_has_permission(srccage, destcage) {
    //     eprintln!("Permission denied between cages.");
    //     return threeiconstant::ELINDAPIABORTED; // Error: Permission denied
    // }

    // Perform the data copy
    unsafe {
        match copytype {
            0 => { // Raw memory copy
                let src_ptr = srcaddr as *const u8;
                let dest_ptr = destaddr as *mut u8;
                std::ptr::copy_nonoverlapping(src_ptr, dest_ptr, len as usize);
            }
            1 => { // Null-terminated string copy
                let src_ptr = srcaddr as *const u8;
                let dest_ptr = destaddr as *mut u8;
                for i in 0..len {
                    let byte = *src_ptr.add(i as usize);
                    *dest_ptr.add(i as usize) = byte;
                    if byte == 0 {
                        break;
                    }
                }
            }
            _ => {
                eprintln!("Unsupported copy type: {}", copytype);
                return threeiconstant::ELINDAPIABORTED; // Error: Unsupported copy type
            }
        }
    }

    0
}

// Helper function for copy_data_between_cages 
// Validates whether the specified memory range is valid, mapped, and meets the required
// permissions for the given cage. Ensure addr + len does not wrap or exceed bounds
// Return type as `Result` is used to distinguish whether the operation failed because the logic verification 
// failed (such as illegal address) or other errors occurred during program operation (such as system call failure)
fn _validate_addr(
    cage_id: u64,
    addr: u64,
    len: u64,
    required_prot: u64,
) -> Result<bool, io::Error> {
    let cage = interface::cagetable_getref(cage_id);
    let rawposix_vmmap = cage.vmmap.read(); 
    // Get the end address for validation
    let end_addr = addr.checked_add(len).expect("Address computation overflowed");
    // Get the base address of the cage and compute the cage valide address range
    // Memory region per cage = 2**64
    let baseaddr = rawposix_vmmap.base_address;
    let max_addr = baseaddr.checked_add(1 << 64).expect("Address computation overflowed");

    if addr < baseaddr || end_address > max_address {
        return Ok(false); // Address exceeds the cage's valid range
    }

    let start_page = addr >> 12;
    let end_page = (addr + len - 1) >> 12;

    let req_prot = required_prot as i32;
    for page as i32 in start_page..=end_page {
        if let Some(entry) = rawposix_vmmap.find_page(page) {
            if entry.cage_id != cage_id || entry.prot & req_prot != req_prot {
                return Ok(false);
            }
        } else {
            return Ok(false); 
        }
    }

    Ok(true)
}

fn _attemp_dest_mapping(
    cage_id: u64,
    addr: u64,
    len: u64,
    required_prot: u64,
) -> Result<bool, io::Error> {
    let cage = interface::cagetable_getref(cage_id);
    let mut rawposix_vmmap = cage.vmmap.write(); 
    let start_page = addr >> 12;
    let end_page = (addr + len - 1) >> 12;

    // Because we are not sure whether all the pages from destaddr to destaddr+len are mapped, 
    // we loop each page to check and try to map them.
    for page as i32 in start_page..=end_page {
        if rawposix_vmmap.find_page(page).is_none() {
            let new_entry = VmmapEntry::new(
                page,
                1,                       
                required_prot as i32,    
                required_prot as i32,    
                MAP_PRIVATE,             
                false,                   // removed = false
                0,                      
                0,                       
                cage_id,                
                MemoryBackingType::Anonymous, 
            );

            rawposix_vmmap.add_entry(new_entry);
        }
    }

    Ok(true)
}

// -- Check if permissions allow data copying between cages
// TODO:
// How we handle permission relationship...?
// fn _has_permission(srccage: u64, destcage: u64) -> bool {
//     lazy_static::lazy_static! {
//         static ref PERMISSION_TABLE: Mutex<HashMap<u64, HashSet<u64>>> = Mutex::new(HashMap::new());
//     }

//     // Check permission
//     let permission_table = PERMISSION_TABLE.lock().unwrap();
//     if let Some(allowed_destinations) = permission_table.get(&srccage) {
//         if allowed_destinations.contains(&destcage) {
//             return true; 
//         } else {
//             eprintln!(
//                 "Permission denied: Cage {} cannot access Cage {}.",
//                 srccage, destcage
//             );
//             return false;
//         }
//     }
//     false
// }

/***************************** make_syscall *****************************/
pub fn make_syscall(
    callnum:u64, 
    targetcage:u64, 
    arg1:u64, 
    _arg1cage:u64,
    arg2:u64, 
    _arg2cage:u64,
    arg3:u64, 
    _arg3cage:u64,
    arg4:u64, 
    _arg4cage:u64,
    arg5:u64, 
    _arg5cage:u64,
    arg6:u64, 
    _arg6cage:u64,
) -> u64 {
    // Need to check dependencies if targetcage != thiscage
    let handler_table = HANDLERTABLE.lock().unwrap();
    if let Some(cagecall_table) = handler_table.get(&targetcage) {
        let cagecalltable = cagecall_table.lock().unwrap();
        if let Some(jump_address) = cagecalltable.thiscalltable.get(&callnum) {
            unsafe {
                return CallFunc(
                    targetcage,
                    arg1, arg2, arg3, arg4, arg5, arg6,
                );
            }
        }
    }
    1
}
