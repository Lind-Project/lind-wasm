//  DashMap<u64,vec![Option<FDTableEntry>;FD_PER_PROCESSS_MAX]>  Space is ~30KB 
//  per cage w/ 1024 fds?!?
//      Static DashMap.  Let's see if having the FDTableEntries be a Vector
//      is any faster...

use crate::threei;

use dashmap::DashMap;

use lazy_static::lazy_static;

use std::collections::HashMap;

use std::sync::Mutex;

// This uses a Dashmap (for cages) with an array of FDTableEntry items.

// Get constants about the fd table sizes, etc.
pub use super::commonconstants::*;

// algorithm name.  Need not be listed.  Used in benchmarking output
#[doc(hidden)]
pub const ALGONAME: &str = "DashMapVecGlobal";

// It's fairly easy to check the fd count on a per-process basis (I just check
// when I would add a new fd).
//
// TODO: I will ignore the total limit for now.  I would ideally do this on
// every creation, close, fork, etc. but it's a PITA to track this.

// We will raise a panic anywhere we receive an unknown cageid.  This frankly
// should not be possible and indicates some sort of internal error in our
// code.  However, other issues, such as an invalid file descriptor when a 
// cage makes a call, will be handled by returning the appropriate errno.

// In order to store this information, I'm going to use a DashMap which
// has keys of (cageid:u64) and values that are an array of FD_PER_PROCESS_MAX
// Option<FDTableEntry> items. 
//
//

// This lets me initialize the code as a global.
lazy_static! {

    #[derive(Debug)]
    pub static ref FDTABLE: DashMap<u64, Vec<Option<FDTableEntry>>> = {
        let m = DashMap::new();
        // Insert a cage so that I have something to fork / test later, if need
        // be. Otherwise, I'm not sure how I get this started. I think this
        // should be invalid from a 3i standpoint, etc. Could this mask an
        // error in the future?
        // m.insert(threei::TESTING_CAGEID,vec!(Option::None;FD_PER_PROCESS_MAX as usize));
        m
    };
}

lazy_static! {
    // This is needed for close and similar functionality.  I need track the
    // number of times a (fdkind,underfd) is open.  Note that this is across 
    // cages in order to enable a library to have  situations where two cages 
    // have the same fd open.  The (fdkind,underfd) tuple is the key and the 
    // number of times it appears is the value.  If it reaches 0, the entry 
    // is removed.
    #[derive(Debug)]
    static ref FDCOUNT: DashMap<(u32,u64), u64> = {
        DashMap::new()
    };

}

#[doc = include_str!("../docs/init_empty_cage.md")]
pub fn init_empty_cage(cageid: u64) {

    assert!(!FDTABLE.contains_key(&cageid),"Known cageid in fdtable access");

    FDTABLE.insert(cageid,vec!(Option::None;FD_PER_PROCESS_MAX as usize));
}

#[doc = include_str!("../docs/translate_virtual_fd.md")]
pub fn translate_virtual_fd(cageid: u64, virtualfd: u64) -> Result<FDTableEntry, threei::RetVal> {

    // They should not be able to pass a new cage I don't know.  I should
    // always have a table for each cage because each new cage is added at fork
    // time
    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");
    // Below condition checks if the virtualfd is out of bounds and if yes it throws an error
    // Note that this assumes that all virtualfd numbers returned < FD_PER_PROCESS_MAX 
    if virtualfd >= FD_PER_PROCESS_MAX {
        return Err(threei::Errno::EBADFD as u64);
    }

    return match FDTABLE.get(&cageid).unwrap()[virtualfd as usize] {
        Some(tableentry) => Ok(tableentry),
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
    fdkind: u32,
    underfd: u64,
    should_cloexec: bool,
    perfdinfo: u64,
) -> Result<u64, threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");
    // Set up the entry so it has the right info...
    // Note, a HashMap stores its data on the heap!  No need to box it...
    // https://doc.rust-lang.org/book/ch08-03-hash-maps.html#creating-a-new-hash-map
    let myentry = FDTableEntry {
        fdkind,
        underfd,
        should_cloexec,
        perfdinfo,
    };

    let mut myfdrow = FDTABLE.get_mut(&cageid).unwrap();

    // Check the fds in order.
    for fdcandidate in 0..FD_PER_PROCESS_MAX {
        // FIXME: This is likely very slow.  Should do something smarter...
        if myfdrow[fdcandidate as usize].is_none() {
            // I just checked.  Should not be there...
            myfdrow[fdcandidate as usize] = Some(myentry);
            _increment_fdcount(myentry);
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
    fdkind: u32,
    underfd: u64,
    should_cloexec: bool,
    perfdinfo: u64,
) -> Result<(), threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

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
        fdkind,
        underfd,
        should_cloexec,
        perfdinfo,
    };

    // This is before the FDTABLE action, so if I decrement the same fd, it
    // calls the intermediate handler instead of the last one.
    _increment_fdcount(myentry);
    let myoptionentry = FDTABLE.get(&cageid).unwrap()[requested_virtualfd as usize];
    // always add the new entry.  I'm doing this first, before I close
    // the old one because I need to ensure I've cleaned up state correctly
    // before calling the close handlers...
    FDTABLE.get_mut(&cageid).unwrap()[requested_virtualfd as usize] = Some(myentry);

    // Update the fdcount / close the old entry, if existed
    if let Some(entry) = myoptionentry {
        _decrement_fdcount(entry);
    }

    Ok(())
}

