use crate::threei;

use lazy_static::lazy_static;

use std::sync::Mutex;

use std::collections::HashMap;

// This is a basic fdtables library.  The purpose is to allow a cage to have
// a set of virtual fds which is translated into real fds.


// Get constants about the fd table sizes, etc.
pub use super::commonconstants::*;

// algorithm name.  Need not be listed in the docs.
#[doc(hidden)]
pub const ALGONAME: &str = "VanillaGlobal";

// It's fairly easy to check the fd count on a per-process basis (I just check
// when I would
// add a new fd).
//
// BUG: I will ignore the total limit for now.  I would ideally do this on
// every creation, close, fork, etc. but it's a PITA to track this.

// We will raise a panic anywhere we receive an unknown cageid.  This frankly
// should not be possible and indicates some sort of internal error in our
// code.  However, it is expected we could receive an invalid file descriptor
// when a cage makes a call.

// In order to store this information, I'm going to use a HashMap which
// has keys of (cageid:u64) and values that are another HashMap.  The second
// HashMap has keys of (virtualfd:64) and values of (realfd:u64,
// should_cloexec:bool, optionalinfo:u64).
//
// To speed up lookups, I could have used arrays instead of HashMaps.  In
// theory, that space is far too large, but likely each could be bounded to
// smaller values like 1024.  For simplicity I avoided this for now.
//
// I thought also about having different tables for the tuple of values
// since they aren't always used together, but this seemed needlessly complex
// (at least at first).
//

// This lets me initialize the code as a global.
lazy_static! {

    #[derive(Debug)]
    static ref GLOBALFDTABLE: Mutex<HashMap<u64, HashMap<u64,FDTableEntry>>> = {
        let mut m = HashMap::new();
        // Insert a cage so that I have something to fork / test later, if need
        // be. Otherwise, I'm not sure how I get this started. I think this
        // should be invalid from a 3i standpoint, etc. Could this mask an
        // error in the future?
        // m.insert(threei::TESTING_CAGEID,HashMap::new());
        Mutex::new(m)
    };
}

lazy_static! {
    // This is needed for close and similar functionality.  I need track the
    // number of times a realfd is open
    #[derive(Debug)]
    static ref GLOBALREALFDCOUNT: Mutex<HashMap<u64, u64>> = {
        Mutex::new(HashMap::new())
    };

}

// Internal helper to hold the close handlers...
struct CloseHandlers {
    intermediate_handler: fn(u64),
    final_handler: fn(u64),
    unreal_handler: fn(u64),
}

lazy_static! {
    // This holds the user registered handlers they want to have called when
    // a close occurs.  I did this rather than return messy data structures
    // from the close, exec, and exit handlers because it seemed cleaner...
    #[derive(Debug)]
    static ref CLOSEHANDLERTABLE: Mutex<CloseHandlers> = {
        let c = CloseHandlers {
            intermediate_handler:NULL_FUNC,
            final_handler:NULL_FUNC,
            unreal_handler:NULL_FUNC,
        };
        Mutex::new(c)
    };
}

#[doc = include_str!("../docs/init_empty_cage.md")]
pub fn init_empty_cage(cageid: u64) {

    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if fdtable.contains_key(&cageid) {
        panic!("Known cageid in fdtable access");
    }

    fdtable.insert(cageid,HashMap::new());
}

#[doc = include_str!("../docs/translate_virtual_fd.md")]
pub fn translate_virtual_fd(cageid: u64, virtualfd: u64) -> Result<u64, threei::RetVal> {
    // Get the lock on the fdtable...  I'm not handling "poisoned locks" now
    // where a thread holding the lock died...
    let fdtable = GLOBALFDTABLE.lock().unwrap();

    // They should not be able to pass a new cage I don't know.  I should
    // always have a table for each cage because each new cage is added at fork
    // time
    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // Below condition checks if the virtualfd is out of bounds and if yes it throws an error
    // Note that this assumes that all virtualfd numbers returned < FD_PER_PROCESS_MAX 
    if virtualfd >= FD_PER_PROCESS_MAX {
        return Err(threei::Errno::EBADFD as u64);
    }

    return match fdtable.get(&cageid).unwrap().get(&virtualfd) {
        Some(tableentry) => Ok(tableentry.realfd),
        None => Err(threei::Errno::EBADFD as u64),
    };
}

