#![feature(lazy_cell)]
#![feature(rustc_private)] //for private crate imports for tests
#![feature(vec_into_raw_parts)]
#![feature(duration_constants)]
#![allow(unused)]

/// Author: Jonathan Singer
///
/// This file provides a command line interface for interacting in certain ways with the lind file
/// system from the host, such as copying files from the host into lind, removing files and
/// directories, and listing files in the lind fs, and more
///
/// This interface should be sufficient for anything we'd need to do between lind and the host
use std::env;
use std::iter::repeat;

mod interface;
mod lib_fs_utils;
mod safeposix;
use lib_fs_utils::*;
use safeposix::{
    cage::*,
    dispatcher::{lindrustfinalize, lindrustinit},
    filesystem::*,
};

fn lind_tree(cage: &Cage, path: &str, indentlevel: usize) {
    let mut lindstat_res: StatData = StatData::default();
    let stat_us = cage.stat_syscall(path, &mut lindstat_res);
    if stat_us == 0 {
        if !is_dir(lindstat_res.st_mode) {
            eprintln!("Tree must be run on a directory!");
            return;
        }

        //visit the children of this directory, and show them all as being children of this directory in the tree
        visit_children(
            cage,
            path,
            Some(indentlevel),
            |childcage, childpath, isdir, childindentlevelopt| {
                let childindentlevel = childindentlevelopt.unwrap();
                //lines to connect non-parent ancestors to their remaining children(if any)
                print!("{}", "|   ".repeat(childindentlevel));
                //line to connect parent to its child
                print!("{}", "|---");
                //actually print out file name
                println!("{}", childpath);

                //recursive call for child
                if isdir {
                    lind_tree(childcage, childpath, childindentlevel + 1);
                }
            },
        );
    } else {
        eprintln!("No such directory exists!");
    }
}

fn lind_ls(cage: &Cage, path: &str) {
    let mut lindstat_res: StatData = StatData::default();
    let stat_us = cage.stat_syscall(path, &mut lindstat_res);

    if stat_us == 0 {
        if is_dir(lindstat_res.st_mode) {
            //for each child, if it's a directory, print its name with a slash, otherwise omit the slash
            visit_children(cage, path, None, |_childcage, childpath, isdir, _| {
                if isdir {
                    print!("{}/ ", childpath);
                } else {
                    print!("{} ", childpath);
                }
            });
        } else {
            print!("{} ", path);
        }
        println!();
    } else {
        eprintln!("No such file exists!");
    }
}

fn print_usage() {
    println!(
        "
Usage: lind_fs_utils [commandname] [arguments...]

Where commandname is one of the following:

cp [hostsource] [linddest]      : Copies files from the host file system into the lind filesystem.
                                  For example, cp bar/etc/passwd /etc/passwd will copy the
                                  former file in the host file system to the latter in lind's fs.
                                  Directories are handled recursively, cp bar/etc /etc/ will make a
                                  directory at /etc in the lind fs, and then populate it with all
                                  of the files in the root fs.
deltree [linddir]               : Delete a directory on the lind file system and all it contains
format                          : Make a new blank fs, removing the current one
help                            : Print this message
ls [lindpath]                   : List the contents of a lind file system directory
mkdir [linddir1...]             : Create a lind file system directory (for each arg)
rm [lindfile1...]               : Delete a file on the lind file system
rmdir [linddir1...]             : Delete a directory on the lind file system
tree [startlindpath]            : Print the lindfs file tree starting at the specified directory
                                  Assumes root directory if no starting path is specified.
update [hostsource] [linddest]  : Copies files from the host file system into the lind filesystem.
                                  Will not copy files if the host and lind files are identical.
                                  For example, update bar/etc/passwd /etc/passwd will copy the
                                  former file in the host file system to the latter in lind's fs if
                                  the latter does not exist or is not identical to the former.
                                  Directories are handled recursively, cp bar/etc /etc/ will make a
                                  directory at /etc in the lind fs, and then populate it with all
                                  of the files in the root fs, with identical files being skipped.
"
    );
}