// We're just setting a flag here, so this should be pretty straightforward.
#[doc = include_str!("../docs/set_cloexec.md")]
pub fn set_cloexec(cageid: u64, virtualfd: u64, is_cloexec: bool) -> Result<(), threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    // return EBADFD, if the fd is missing...
    if FDTABLE.get(&cageid).unwrap()[virtualfd as usize].is_none() {
        return Err(threei::Errno::EBADFD as u64);
    }
    // Set the is_cloexec flag
    FDTABLE.get_mut(&cageid).unwrap()[virtualfd as usize].as_mut().unwrap().should_cloexec = is_cloexec;
    Ok(())
}

// We're setting an opaque value here. This should be pretty straightforward.
#[doc = include_str!("../docs/set_perfdinfo.md")]
pub fn set_perfdinfo(
    cageid: u64,
    virtualfd: u64,
    perfdinfo: u64,
) -> Result<(), threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    // return EBADFD, if the fd is missing...
    if FDTABLE.get(&cageid).unwrap()[virtualfd as usize].is_none() {
        return Err(threei::Errno::EBADFD as u64);
    }

    // Set optionalinfo or return EBADFD, if that's missing...
    FDTABLE.get_mut(&cageid).unwrap()[virtualfd as usize].as_mut().unwrap().perfdinfo = perfdinfo;
    Ok(())
}

// Helper function used for fork...  Copies an fdtable for another process
#[doc = include_str!("../docs/copy_fdtable_for_cage.md")]
pub fn copy_fdtable_for_cage(srccageid: u64, newcageid: u64) -> Result<(), threei::Errno> {

    assert!(FDTABLE.contains_key(&srccageid),"Unknown cageid in fdtable access");
    assert!(!FDTABLE.contains_key(&newcageid),"Known cageid in fdtable access");

    // Insert a copy and ensure it didn't exist...
    let hmcopy = FDTABLE.get(&srccageid).unwrap().clone();

    // Increment copied items
    for entry in FDTABLE.get(&srccageid).unwrap().iter() {
        if entry.is_some() {
            _increment_fdcount(entry.unwrap());
        }
    }

    assert!(FDTABLE.insert(newcageid, hmcopy).is_none());
    
    // I'm not going to bother to check the number of fds used overall yet...
    //    Err(threei::Errno::EMFILE as u64),
    Ok(())
}

// This is mostly used in handling exit, etc.  Returns the HashMap
// for the cage.
#[doc = include_str!("../docs/remove_cage_from_fdtable.md")]
pub fn remove_cage_from_fdtable(cageid: u64) {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");


    // remove the item first and then we clean up and call their close
    // handlers.
    let myfdrow = FDTABLE.remove(&cageid).unwrap().1;

    // Take only the Some items in here (clippy suggested)
    for entry in myfdrow.into_iter().flatten() {
        _decrement_fdcount(entry);
    }

}