// This is fairly slow if I just iterate sequentially through numbers.
// However there are not that many to choose from.  I could pop from a list
// or a set as well...  Likely the best solution is to keep a count of the
// largest fd handed out and to just use this until you wrap.  This will be
// super fast for a normal cage and will be correct in the weird case.
// Right now, I'll just implement the slow path and will speed this up
// later, if needed.
#[doc = include_str!("../docs/get_unused_virtual_fd.md")]
pub fn get_unused_virtual_fd(
    cageid: u64,
    realfd: u64,
    should_cloexec: bool,
    optionalinfo: u64,
) -> Result<u64, threei::RetVal> {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }
    // Set up the entry so it has the right info...
    // Note, a HashMap stores its data on the heap!  No need to box it...
    // https://doc.rust-lang.org/book/ch08-03-hash-maps.html#creating-a-new-hash-map
    let myentry = FDTableEntry {
        realfd,
        should_cloexec,
        optionalinfo,
    };

    let myfdmap = fdtable.get_mut(&cageid).unwrap();

    // Check the fds in order.
    for fdcandidate in 0..FD_PER_PROCESS_MAX {
        // Get the entry if it's Vacant and assign it to e (so I can fill
        // it in).
        if let std::collections::hash_map::Entry::Vacant(e) = myfdmap.entry(fdcandidate) {
            e.insert(myentry);
            _increment_realfd(realfd);
            return Ok(fdcandidate);
        }
    }

    // I must have checked all fds and failed to find one open.  Fail!
    Err(threei::Errno::EMFILE as u64)
}

// This is used for things like dup2, which need a specific fd...
// If the requested_virtualfd is used, I close it...
#[doc = include_str!("../docs/get_specific_virtual_fd.md")]
pub fn get_specific_virtual_fd(
    cageid: u64,
    requested_virtualfd: u64,
    realfd: u64,
    should_cloexec: bool,
    optionalinfo: u64,
) -> Result<(), threei::RetVal> {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // If you ask for a FD number that is too large, I'm going to reject it.
    // Note that, I need to use the FD_PER_PROCESS_MAX setting because this
    // is also how I'm tracking how many values you have open.  If this
    // changed, then these constants could be decoupled...
    if requested_virtualfd > FD_PER_PROCESS_MAX {
        return Err(threei::Errno::EBADF as u64);
    }

    // Set up the entry so it has the right info...
    // Note, a HashMap stores its data on the heap!  No need to box it...
    // https://doc.rust-lang.org/book/ch08-03-hash-maps.html#creating-a-new-hash-map
    let myentry = FDTableEntry {
        realfd,
        should_cloexec,
        optionalinfo,
    };

    // I moved this up so that if I decrement the same realfd, it calls
    // the intermediate handler instead of the final one.
    _increment_realfd(realfd);

    // always add the new entry.  insert returns the old entry.
    let myoptionentry = fdtable.get_mut(&cageid).unwrap().insert(requested_virtualfd,myentry);
    drop(fdtable);

    // Close the old entry, if I need to...
    if let Some(entry) = myoptionentry {
        if entry.realfd != NO_REAL_FD {
            _decrement_realfd(entry.realfd);
        }
        else {
            // Let their code know this has been closed...
            let unrealclosehandler = (*CLOSEHANDLERTABLE.lock().unwrap()).unreal_handler;
            (unrealclosehandler)(entry.optionalinfo);
        }
    }

    Ok(())
}

// We're just setting a flag here, so this should be pretty straightforward.
#[doc = include_str!("../docs/set_cloexec.md")]
pub fn set_cloexec(cageid: u64, virtualfd: u64, is_cloexec: bool) -> Result<(), threei::RetVal> {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // Set the is_cloexec flag or return EBADFD, if that's missing...
    return match fdtable.get_mut(&cageid).unwrap().get_mut(&virtualfd) {
        Some(tableentry) => {
            tableentry.should_cloexec = is_cloexec;
            Ok(())
        }
        None => Err(threei::Errno::EBADFD as u64),
    };
}

// Super easy, just return the optionalinfo field...
#[doc = include_str!("../docs/get_optionalinfo.md")]
pub fn get_optionalinfo(cageid: u64, virtualfd: u64) -> Result<u64, threei::RetVal> {
    let fdtable = GLOBALFDTABLE.lock().unwrap();
    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    return match fdtable.get(&cageid).unwrap().get(&virtualfd) {
        Some(tableentry) => Ok(tableentry.optionalinfo),
        None => Err(threei::Errno::EBADFD as u64),
    };
}

