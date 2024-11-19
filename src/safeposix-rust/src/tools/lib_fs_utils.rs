#![allow(dead_code)] //suppress warning for these functions not being used in main library target

use std::ffi::CStr;
use std::fs::File;
use std::io::{prelude, Read};
use std::os::raw::c_char;

use crate::interface;
use crate::interface::errnos::{syscall_error, Errno};
use crate::interface::types::{ClippedDirent, CLIPPED_DIRENT_SIZE};
use crate::safeposix::{cage::*, filesystem::*};

const LINUX_MAX_RW_COUNT: usize = 0x7FFFF000;

//we currently handle symlinks as normal files

pub fn update_dir_into_lind(cage: &Cage, hostfilepath: &interface::RustPath, lindfilepath: &str) {
    if hostfilepath.exists() {
        if let Ok(_) = hostfilepath.read_link() {
            println!("following symlink at {:?} on host fs", hostfilepath);
        } //if read_link succeeds it's a symlink, whose destination must exist because of the nature of the .exists function
    } else {
        eprintln!("Cannot locate file on host fs: {:?}", hostfilepath);
        return;
    }

    //update directly if not a directory on the host, otherwise recursively handle children
    if hostfilepath.is_file() {
        update_into_lind(cage, hostfilepath, lindfilepath);
    } else {
        let children = hostfilepath.read_dir().unwrap();
        for wrappedchild in children {
            let child = wrappedchild.unwrap();
            let newlindpath = if lindfilepath.ends_with("/") {
                format!("{}{}", lindfilepath, child.file_name().to_str().unwrap())
            } else {
                format!("{}/{}", lindfilepath, child.file_name().to_str().unwrap())
            };
            update_dir_into_lind(cage, child.path().as_path(), newlindpath.as_str());
        }
    }
}

fn update_into_lind(cage: &Cage, hostfilepath: &interface::RustPath, lindfilepath: &str) {
    if !hostfilepath.exists() || !hostfilepath.is_file() {
        println!(
            "{:?} does not exist or is not a regular file, skipping",
            hostfilepath
        );
        return;
    }
    let fmetadata = hostfilepath.metadata().unwrap();

    let host_size = fmetadata.len();
    let mut lindstat_res: StatData = StatData::default();
    let stat_us = cage.stat_syscall(lindfilepath, &mut lindstat_res);

    let lind_exists;
    let lind_isfile;
    let lind_size;
    if stat_us < 0 {
        lind_exists = false;
        lind_isfile = false;
        lind_size = 0;
    } else {
        lind_exists = true;
        lind_isfile = is_reg(lindstat_res.st_mode);
        lind_size = lindstat_res.st_size;
    }

    if lind_exists && !lind_isfile {
        println!(
            "{:?} on lind file system is not a regular file, skipping",
            hostfilepath
        );
        return;
    }

    //compare files to tell whether they are identical
    let samefile = if host_size as usize == lind_size {
        let mut hostslice = vec![0u8; lind_size];
        let mut lindslice = vec![0u8; lind_size];
        let mut hostfile = File::open(hostfilepath).unwrap();
        hostfile.read(hostslice.as_mut_slice()).unwrap();
        let lindfd = cage.open_syscall(lindfilepath, O_RDONLY | O_CREAT, S_IRWXA);
        cage.read_syscall(lindfd, lindslice.as_mut_ptr(), lind_size);
        cage.close_syscall(lindfd);
        hostslice == lindslice
    } else {
        false
    };

    //if they are not the same file, remove the lind file and replace it with the host file
    if !samefile {
        if lind_exists {
            cage.unlink_syscall(lindfilepath);
            println!("removing {} on lind file system", lindfilepath);
        }
        cp_into_lind(cage, hostfilepath, lindfilepath, true);
    } else {
        println!(
            "Same files on host and lind--{:?} and {}, skipping",
            hostfilepath, lindfilepath
        );
    }
}

pub fn cp_dir_into_lind(
    cage: &Cage,
    hostfilepath: &interface::RustPath,
    lindfilepath: &str,
    create_missing_dirs: bool,
) {
    if hostfilepath.exists() {
        if let Ok(_) = hostfilepath.read_link() {
            println!("following symlink at {:?} on host fs", hostfilepath);
        } //if read_link succeeds it's a symlink, whose destination must exist because of the nature of the .exists function
    } else {
        eprintln!("Cannot locate file on host fs: {:?}", hostfilepath);
        return;
    }

    //update directly if not a directory on the host, otherwise recursively handle children
    if hostfilepath.is_file() {
        cp_into_lind(cage, hostfilepath, lindfilepath, create_missing_dirs);
    } else if hostfilepath.is_dir() {
        let children = hostfilepath.read_dir().unwrap();
        for wrappedchild in children {
            let child = wrappedchild.unwrap();
            let newlindpath = if lindfilepath.ends_with("/") {
                format!("{}{}", lindfilepath, child.file_name().to_str().unwrap())
            } else {
                format!("{}/{}", lindfilepath, child.file_name().to_str().unwrap())
            };
            cp_dir_into_lind(
                cage,
                child.path().as_path(),
                newlindpath.as_str(),
                create_missing_dirs,
            );
        }
    }
}