// This removes all fds with the should_cloexec flag set.  They are returned
// in a new hashmap...
#[doc = include_str!("../docs/empty_fds_for_exec.md")]
pub fn empty_fds_for_exec(cageid: u64) {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    let mut myfdrow = FDTABLE.get_mut(&cageid).unwrap();
    // I need to call all the close handlers at the end.  So I need to 
    // get vector of them to do the operation on...
    let mut closevec = Vec::new();

    for item in 0..FD_PER_PROCESS_MAX as usize {
        if myfdrow[item].is_some() && myfdrow[item].unwrap().should_cloexec {
            // handle this in a moment...
            closevec.push(myfdrow[item].unwrap());

            // Always zero out the row before calling their handler
            myfdrow[item] = None;
        }
    }

    // Need to drop the lock, before calling the handlers.
    drop(myfdrow);

    // Now, we can call the close handlers!
    for entry in closevec {
        _decrement_fdcount(entry);
    }

}

// Returns the HashMap returns a copy of the fdtable for a cage.  Useful 
// helper function for a caller that needs to examine the table.  Likely could
// be more efficient by letting the caller borrow this...
#[doc = include_str!("../docs/return_fdtable_copy.md")]
#[must_use] // must use the return value if you call it.
pub fn return_fdtable_copy(cageid: u64) -> HashMap<u64, FDTableEntry> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    let mut myhashmap = HashMap::new();

    let myfdrow = FDTABLE.get(&cageid).unwrap();
    for item in 0..FD_PER_PROCESS_MAX as usize {
        if myfdrow[item].is_some() {
            myhashmap.insert(item as u64,myfdrow[item].unwrap());
        }
    }
    myhashmap
}



/******************* CLOSE SPECIFIC FUNCTIONALITY *******************/

// These indicate what functions should be called upon a virtualfd closing.
// The handler which is called depends on number of (fdkind,underfd) tuples
// that are used across *all instances managed by this library including in
// other cages*.
struct CloseHandlers {
    // Called when close is called, but at least one (fdkind,underfd)
    // reference still remains.  Called with (fdkind,underfd,count)
    intermediate: fn(FDTableEntry,u64),
    // Called when close is called, but at least one (fdkind,underfd)
    // reference still remains. Called with (fdkind,underfd,0)
    last: fn(FDTableEntry,u64),
}


lazy_static! {
    // This holds the user registered handlers they want to have called when
    // a close occurs.  I did this rather than return messy data structures
    // from the close, exec, and exit handlers because it seemed cleaner...
    #[derive(Debug)]
    static ref CLOSEHANDLERTABLE: Mutex<HashMap<u32,CloseHandlers>> = {
        Mutex::new(HashMap::new())
    };
}


#[doc = include_str!("../docs/close_virtualfd.md")]
pub fn close_virtualfd(cageid:u64, virtfd:u64) -> Result<(),threei::RetVal> {

    // Below condition checks if the virtualfd is out of bounds and if yes it throws an error
    // Note that this assumes that all virtualfd numbers returned < FD_PER_PROCESS_MAX 
    if virtualfd >= FD_PER_PROCESS_MAX {
        return Err(threei::Errno::EBADFD as u64);
    }

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    // cloning this so I don't hold a lock and deadlock close handlers
    let mut myfdrow = FDTABLE.get_mut(&cageid).unwrap().clone();


    if myfdrow[virtfd as usize].is_some() {
        let entry = myfdrow[virtfd as usize];

        // Zero out this entry before calling the close handler...
        myfdrow[virtfd as usize] = None;

        FDTABLE.insert(cageid, myfdrow.clone());

        // always _decrement last as it may call the user handler...
        _decrement_fdcount(entry.unwrap());
        return Ok(());
    }
    Err(threei::Errno::EBADFD as u64)
}


// Register a series of helpers to be called for close.  Can be called
// multiple times to override the older helpers.
#[doc = include_str!("../docs/register_close_handlers.md")]
pub fn register_close_handlers(fdkind:u32, intermediate: fn(FDTableEntry,u64), last: fn(FDTableEntry,u64)) {
    // Unlock the table and set the handlers...
    let mut closehandlertable = CLOSEHANDLERTABLE.lock().unwrap();
    let closehandler = CloseHandlers {
        intermediate,
        last,
    };
    // overwrite whatever is in there...
    closehandlertable.insert(fdkind,closehandler);
}