// We're setting an opaque value here. This should be pretty straightforward.
#[doc = include_str!("../docs/set_optionalinfo.md")]
pub fn set_optionalinfo(
    cageid: u64,
    virtualfd: u64,
    optionalinfo: u64,
) -> Result<(), threei::RetVal> {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // Set optionalinfo or return EBADFD, if that's missing...
    return match fdtable.get_mut(&cageid).unwrap().get_mut(&virtualfd) {
        Some(tableentry) => {
            tableentry.optionalinfo = optionalinfo;
            Ok(())
        }
        None => Err(threei::Errno::EBADFD as u64),
    };
}

// Helper function used for fork...  Copies an fdtable for another process
#[doc = include_str!("../docs/copy_fdtable_for_cage.md")]
pub fn copy_fdtable_for_cage(srccageid: u64, newcageid: u64) -> Result<(), threei::Errno> {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&srccageid) {
        panic!("Unknown srccageid in fdtable access");
    }
    if fdtable.contains_key(&newcageid) {
        panic!("Known newcageid in fdtable access");
    }

    // Insert a copy and ensure it didn't exist...
    let hmcopy = fdtable.get(&srccageid).unwrap().clone();

    // increment the reference to items in the fdtable appropriately...
    for v in fdtable.get(&srccageid).unwrap().values() {
        if v.realfd != NO_REAL_FD {
            _increment_realfd(v.realfd);
        }
    }

    // insert the new table...
    assert!(fdtable.insert(newcageid, hmcopy).is_none());
    Ok(())
    // I'm not going to bother to check the number of fds used overall yet...
    //    Err(threei::Errno::EMFILE as u64),
}

// This is mostly used in handling exit, etc.  Returns the HashMap
// for the cage.
#[doc = include_str!("../docs/remove_cage_from_fdtable.md")]
pub fn remove_cage_from_fdtable(cageid: u64) {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    let cagetable = fdtable.remove(&cageid).unwrap();
    drop(fdtable);

    // decrement the reference to items in the fdtable appropriately...
    for v in cagetable.values() {
        if v.realfd != NO_REAL_FD {
            _decrement_realfd(v.realfd);
        }
        else {
            // Let their code know this has been closed...
            let unrealclosehandler = CLOSEHANDLERTABLE.lock().unwrap().unreal_handler;
            (unrealclosehandler)(v.optionalinfo);
        }
    }

}

// This removes all fds with the should_cloexec flag set.  They are returned
// in a new hashmap...
#[doc = include_str!("../docs/empty_fds_for_exec.md")]
pub fn empty_fds_for_exec(cageid: u64) {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // Create this hashmap through an lambda that checks should_cloexec...
    // See: https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.extract_if

/*    fdtable
        .get_mut(&cageid)
        .unwrap()
        .extract_if(|_k, v| v.should_cloexec)
        .collect()*/

    // I'm writing the below code to avoid using the extract_if experimental 
    // nightly function...
    let thiscagefdtable = fdtable.get_mut(&cageid).unwrap();

    let mut without_cloexec_hm:HashMap<u64,FDTableEntry> = HashMap::new();
    // I bother to put this in a hashmap so I can call the closehandlers
    // all after I have re-inserted everything.  This ensures the state
    // is consistent.  I only need the values, not the keys...
    let mut with_cloexec_vec:Vec<FDTableEntry> = Vec::new();

    for (k,v) in thiscagefdtable.drain() {
        if v.should_cloexec {
            with_cloexec_vec.push(v);
        }
        else{
            without_cloexec_hm.insert(k,v);
        }

    }

    // Put the ones without_cloexec back in the hashmap since they shouldn't 
    // be closed...
    fdtable.insert(cageid,without_cloexec_hm);
    // Release the lock...
    drop(fdtable);

    // Now call the close handlers on the others...
    for v in with_cloexec_vec {
        if v.realfd == NO_REAL_FD {
            // Let their code know this has been closed...  I get the handler
            // repeatedly in case they change it during execution of another
            // handler...
            let closehandlerunreal = CLOSEHANDLERTABLE.lock().unwrap().unreal_handler;
            (closehandlerunreal)(v.optionalinfo);
        }
        else {
            // Let the helper tell the user and decrement the count
            _decrement_realfd(v.realfd);
        }
    }

}