fn main() {
    lindrustinit(0); // no verbosity
    let mut args = env::args();
    let utilcage = Cage {
        cageid: 0,
        cwd: interface::RustLock::new(interface::RustRfc::new(interface::RustPathBuf::from("/"))),
        parent: 0,
        filedescriptortable: init_fdtable(),
        cancelstatus: interface::RustAtomicBool::new(false),
        getgid: interface::RustAtomicI32::new(-1),
        getuid: interface::RustAtomicI32::new(-1),
        getegid: interface::RustAtomicI32::new(-1),
        geteuid: interface::RustAtomicI32::new(-1),
        rev_shm: interface::Mutex::new(vec![]),
        mutex_table: interface::RustLock::new(vec![]),
        cv_table: interface::RustLock::new(vec![]),
        sem_table: interface::RustHashMap::new(),
        thread_table: interface::RustHashMap::new(),
        signalhandler: interface::RustHashMap::new(),
        sigset: interface::RustHashMap::new(),
        pendingsigset: interface::RustHashMap::new(),
        main_threadid: interface::RustAtomicU64::new(0),
        interval_timer: interface::IntervalTimer::new(0),
    };

    args.next(); //first arg is executable, we don't care
    let command = if let Some(cmd) = args.next() {
        cmd
    } else {
        print_usage();
        return; //print usage
    };

    match command.as_str() {
        "help" | "usage" => {
            print_usage();
        }

        "cp" => {
            let source = args.next().expect("cp needs 2 arguments");
            let dest = args.next().expect("cp needs 2 arguments");
            args.next()
                .and_then::<String, fn(String) -> Option<String>>(|_| {
                    panic!("cp cannot take more than 2 arguments")
                });
            cp_dir_into_lind(
                &utilcage,
                interface::RustPath::new(&source),
                dest.as_str(),
                true,
            );
        }

        "update" => {
            let source = args.next().expect("update needs 2 arguments");
            let dest = args.next().expect("update needs 2 arguments");
            args.next()
                .and_then::<String, fn(String) -> Option<String>>(|_| {
                    panic!("update cannot take more than 2 arguments")
                });
            update_dir_into_lind(&utilcage, interface::RustPath::new(&source), dest.as_str());
        }

        "ls" => {
            let file = args.next().expect("ls needs 1 argument");
            args.next()
                .and_then::<String, fn(String) -> Option<String>>(|_| {
                    panic!("ls cannot take more than 1 argument")
                });
            lind_ls(&utilcage, file.as_str());
        }

        "tree" => {
            let rootdir = if let Some(dirstr) = args.next() {
                dirstr
            } else {
                "/".to_owned()
            };
            println!("{}", rootdir);
            lind_tree(&utilcage, rootdir.as_str(), 0);
        }

        "format" => {
            lind_deltree(&utilcage, "/"); //This doesn't actually fully remove all of the linddata files... TODO: debug

            let mut logobj = LOGMAP.write();
            let log = logobj.take().unwrap();
            let _close = log.close().unwrap();
            drop(logobj);
            let _logremove = interface::removefile(LOGFILENAME.to_string());

            format_fs();
            return;
        }

        "deltree" => {
            let rootdir = args.next().expect("deltree needs 1 argument");
            args.next()
                .and_then::<String, fn(String) -> Option<String>>(|_| {
                    panic!("deltree cannot take more than 1 argument")
                });
            lind_deltree(&utilcage, rootdir.as_str());
        }

        "rm" => {
            for file in args {
                utilcage.unlink_syscall(file.as_str());
            }
        }

        "mkdir" => {
            for dir in args {
                utilcage.mkdir_syscall(dir.as_str(), S_IRWXA);
            }
        }

        "rmdir" => {
            for dir in args {
                utilcage.chmod_syscall(dir.as_str(), S_IRWXA);
                utilcage.rmdir_syscall(dir.as_str());
            }
        }

        _ => {
            eprintln!("Error, command unknown");
            return;
        }
    }
    lindrustfinalize();
}