// Helpers to track the count of times each (fdkind,underfd) is used
#[doc(hidden)]
fn _decrement_fdcount(entry:FDTableEntry) {

    let mytuple = (entry.fdkind, entry.underfd);

    let newcount:u64 = FDCOUNT.get(&mytuple).unwrap().value() - 1;

    let intermediatech;
    let lastch;
    // Doing this to release the lock so I can call it recursively...
    let closehandlers = CLOSEHANDLERTABLE.lock().unwrap();
    if let Some(closehandlerentry) = closehandlers.get(&entry.fdkind) {
        intermediatech =  closehandlerentry.intermediate;
        lastch = closehandlerentry.last;
    }
    else {
        // TODO: If at any future point, I wanted to add a "default" handler
        // for all fdkind values, I would add it here...
        intermediatech = NULL_FUNC;
        lastch = NULL_FUNC;
    }
    // release the lock...
    drop(closehandlers);

    if newcount > 0 {
        // Update before calling their close handler in case they do operations
        // inside the close handler which create / close fds...
        FDCOUNT.insert(mytuple,newcount);
        (intermediatech)(entry,newcount);
    }
    else{
        // Remove before calling their close handler in case they do operations
        // inside the close handler which create / close fds...
        FDCOUNT.remove(&mytuple);
        (lastch)(entry,0);
    }
}