// returns a copy of the fdtable for a cage.  Useful helper function for a
// caller that needs to examine the table.  Likely could be more efficient by
// letting the caller borrow this...
#[doc = include_str!("../docs/return_fdtable_copy.md")]
pub fn return_fdtable_copy(cageid: u64) -> HashMap<u64, FDTableEntry> {
    let fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    fdtable.get(&cageid).unwrap().clone()
}

/******************* CLOSE SPECIFIC FUNCTIONALITY *******************/

// Helper for close.  Returns a tuple of realfd, number of references
// remaining.
#[doc = include_str!("../docs/close_virtualfd.md")]
pub fn close_virtualfd(cageid:u64, virtfd:u64) -> Result<(),threei::RetVal> {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap();

    if !fdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // Remove this item from the table (and inspect it)
    let thisoption = fdtable.get_mut(&cageid).unwrap().remove(&virtfd);
    drop(fdtable);

    match thisoption {
        Some(entry) =>
            if entry.realfd == NO_REAL_FD {
                // Let their code know this has been closed...
                let closehandlerunreal = CLOSEHANDLERTABLE.lock().unwrap().unreal_handler;
                let optionalinfo = entry.optionalinfo;
                (closehandlerunreal)(optionalinfo);
                Ok(())
            }
            else {
                _decrement_realfd(entry.realfd);
                Ok(())
            }
        None => Err(threei::Errno::EBADFD as u64),
    }
}


// Register a series of helpers to be called for close.  Can be called
// multiple times to override the older helpers.
#[doc = include_str!("../docs/register_close_handlers.md")]
pub fn register_close_handlers(intermediate_handler: fn(u64), final_handler: fn(u64), unreal_handler: fn(u64)) {
    // Unlock the table and set the handlers...
    // TODO: I made a serious attempt to try to keep closehandlers that call
    // close, etc. from deadlocking the system.  More testing, etc. is needed.
    let mut closehandlers = CLOSEHANDLERTABLE.lock().unwrap();
    closehandlers.intermediate_handler = intermediate_handler;
    closehandlers.final_handler = final_handler;
    closehandlers.unreal_handler = unreal_handler;
}

// Helpers to track the count of times each realfd is used
#[doc(hidden)]
fn _decrement_realfd(realfd:u64) -> u64 {
    // Do nothing if it's not a realfd...
    assert!(realfd != NO_REAL_FD, "Called _decrement_realfd with NO_REAL_FD");

    // Get this table's lock...
    let mut realfdcount = GLOBALREALFDCOUNT.lock().unwrap();

    let newcount:u64 = realfdcount.get(&realfd).unwrap() - 1;
    let closehandlers = CLOSEHANDLERTABLE.lock().unwrap();
    let intermediateclosehandler = closehandlers.intermediate_handler;
    let finalclosehandler = closehandlers.final_handler;
    // release the lock...
    drop(closehandlers);

    if newcount > 0 {
        realfdcount.insert(realfd,newcount);
        // Need to drop locks to call the handlers or else will deadlock...
        drop(realfdcount);

        (intermediateclosehandler)(realfd);
    }
    else {
        // Remove before calling their close handler in case they do operations
        // inside the close handler which create / close fds...
        realfdcount.remove(&realfd);
        // Need to drop locks to call the handlers or else will deadlock...
        drop(realfdcount);

        (finalclosehandler)(realfd);
    }
    newcount
}

// Helpers to track the count of times each realfd is used
#[doc(hidden)]
fn _increment_realfd(realfd:u64) -> u64 {
    if realfd == NO_REAL_FD {
        return 0
    }

    // Get this table's lock...
    let mut realfdcount = GLOBALREALFDCOUNT.lock().unwrap();

    // Get a mutable reference to the entry so we can update it.
    return match realfdcount.get_mut(&realfd) {
        Some(count) => {
            *count += 1;
            *count
        }
        None => {
            realfdcount.insert(realfd, 1);
            1
        }
    }
}

/***************   Code for handling select() ****************/

use libc::fd_set;
use std::collections::HashSet;
use std::cmp;
use std::mem;

// Helper to get an empty fd_set.  Helper function to isolate unsafe code,
// etc.
#[doc(hidden)]
pub fn _init_fd_set() -> fd_set {
    let raw_fd_set:fd_set;
    unsafe {
        let mut this_fd_set = mem::MaybeUninit::<libc::fd_set>::uninit();
        libc::FD_ZERO(this_fd_set.as_mut_ptr());
        raw_fd_set = this_fd_set.assume_init()
    }
    raw_fd_set
}

