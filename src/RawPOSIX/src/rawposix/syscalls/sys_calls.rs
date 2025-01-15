use crate::cage::*;
use std::sync::atomic::Ordering::*;
use crate::fdtables;

pub fn exit_syscall(cageid: u64, status_arg: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> i32 {
    let status = status_arg as i32;
    let _ = fdtables::remove_cage_from_fdtable(cageid);

    // Get the self cage
    let selfcage = get_cage(cageid).unwrap();
    if selfcage.parent != cageid {
        let parent_cage = get_cage(selfcage.parent);
        if let Some(parent) = parent_cage {
            parent.child_num.fetch_sub(1, SeqCst);
            let mut zombie_vec = parent.zombies.write();
            zombie_vec.push(Zombie {cageid: cageid, exit_code: status });
        } else {
            // if parent already exited
            // BUG: we currently do not handle the situation where a parent has exited already
        }
    }

    println!("exit from cageid = {:?}", cageid);
    status
}