// Helpers to track the count of times each (fdkind,underfd) is used
#[doc(hidden)]
fn _increment_fdcount(entry:FDTableEntry) {

    let mytuple = (entry.fdkind, entry.underfd);

    // Get a mutable reference to the entry so we can update it.
    if let Some(mut count) = FDCOUNT.get_mut(&mytuple) {
        *count += 1;
    } else {
        FDCOUNT.insert(mytuple, 1);
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
#[must_use] // must use the return value if you call it.
pub fn _init_fd_set() -> fd_set {
    let raw_fd_set:fd_set;
    unsafe {
        let mut this_fd_set = mem::MaybeUninit::<libc::fd_set>::uninit();
        libc::FD_ZERO(this_fd_set.as_mut_ptr());
        raw_fd_set = this_fd_set.assume_init();
    }
    raw_fd_set
}

#[doc(hidden)]
pub fn _fd_set(fd:u64, thisfdset:&mut fd_set) {
    unsafe{libc::FD_SET(fd as i32,thisfdset)}
}

#[doc(hidden)]
#[must_use] // must use the return value if you call it.
pub fn _fd_isset(fd:u64, thisfdset:&fd_set) -> bool {
    unsafe{libc::FD_ISSET(fd as i32,thisfdset)}
}



// This is a helper that just does a single type (r/w/e) and returns:
//    bithashmap: HashMap<fdkind, (nfds, fd_set)>
//    unhandledhashmap: HashMap<fdkind, HashSet<FDTableEntry>>
//    mappingtable: HashMap<FDTableEntry, virt_fd>
// 
// With this we trivially build the whole function...

// helper to call before calling select beneath you.  Translates your virtfds 
// into a bitmask you may use for select.
// See: https://man7.org/linux/man-pages/man2/select.2.html for details / 
// corner cases about the arguments.
//

// I hate doing this, but don't know how to make this interface better...
#[allow(clippy::type_complexity)]
#[allow(clippy::implicit_hasher)]
#[doc = include_str!("../docs/get_bitmask_for_select.md")]
pub fn get_bitmask_for_select(cageid:u64, nfds:u64, bits:Option<fd_set>, fdkinds:&HashSet<u32>) -> Result<(HashMap<u32,(u64, fd_set)>, HashMap<u32,HashSet<FDTableEntry>>, HashMap<(u32,u64),u64>),threei::RetVal> {
    
    if nfds >= FD_PER_PROCESS_MAX {
        return Err(threei::Errno::EINVAL as u64);
    }

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    // The three things I will return...
    let mut retbittable:HashMap<u32,(u64,fd_set)> = HashMap::new();
    let mut retunparsedtable:HashMap<u32,HashSet<FDTableEntry>> = HashMap::new();
    let mut mappingtable:HashMap<(u32,u64),u64> = HashMap::new();

    // If we were asked to do this on nothing, return empty mappings...
    if bits.is_none() {
        return Ok((retbittable, retunparsedtable, mappingtable));
    }

    let infdset = bits.unwrap();

    // dashmaps are lockless, but usually I would grab a lock on the fdtable
    // here...  
    let binding = FDTABLE.get(&cageid).unwrap();
    let myfdrow = binding.value().clone();

    // Clippy is somehow missing how the virtualfd is being used throughout
    // here.  It's not just a range value
    #[allow(clippy::needless_range_loop)]
    // iterate through the set bits...
    for bit in 0..nfds as usize {
        let pos = bit as u64;
        if _fd_isset(pos,&infdset) {
            if let Some(entry) = myfdrow[bit] {
                
                // I like to do the shorter case first rather than having 
                // it later.
                #[allow(clippy::if_not_else)]
                // Which return set do I go in?
                if !fdkinds.contains(&entry.fdkind) {
                    // Is unparsed...  Clippy's suggestion to insert if missing
                    retunparsedtable.entry(entry.fdkind).or_default();
                    retunparsedtable.get_mut(&entry.fdkind).unwrap().insert(entry);
                    // and update the mappingtable to have the bit from the
                    // original fd...
                    mappingtable.insert((entry.fdkind,entry.underfd),pos);
                }
                else {

                    let startingnfds;
                    let mut startingfdset;

                    // Either initialize it or use what exists
                    if retbittable.contains_key(&entry.fdkind) {
                        (startingnfds, startingfdset) = *retbittable.get(&entry.fdkind).unwrap();
                    }
                    else{
                        startingnfds = 1;
                        // I don't init this above because a fd_set is a large
                        // data structure and would be costly. 
                        startingfdset = _init_fd_set();
                    }

                    // Update the table and the nfds
                    _fd_set(entry.underfd,&mut startingfdset);
                    let newnfds = cmp::max(startingnfds, entry.underfd+1);

                    // and update the mappingtable to have the bit from the
                    // original fd...
                    mappingtable.insert((entry.fdkind,entry.underfd),pos);

                    // insert the item
                    retbittable.insert(entry.fdkind,(newnfds,startingfdset));
                }
            }
            else {
                return Err(threei::Errno::EBADF as u64);
            }
        }
    }
    Ok((retbittable, retunparsedtable, mappingtable))
    
}


#[allow(clippy::type_complexity)]
#[allow(clippy::implicit_hasher)]
#[doc = include_str!("../docs/prepare_bitmasks_for_select.md")]
pub fn prepare_bitmasks_for_select(cageid:u64, nfds:u64, rbits:Option<fd_set>, wbits:Option<fd_set>, ebits:Option<fd_set>, fdkinds:&HashSet<u32>) -> Result<([HashMap<u32,(u64, fd_set)>;3], [HashMap<u32,HashSet<FDTableEntry>>;3], HashMap<(u32,u64),u64>),threei::RetVal> {
    // This is a pretty simple function.  Calls get_bitmask_for_select 
    // repeatedly and combines the results...
    // [HashSet<(u64,u64)>;3]

    // return the error, if need be
    let rresult = get_bitmask_for_select(cageid, nfds, rbits, fdkinds)?;
    let wresult = get_bitmask_for_select(cageid, nfds, wbits, fdkinds)?;
    let eresult = get_bitmask_for_select(cageid, nfds, ebits, fdkinds)?;

    let mut mappingtable = rresult.2;
    mappingtable.extend(wresult.2);
    mappingtable.extend(eresult.2);

    Ok(([rresult.0,wresult.0,eresult.0],[rresult.1,wresult.1,eresult.1],mappingtable))

}


// helper to call after calling select beneath you.  returns the fd_set you 
// need for your return from a select call and the number of unique flags
// set...

// I've given them the hashmap, so don't need flexibility in what they return...
#[allow(clippy::implicit_hasher)]
#[must_use] // must use the return value if you call it.
#[doc = include_str!("../docs/get_one_virtual_bitmask_from_select_result.md")]
pub fn get_one_virtual_bitmask_from_select_result(fdkind:u32, nfds:u64, bits:Option<fd_set>, unprocessedset:HashSet<u64>, startingbits:Option<fd_set>,mappingtable:&HashMap<(u32,u64),u64>) -> (u64, Option<fd_set>) {

    // Note, I don't need the cage_id here because I have the mappingtable...

    assert!(nfds < FD_PER_PROCESS_MAX,"This shouldn't be possible because we shouldn't have returned this previously");

    let mut flagsset = 0;

    if bits.is_none() && unprocessedset.is_empty() {
        return (flagsset,None);
    }

    // I probably should pass a reference to startingbits to avoid copying the 
    // bit structure...
    let mut retbits = match startingbits {
        Some(val) => val,
        None => _init_fd_set(),
    };

    if let Some(inset) = bits {
        for bit in 0..nfds as usize {
            let pos = bit as u64;
            if _fd_isset(pos,&inset)&& !_fd_isset(*mappingtable.get(&(fdkind,pos)).unwrap(),&retbits) {
                flagsset+=1;
                _fd_set(*mappingtable.get(&(fdkind,pos)).unwrap(),&mut retbits);
            }
        }
    }
    for virtfd in unprocessedset {
        if !_fd_isset(virtfd,&retbits) {
            flagsset+=1;
            _fd_set(virtfd,&mut retbits);
        }
    }

    (flagsset,Some(retbits))

}



/********************** POLL SPECIFIC FUNCTIONS **********************/

// helper to call before calling poll beneath you.  replaces the fds in 
// the poll struct with virtual versions and returns the items you need
// to check yourself...
#[allow(clippy::implicit_hasher)]
#[allow(clippy::type_complexity)]
#[doc = include_str!("../docs/convert_virtualfds_for_poll.md")]
#[must_use] // must use the return value if you call it.
pub fn convert_virtualfds_for_poll(cageid:u64, virtualfds:HashSet<u64>) -> (HashMap<u32,HashSet<(u64,FDTableEntry)>>, HashMap<(u32,u64),u64>) {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    let thefdrow = FDTABLE.get(&cageid).unwrap().clone();
    let mut mappingtable:HashMap<(u32,u64),u64> = HashMap::new();
    let mut rethashmap:HashMap<u32,HashSet<(u64,FDTableEntry)>> = HashMap::new();

    
    // BUG?: I'm ignoring the fact that virtualfds can show up multiple times.
    // I'm not sure this actually matters, but I didn't think hard about it.
    for virtfd in virtualfds {
        if let Some(entry) = thefdrow[virtfd as usize] {
            // Insert an empty HashSet, if needed
            rethashmap.entry(entry.fdkind).or_default();
            mappingtable.entry((entry.fdkind,entry.underfd)).or_default();

            rethashmap.get_mut(&entry.fdkind).unwrap().insert((virtfd,entry));
            mappingtable.insert((entry.fdkind,entry.underfd), virtfd);
        }
        else {
            let myentry = FDTableEntry {
                fdkind:FDT_INVALID_FD,
                underfd:virtfd,
                should_cloexec:false,
                perfdinfo:u64::from(FDT_INVALID_FD),
            };

            // Insert an empty HashSet, if needed
            rethashmap.entry(FDT_INVALID_FD).or_default();
            mappingtable.entry((FDT_INVALID_FD,virtfd)).or_default();

            rethashmap.get_mut(&FDT_INVALID_FD).unwrap().insert((virtfd,myentry));
            // Add this because they need to handle it if POLLNVAL is set.
            // An exception should not be raised!!!

            // I will add this to the mapping table, because I do think they
            // may want to raise an exception, etc. based upon this and signal
            // back.  I am setting the underfd to be the virtfd, so I can 
            // reverse this process, if multiple entries like this occur.
            mappingtable.insert((FDT_INVALID_FD,virtfd), virtfd);
        }
    }

    (rethashmap, mappingtable)
}



// helper to call after calling poll.  replaces the fds in the vector
// with virtual ones...
#[doc = include_str!("../docs/convert_poll_result_back_to_virtual.md")]
// I give them the hashmap, so don't need flexibility in what they return...
#[allow(clippy::implicit_hasher)]
#[must_use] // must use the return value if you call it.
pub fn convert_poll_result_back_to_virtual(fdkind:u32,underfd:u64, mappingtable:&HashMap<(u32,u64),u64>) -> Option<u64> {

    // I don't care what cage was used, and don't need to lock anything...
    // I have the mappingtable!
    
    // Should this even be a function?
    mappingtable.get(&(fdkind,underfd)).copied()
}



/********************** EPOLL SPECIFIC FUNCTIONS **********************/


// Supporting epollfds is done by a fdkind which is not set by the user.  
// There are a few complexities here:
// 1) an epollfd gets a virtual file descriptor
// 2) a epollfd can point to any number of other fds of different kinds
// 3) an epollfd can point to epollfds, which can point to other epollfds, etc.
//    and possibly cause a loop to occur (which is an error)
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
// epoll_ctl is complex, but really has the same fundamental problem as 
// epoll_create: the epollfd.
//
// I'll create a new fdkind for epoll.  When epoll_create is called, the 
// caller can decide which fdkinds need to be passed down to the underlying 
// epoll_create call(s).  Similarly, when epoll_ctl is called, one either 
// handles the call internally or uses the underfd for the fdkind...
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


// Okay, so the basic structure is like this:
// 1) epoll_create_helper sets up an epollfd.  You will need to have a unique 
// underfd for each fdkind below you, if you want to call down.   
// 2) my API should track all of the fdkinds where there isn't an underlying
// epollfd
// 3) epoll_ctl / epoll_wait will work on whichever is appropriate
//
// A hashmap of:
//  HashMap<fdkind,underepollfd> for tracking how to call down.
//  HashMap<fdkind,HashMap<virtfd,epoll_event>> seems to make the most sense for the other
//      descriptors.


// a structure that exists for each epoll descriptor to track the underfd(s)
// and parts the user will handle
#[derive(Clone, Debug, Default)]
struct EPollDescriptorInfo {
    // I didn't combine thewe two hashmaps into one because they are used
    // separately and the resulting value type would be too messy...

    underfdhashmap: HashMap<u32,u64>, // The underfd for a specific fdkind.
                                      // Used only when an epoll call will
                                      // call down beneath it.
    userhandledhashmap: HashMap<u32,HashMap<u64,epoll_event>>,
                                      // This has all of the things the user
                                      // will virtualize and handle.  The key
                                      // is the fdkind.  
}

// TODO: I don't clean up this table yet.  I probably should when the last 
// reference to a fd is closed, but this bookkeeping seems excessive at this
// time...
#[derive(Clone, Debug)]
struct EPollTable {
    highestneverusedentry: u64, // Never resets (even after close).  Used to
                                // let us quickly get an unused entry
    thisepolltable: HashMap<u64,EPollDescriptorInfo>, 
}

lazy_static! {

    #[derive(Debug)]
    static ref EPOLLTABLE: Mutex<EPollTable> = {
        let newetable = HashMap::new();
        let m = EPollTable {
            highestneverusedentry:0, 
            thisepolltable:newetable,
        };
        Mutex::new(m)
    };
}

fn _get_epoll_entrynum_or_error(cageid:u64, epfd:u64) -> Result<u64,threei::RetVal> {
    // Is the epfd ok? 
    match FDTABLE.get(&cageid).unwrap()[epfd as usize] {
        None => {
            Err(threei::Errno::EBADF as u64)
        },
        Some(tableentry) => { 
            // You must call this on an epoll fd
            if tableentry.fdkind == FDT_KINDEPOLL {
                Ok(tableentry.underfd)
            }
            else {
                Err(threei::Errno::EINVAL as u64)
            }
        },
    }
}


#[doc = include_str!("../docs/epoll_create_empty.md")]
pub fn epoll_create_empty(cageid:u64, should_cloexec:bool) -> Result<u64,threei::RetVal> {

    let mut ept = EPOLLTABLE.lock().unwrap();

    // return the same errno (EMFile), if we get one 
    let newepollfd = get_unused_virtual_fd(cageid, FDT_KINDEPOLL, ept.highestneverusedentry, should_cloexec, 0)?;

    let newentrynum = ept.highestneverusedentry;
    ept.highestneverusedentry+=1;
    
    // Create a new entry with empty values
    ept.thisepolltable.entry(newentrynum).or_default();
    Ok(newepollfd)

}

#[doc = include_str!("../docs/epoll_add_underfd.md")]
pub fn epoll_add_underfd(cageid:u64, virtepollfd:u64, fdkind:u32, underfd:u64) -> Result<(),threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    let mut ept = EPOLLTABLE.lock().unwrap();

    // get this or error out...
    let epentrynum =  _get_epoll_entrynum_or_error(cageid, virtepollfd)?;

    let myhm = &mut ept.thisepolltable.get_mut(&epentrynum).unwrap().underfdhashmap;

    assert!(!myhm.contains_key(&fdkind),"Adding duplicate underfd to epollfd");
        
    myhm.insert(fdkind,underfd);

    Ok(())

}


#[doc = include_str!("../docs/epoll_get_underfd_hashmap.md")]
pub fn epoll_get_underfd_hashmap(cageid:u64, virtepollfd:u64) -> Result<HashMap<u32,u64>,threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    let ept = EPOLLTABLE.lock().unwrap();

    // get this or error out...
    let epentrynum =  _get_epoll_entrynum_or_error(cageid, virtepollfd)?;

    Ok(ept.thisepolltable.get(&epentrynum).unwrap().underfdhashmap.clone())

}