#[doc(hidden)]
pub fn _fd_set(fd:u64, thisfdset:&mut fd_set) {
    unsafe{libc::FD_SET(fd as i32,thisfdset)}
}

#[doc(hidden)]
pub fn _fd_isset(fd:u64, thisfdset:&fd_set) -> bool {
    unsafe{libc::FD_ISSET(fd as i32,thisfdset)}
}

// Computes the bitmodifications and returns a (maxnfds, unrealset) tuple...
#[doc(hidden)]
fn _do_bitmods(myfdmap:HashMap<u64,FDTableEntry>, nfds:u64, infdset: fd_set, thisfdset: &mut fd_set, mappingtable: &mut HashMap<u64,u64>) -> Result<(u64,HashSet<(u64,u64)>),threei::RetVal> {
    let mut unrealhashset:HashSet<(u64,u64)> = HashSet::new();
    // Iterate through the infdset and set those values as is appropriate
    let mut highestpos = 0;

    // Clippy is somehow missing how pos is using bit.
    #[allow(clippy::needless_range_loop)]
    for bit in 0..nfds as usize {
        let pos = bit as u64;
        if _fd_isset(pos,&infdset) {
            if let Some(entry) = myfdmap.get(&pos) {
                if entry.realfd == NO_REAL_FD {
                    unrealhashset.insert((pos,entry.optionalinfo));
                }
                else {
                    mappingtable.insert(entry.realfd, pos);
                    _fd_set(entry.realfd,thisfdset);
                    // I add one because select expects nfds to be the max+1
                    highestpos = cmp::max(highestpos, entry.realfd+1);
                }
            }
            else {
                return Err(threei::Errno::EINVAL as u64);
            }
        }
    }
    Ok((highestpos,unrealhashset))
}

// helper to call before calling select beneath you.  Translates your virtfds
// to realfds.
// See: https://man7.org/linux/man-pages/man2/select.2.html for details /
// corner cases about the arguments.

// I hate doing these, but don't know how to make this interface better...
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
#[doc = include_str!("../docs/get_real_bitmasks_for_select.md")]
pub fn get_real_bitmasks_for_select(cageid:u64, nfds:u64, readbits:Option<fd_set>, writebits:Option<fd_set>, exceptbits:Option<fd_set>) -> Result<(u64, Option<fd_set>, Option<fd_set>, Option<fd_set>, [HashSet<(u64,u64)>;3], HashMap<u64,u64>),threei::RetVal> {

    if nfds >= FD_PER_PROCESS_MAX {
        return Err(threei::Errno::EINVAL as u64);
    }

    let globfdtable = GLOBALFDTABLE.lock().unwrap();

    if !globfdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    let mut unrealarray:[HashSet<(u64,u64)>;3] = [HashSet::new(),HashSet::new(),HashSet::new()];
    let mut mappingtable:HashMap<u64,u64> = HashMap::new();
    let mut newnfds = 0;

    // putting results in a vec was the cleanest way I found to do this..
    let mut resultvec = Vec::new();

    for (unrealoffset, inset) in [readbits,writebits, exceptbits].into_iter().enumerate() {
        match inset {
            Some(virtualbits) => {
                let mut retset = _init_fd_set();
                let (thisnfds,myunrealhashset) = _do_bitmods(globfdtable.get(&cageid).unwrap().clone(),nfds,virtualbits, &mut retset,&mut mappingtable)?;
                resultvec.push(Some(retset));
                newnfds = cmp::max(thisnfds, newnfds);
                unrealarray[unrealoffset] = myunrealhashset;
            }
            None => {
                // This item is null.  No unreal items
                resultvec.push(None);
                unrealarray[unrealoffset] = HashSet::new();
            }
        }
    }

    Ok((newnfds, resultvec[0], resultvec[1], resultvec[2], unrealarray, mappingtable))

}


// helper to call after calling select beneath you.  returns the fd_sets you
// need for your return from a select call and the number of unique flags
// set...