fn cp_into_lind(
    cage: &Cage,
    hostfilepath: &interface::RustPath,
    lindfilepath: &str,
    create_missing_dirs: bool,
) {
    if !hostfilepath.exists() {
        eprintln!("Cannot locate file on host fs: {:?}", hostfilepath);
        return;
    }
    if !hostfilepath.is_file() {
        eprintln!("File is not a regular file on host fs: {:?}", hostfilepath);
        return;
    }

    let lindtruepath = normpath(convpath(lindfilepath), cage);

    //if a directory in the lindfilepath does not exist in the lind file system, create it!
    let mut ancestor = interface::RustPathBuf::from("/");
    for component in lindtruepath.parent().unwrap().components() {
        ancestor.push(component);
        let mut lindstat_res: StatData = StatData::default();

        //check whether file exists
        let stat_us = cage.stat_syscall(ancestor.to_str().unwrap(), &mut lindstat_res);
        if stat_us == 0 {
            if !is_dir(lindstat_res.st_mode) {
                eprintln!("Fatal error in trying to create child of non-directory file");
                return;
            }
            continue;
        }
        if stat_us != -(Errno::ENOENT as i32) {
            eprintln!("Fatal error in trying to get lind file path");
            return;
        }

        //check whether we are supposed to create missing directories, and whether we'd be
        //clobbering anything to do so (if so error out)
        if create_missing_dirs {
            if cage.mkdir_syscall(ancestor.to_str().unwrap(), S_IRWXA) != 0 {
                //let's not mirror stat data
                eprintln!("Lind fs path does not exist but should not be created (is rooted at non-directory) {:?}", ancestor);
                return;
            }
        } else {
            eprintln!(
                "Lind fs path does not exist but should not be created {:?}",
                ancestor
            );
            return;
        }
    }

    //copy file contents into lind file system
    let mut host_fileobj = File::open(hostfilepath).unwrap();
    let mut filecontents: Vec<u8> = Vec::new();
    host_fileobj.read_to_end(&mut filecontents).unwrap();

    let lindfd = cage.open_syscall(
        lindtruepath.to_str().unwrap(),
        O_CREAT | O_TRUNC | O_WRONLY,
        S_IRWXA,
    );
    assert!(lindfd >= 0);

    let veclen = filecontents.len();
    let mut writtenlen: usize = 0;

    //on Linux, write() (and similar system calls) will transfer at most 0x7ffff000 (2,147,479,552) bytes
    //dividing filecontents into chunks of 0x7ffff000 (2,147,479,552) bytes and writing each chunk
    for chunk in filecontents.chunks(LINUX_MAX_RW_COUNT) {
        writtenlen += cage.write_syscall(lindfd, chunk.as_ptr(), chunk.len()) as usize;
    }
    //confirm that write succeeded
    assert_eq!(veclen, writtenlen);

    //get diagnostic data to print
    let mut lindstat_res: StatData = StatData::default();
    let _stat_us = cage.fstat_syscall(lindfd, &mut lindstat_res);
    let inode = lindstat_res.st_ino;

    assert_eq!(cage.close_syscall(lindfd), 0);

    println!("Copied {:?} as {} ({})", hostfilepath, lindfilepath, inode);
}

pub fn visit_children(
    cage: &Cage,
    path: &str,
    arg: Option<usize>,
    visitor: fn(&Cage, &str, bool, Option<usize>),
) {
    //get buffer in which getdents will write its stuff
    let mut bigbuffer = [0u8; 65536];
    let dentptr = bigbuffer.as_mut_ptr();

    let dirfd = cage.open_syscall(path, O_RDONLY, 0);
    assert!(dirfd >= 0);

    loop {
        let direntres = cage.getdents_syscall(dirfd, dentptr, 65536);

        //if we've read every entry in this directory, we're done
        if direntres == 0 {
            break;
        }

        let mut dentptrindex = 0isize;

        //while there are still more entries to read
        while dentptrindex < direntres as isize {
            //get information for where the next entry is (if relevant)
            let clipped_dirent_ptr = dentptr.wrapping_offset(dentptrindex) as *mut ClippedDirent;
            let clipped_dirent = unsafe { &*clipped_dirent_ptr };

            //get the file name for the child
            let cstrptr = dentptr.wrapping_offset(dentptrindex + CLIPPED_DIRENT_SIZE as isize);
            let filenamecstr = unsafe { CStr::from_ptr(cstrptr as *const c_char) };
            let filenamestr = filenamecstr.to_str().unwrap();

            dentptrindex += clipped_dirent.d_reclen as isize;

            //ignore these entries
            if filenamestr == "." || filenamestr == ".." {
                continue;
            }

            let fullstatpath = if path.ends_with("/") {
                [path, filenamestr].join("")
            } else {
                [path, "/", filenamestr].join("")
            };

            //stat to tell whether it's a directory
            let mut lindstat_res: StatData = StatData::default();
            let _stat_us = cage.stat_syscall(fullstatpath.as_str(), &mut lindstat_res);

            //call the visitor function on the child path
            visitor(
                cage,
                fullstatpath.as_str(),
                is_dir(lindstat_res.st_mode),
                arg,
            );
        }
    }
    cage.close_syscall(dirfd);
}

pub fn lind_deltree(cage: &Cage, path: &str) {
    let mut lindstat_res: StatData = StatData::default();
    let stat_us = cage.stat_syscall(path, &mut lindstat_res);

    if stat_us == 0 {
        if !is_dir(lindstat_res.st_mode) {
            cage.unlink_syscall(path);
            return;
        } else {
            //remove all children recursively
            visit_children(cage, path, None, |childcage, childpath, isdir, _| {
                if isdir {
                    lind_deltree(childcage, childpath);
                } else {
                    childcage.unlink_syscall(childpath);
                }
            });

            //remove specified directory now that it is empty
            cage.chmod_syscall(path, S_IRWXA);
            cage.rmdir_syscall(path);
        }
    } else {
        eprintln!("No such directory exists!");
    }
}