#[doc = include_str!("../docs/virtualize_epoll_ctl.md")]
pub fn virtualize_epoll_ctl(cageid:u64, epfd:u64, op:i32, virtfd:u64, event:epoll_event) -> Result<(),threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    if epfd == virtfd {
        return Err(threei::Errno::EINVAL as u64);
    }

    // get this or error out...
    let epentrynum =  _get_epoll_entrynum_or_error(cageid, epfd)?;

    // Okay, I know which table entry, now verify the virtfd...


    let virtfdkind:u32;

    if let Some(tableentry) = FDTABLE.get(&cageid).unwrap()[virtfd as usize] {
        // Right now, I don't support this, so error...
        if tableentry.fdkind == FDT_KINDEPOLL {
            // TODO: support EPOLLFDs...
            return Err(threei::Errno::ENOSYS as u64);
        }
        virtfdkind = tableentry.fdkind;
    }
    else {
        // The virtual Fd doesn't exist -- error...
        return Err(threei::Errno::EBADF as u64);
    }

    let mut eptable = EPOLLTABLE.lock().unwrap();
    let userhm = &mut eptable.thisepolltable.get_mut(&epentrynum).unwrap().userhandledhashmap;

    match op {
        EPOLL_CTL_ADD => {
            let thisuserhm = userhm.entry(virtfdkind).or_default();
            if thisuserhm.contains_key(&virtfd) {
                return Err(threei::Errno::EEXIST as u64);
            }
            // BUG: Need to check for ELOOP here once I support EPOLLFDs
            // referencing each other...

            thisuserhm.insert(virtfd, event);
        },
        EPOLL_CTL_MOD => {
            if !userhm.contains_key(&virtfdkind) {
                return Err(threei::Errno::ENOENT as u64);
            }
            let thisuserhm: &mut HashMap<u64, epoll_event> = userhm.get_mut(&virtfdkind).unwrap();
            if !thisuserhm.contains_key(&virtfd) {
                return Err(threei::Errno::ENOENT as u64);
            }
            thisuserhm.insert(virtfd, event);
        },
        EPOLL_CTL_DEL => {
            if !userhm.contains_key(&virtfdkind) {
                return Err(threei::Errno::ENOENT as u64);
            }
            let thisuserhm: &mut HashMap<u64, epoll_event> = userhm.get_mut(&virtfdkind).unwrap();
            if !thisuserhm.contains_key(&virtfd) {
                return Err(threei::Errno::ENOENT as u64);
            }
            thisuserhm.remove(&virtfd);
            // If this was the last entry, delete the key altogether...
            if thisuserhm.is_empty() {
                userhm.remove(&virtfdkind);
            }
        },
        _ => {
            return Err(threei::Errno::EINVAL as u64);
        },
    };
    Ok(())
}