// I hate doing these, but don't know how to make this interface better...
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
#[doc = include_str!("../docs/get_virtual_bitmasks_from_select_result.md")]
pub fn get_virtual_bitmasks_from_select_result(nfds:u64, readbits:Option<fd_set>, writebits:Option<fd_set>, exceptbits:Option<fd_set>,unrealreadset:HashSet<u64>, unrealwriteset:HashSet<u64>, unrealexceptset:HashSet<u64>, mappingtable:&HashMap<u64,u64>) -> Result<(u64, Option<fd_set>, Option<fd_set>, Option<fd_set>),threei::RetVal> {

    // Note, I don't need the cage_id here because I have the mappingtable...

    if nfds >= FD_PER_PROCESS_MAX {
        panic!("This shouldn't be possible because we shouldn't have returned this previously")
    }

    let mut flagsset = 0;
    let mut retvec = Vec::new();

    for (insetoption,unrealset) in [(readbits,unrealreadset), (writebits,unrealwriteset), (exceptbits,unrealexceptset)] {
        // If I don't have any data, just return None (NULL) and skip...
        if insetoption.is_none()&&unrealset.is_empty() {
            retvec.push(None);
            continue;
        }

        let mut retbits = _init_fd_set();
        if let Some(inset) = insetoption {
            for bit in 0..nfds as usize {
                let pos = bit as u64;
                if _fd_isset(pos,&inset)&& !_fd_isset(*mappingtable.get(&pos).unwrap(),&retbits) {
                    flagsset+=1;
                    _fd_set(*mappingtable.get(&pos).unwrap(),&mut retbits);
                }
            }
        }
        for virtfd in unrealset {
            if !_fd_isset(virtfd,&retbits) {
                flagsset+=1;
                _fd_set(virtfd,&mut retbits);
            }
        }
        retvec.push(Some(retbits));
    }

    Ok((flagsset,retvec[0],retvec[1],retvec[2]))

}



/********************** POLL SPECIFIC FUNCTIONS **********************/

// helper to call before calling poll beneath you.  replaces the fds in
// the poll struct with virtual versions and returns the items you need
// to check yourself...
#[allow(clippy::type_complexity)]
#[doc = include_str!("../docs/convert_virtualfds_to_real.md")]
pub fn convert_virtualfds_to_real(cageid:u64, virtualfds:Vec<u64>) -> (Vec<u64>, Vec<(u64,u64)>, Vec<u64>, HashMap<u64,u64>) {

    let globfdtable = GLOBALFDTABLE.lock().unwrap();

    if !globfdtable.contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    let mut unrealvec = Vec::new();
    let mut realvec = Vec::new();
    let mut invalidvec = Vec::new();
    let thefdhm = globfdtable.get(&cageid).unwrap();
    let mut mappingtable:HashMap<u64,u64> = HashMap::new();

    // BUG?: I'm ignoring the fact that virtualfds can show up multiple times.
    // I'm not sure this actually matters, but I didn't think hard about it.
    for virtfd in virtualfds {
        match thefdhm.get(&virtfd) {
            Some(entry) => {
                // always append the value here.  NO_REAL_FD will be added
                // in the appropriate places to tell them to handle those calls
                // themself.
                realvec.push(entry.realfd);
                if entry.realfd == NO_REAL_FD {
                    unrealvec.push((virtfd,entry.optionalinfo));
                }
                else{
                    mappingtable.insert(entry.realfd, virtfd);
                }
            }
            None => {
                // Add this because they need to handle it if POLLNVAL is set.
                // An exception should not be raised!!!
                realvec.push(INVALID_FD);
                invalidvec.push(virtfd);
            }
        }
    }

    (realvec, unrealvec, invalidvec, mappingtable)
}



// helper to call after calling poll.  replaces the realfds the vector
// with virtual ones...
#[doc = include_str!("../docs/convert_realfds_back_to_virtual.md")]
pub fn convert_realfds_back_to_virtual(realfds:Vec<u64>, mappingtable:&HashMap<u64,u64>) -> Vec<u64> {

    // I don't care what cage was used, and don't need to lock anything...
    // I have the mappingtable!

    let mut virtvec = Vec::new();

    for realfd in realfds {
        virtvec.push(*mappingtable.get(&realfd).unwrap());
    }

    virtvec
}

/********************** EPOLL SPECIFIC FUNCTIONS **********************/


// Okay this adds a big wrinkle, epollfds.  The reason these are complex is
// multi-fold:
// 1) they themselves are file descriptors and take up a slot.
// 2) a epollfd can point to any number of other fds
// 3) an epollfd can point to epollfds, which can point to other epollfds, etc.
//    and possibly cause a loop to occur (which is an error)
// 4) an epollfd can point to a mix of virtual and realfds.
//
// My thinking is this is handled as similarly to poll as possible.  We push
// off the problem of understanding what the event types are to the implementer
// of the library.
//
// In my view, epoll_wait() is quite simple to support.  One basically just
// keeps a list of virtual fds for this epollfd and their corresponding event
// types, which they may need to poll themselves.  After this, they handle the
// call.
//
// epoll_create makes a new fd type, which really is unfortunate.  To this
// point, I haven't had to care about anything except in-memory fds (unreal),
// and doing the virtual <-> real mappings.  The caller can decide whether to
// create an underlying epollfd when this is called, when epoll_ctl is called
// to add a realfd, etc.
//
// epoll_ctl is complex, but really has the same fundamental problem as
// epoll_create: the epollfd.
//
// What if I just ignore the epollfd problem by just making another table
// for epoll information?  Then what I do is set the realfd to EPOLLFD and
// have optionalinfo point into the epollfd table.  If I do this, then when
// epoll_create is called, if it contains realfds, those need to be passed
// down to the underlying epoll_create.  Similarly, when epoll_ctl is called,
// we either modify our data or return the realfd...
//
// Interestingly, this actually would be just as easy to build on top of the
// fdtables library as into it.
//
// Each epollfd will have some virtual fds associated with it.  Each of those
// will have an event mask.  So I'll have a mutex around an EPollTable struct.
// This contains the next available entry and an epollhashmap<virtfd, event>.
// I use a hashmap here to better support removing and modifying items.

// Note, I'm defining a bunch of symbols myself because libc doesn't import
// them on systems that don't support epoll and I want to be able to build
// the code anywhere.  See commonconstants.rs for more info.

// Design notes: I'm not adding realfds.  I return them when you do a epoll_ctl
// operation that tries to add them.  So, I only have unrealfds in my epoll
// structures.

// TODO: I don't clean up this table yet.  I probably should when the last
// reference to a fd is closed, but this bookkeeping seems excessive at this
// time...
#[derive(Clone, Debug)]
struct EPollTable {
    highestneverusedentry: u64, // Never resets (even after close).  Used to
                                // let us quickly get an unused entry
    thisepolltable: HashMap<u64,HashMap<u64,epoll_event>>, // the epollentry ->
                                                           // virtfd ->
                                                           // event map
    realfdtable: HashMap<u64,u64>, // the epollentry -> realfd map.  I need 
                                   // this because the realfd field in the
                                   // main data structure is EPOLLFD
}

lazy_static! {

    #[derive(Debug)]
    static ref EPOLLTABLE: Mutex<EPollTable> = {
        let newetable = HashMap::new();
        let newrealfdtable = HashMap::new();
        let m = EPollTable {
            highestneverusedentry:0,
            realfdtable:newrealfdtable,
            thisepolltable:newetable,
        };
        Mutex::new(m)
    };
}


#[doc = include_str!("../docs/epoll_create_helper.md")]
pub fn epoll_create_helper(cageid:u64, realfd:u64, should_cloexec:bool) -> Result<u64,threei::RetVal> {

    let mut ept = EPOLLTABLE.lock().unwrap();

    // I'll use my other functions to make this easier.
    // return the same errno (EMFile), if we get one
    let newepollfd = get_unused_virtual_fd(cageid, EPOLLFD, should_cloexec, ept.highestneverusedentry)?;

    let newentry = ept.highestneverusedentry;
    ept.highestneverusedentry+=1;
    // add in my realfd.
    ept.realfdtable.insert(newentry,realfd);
    // if it errored out above that is okay. I haven't changed any state yet.
    ept.thisepolltable.insert(newentry, HashMap::new());
    Ok(newepollfd)

}