#[doc = include_str!("../docs/get_virtual_epoll_wait_data.md")]
pub fn get_virtual_epoll_wait_data(cageid:u64, epfd:u64) -> Result<HashMap<u32,HashMap<u64,epoll_event>>,threei::RetVal> {

    assert!(FDTABLE.contains_key(&cageid),"Unknown cageid in fdtable access");

    // get this or error out...
    let epentrynum =  _get_epoll_entrynum_or_error(cageid, epfd)?;

    let eptable = EPOLLTABLE.lock().unwrap();
    Ok(eptable.thisepolltable.get(&epentrynum).unwrap().userhandledhashmap.clone())
}



/********************** TESTING HELPER FUNCTION **********************/

#[doc(hidden)]
// Helper to initialize / empty out state so we can test with a clean system...
// This is only used in tests, thus is hidden...
pub fn refresh() {
    FDTABLE.clear();
    FDTABLE.insert(threei::TESTING_CAGEID,vec![Option::None;FD_PER_PROCESS_MAX as usize]);
    let mut closehandlers = CLOSEHANDLERTABLE.lock().unwrap_or_else(|e| {
        CLOSEHANDLERTABLE.clear_poison();
        e.into_inner()
    });
    closehandlers.clear();
    // Note, it doesn't seem that Dashmaps can be poisoned...
}