#[doc = include_str!("../docs/try_epoll_ctl.md")]
pub fn try_epoll_ctl(cageid:u64, epfd:u64, op:i32, virtfd:u64, event:epoll_event) -> Result<(u64,u64),threei::RetVal> {

    if !GLOBALFDTABLE.lock().unwrap().contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    if epfd == virtfd {
        return Err(threei::Errno::EINVAL as u64);
    }
    // Is the epfd ok?
    let epollentrynum:u64 = match GLOBALFDTABLE.lock().unwrap().get(&cageid).unwrap().get(&epfd) {
        None => {
            return Err(threei::Errno::EBADF as u64);
        },
        // Do I need to have EPOLLFDs here too?
        Some(tableentry) => {
            if tableentry.realfd != EPOLLFD {
                return Err(threei::Errno::EINVAL as u64);
            }
            tableentry.optionalinfo
        },
    };

    // Okay, I know which table entry and verified the virtfd...

    let mut epttable = EPOLLTABLE.lock().unwrap();
    let realepollfd = epttable.realfdtable.get(&epollentrynum).unwrap().clone();
    let eptentry = epttable.thisepolltable.get_mut(&epollentrynum).unwrap();

    // check if the virtfd is real and error...
    // I don't care about its contents except to ensure it isn't real...
    match GLOBALFDTABLE.lock().unwrap().get(&cageid).unwrap().get(&virtfd) {
        // Do I need to have EPOLLFDs here too?
        Some(tableentry) => {
            if tableentry.realfd != NO_REAL_FD {
                // Return realfds because the caller should handle them instead
                // I only track unrealfds.
                if tableentry.realfd == EPOLLFD {
                    // BUG: How should I be doing this, really!?!
                    println!("epollfds acting on epollfds is not supported!");
                }
                return Ok((realepollfd,tableentry.realfd));
            }
        },
        None => {
            return Err(threei::Errno::EBADF as u64);
        },
    };

    // okay, virtfd is real...

    match op {
        EPOLL_CTL_ADD => {
            if eptentry.contains_key(&virtfd) {
                return Err(threei::Errno::EEXIST as u64);
            }
            // BUG: Need to check for ELOOP here...

            eptentry.insert(virtfd, event);
        },
        EPOLL_CTL_MOD => {
            if !eptentry.contains_key(&virtfd) {
                return Err(threei::Errno::ENOENT as u64);
            }
            eptentry.insert(virtfd, event);
        },
        EPOLL_CTL_DEL => {
            if !eptentry.contains_key(&virtfd) {
                return Err(threei::Errno::ENOENT as u64);
            }
            eptentry.remove(&virtfd);
        },
        _ => {
            return Err(threei::Errno::EINVAL as u64);
        },
    };
    Ok((realepollfd,NO_REAL_FD))
}


#[doc = include_str!("../docs/get_epoll_wait_data.md")]
pub fn get_epoll_wait_data(cageid:u64, epfd:u64) -> Result<(u64,HashMap<u64,epoll_event>),threei::RetVal> {

    if !GLOBALFDTABLE.lock().unwrap().contains_key(&cageid) {
        panic!("Unknown cageid in fdtable access");
    }

    // Note that because I don't track realfds or deal with epollfds, I just
    // return the epolltable...
    let epollentrynum:u64 = match GLOBALFDTABLE.lock().unwrap().get(&cageid).unwrap().get(&epfd) {
        None => {
            return Err(threei::Errno::EBADF as u64);
        },
        // Do I need to have EPOLLFDs here too?
        Some(tableentry) => {
            if tableentry.realfd != EPOLLFD {
                return Err(threei::Errno::EINVAL as u64);
            }
            tableentry.optionalinfo
        },
    };

    let epttable = EPOLLTABLE.lock().unwrap();
    Ok((*epttable.realfdtable.get(&epollentrynum).unwrap(),epttable.thisepolltable[&epollentrynum].clone()))
}


/********************** TESTING HELPER FUNCTION **********************/

// Helper to initialize / empty out state so we can test with a clean system...
// only used when testing...
//
// I'm cleaning up "poisoned" mutexes here so that I can handle tests that 
// panic
#[doc(hidden)]
pub fn refresh() {
    let mut fdtable = GLOBALFDTABLE.lock().unwrap_or_else(|e| {
        GLOBALFDTABLE.clear_poison();
        e.into_inner()
    });
    fdtable.clear();
    fdtable.insert(threei::TESTING_CAGEID, HashMap::new());
    let mut closehandlers = CLOSEHANDLERTABLE.lock().unwrap_or_else(|e| {
        CLOSEHANDLERTABLE.clear_poison();
        e.into_inner()
    });
    closehandlers.intermediate_handler = NULL_FUNC;
    closehandlers.final_handler = NULL_FUNC;
    closehandlers.unreal_handler = NULL_FUNC;
    let mut _realfdcount = GLOBALREALFDCOUNT.lock().unwrap_or_else(|e| {
        GLOBALREALFDCOUNT.clear_poison();
        e.into_inner()
    });
}

