// Filesystem metadata struct
#![allow(dead_code)]

use super::net::NET_METADATA;
use super::syscalls::fs_constants::*;
use super::syscalls::sys_constants::*;
use crate::interface;

use super::cage::Cage;

pub const METADATAFILENAME: &str = "lind.metadata";

pub const LOGFILENAME: &str = "lind.md.log";

pub static LOGMAP: interface::RustLazyGlobal<
    interface::RustRfc<interface::RustLock<Option<interface::EmulatedFileMap>>>,
> = interface::RustLazyGlobal::new(|| interface::RustRfc::new(interface::RustLock::new(None)));

pub static FS_METADATA: interface::RustLazyGlobal<interface::RustRfc<FilesystemMetadata>> =
    interface::RustLazyGlobal::new(|| {
        interface::RustRfc::new(FilesystemMetadata::init_fs_metadata())
    }); //we want to check if fs exists before doing a blank init, but not for now

type FileObjectTable = interface::RustHashMap<usize, interface::EmulatedFile>;
pub static FILEOBJECTTABLE: interface::RustLazyGlobal<FileObjectTable> =
    interface::RustLazyGlobal::new(|| interface::RustHashMap::new());

#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, Debug)]
pub enum Inode {
    File(GenericInode),
    CharDev(DeviceInode),
    Socket(SocketInode),
    Dir(DirectoryInode),
}

#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, Debug)]
pub struct GenericInode {
    pub size: usize,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub linkcount: u32,
    #[serde(skip)]
    //skips serializing and deserializing field, will populate with u32 default of 0 (refcount should not be persisted)
    pub refcount: u32,
    pub atime: u64,
    pub ctime: u64,
    pub mtime: u64,
}

#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, Debug)]
pub struct DeviceInode {
    pub size: usize,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub linkcount: u32,
    #[serde(skip)]
    //skips serializing and deserializing field, will populate with u32 default of 0 (refcount should not be persisted)
    pub refcount: u32,
    pub atime: u64,
    pub ctime: u64,
    pub mtime: u64,
    pub dev: DevNo,
}

#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, Debug)]
pub struct SocketInode {
    pub size: usize,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub linkcount: u32,
    #[serde(skip)]
    pub refcount: u32,
    pub atime: u64,
    pub ctime: u64,
    pub mtime: u64,
}

#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, Debug)]
pub struct DirectoryInode {
    pub size: usize,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub linkcount: u32,
    #[serde(skip)]
    //skips serializing and deserializing field, will populate with u32 default of 0 (refcount should not be persisted)
    pub refcount: u32,
    pub atime: u64,
    pub ctime: u64,
    pub mtime: u64,
    pub filename_to_inode_dict: interface::RustHashMap<String, usize>,
}

#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, Debug)]
pub struct FilesystemMetadata {
    pub nextinode: interface::RustAtomicUsize,
    pub dev_id: u64,
    pub inodetable: interface::RustHashMap<usize, Inode>,
}

pub fn init_filename_to_inode_dict(
    curinode: usize,
    parentinode: usize,
) -> interface::RustHashMap<String, usize> {
    let retval = interface::RustHashMap::new();
    retval.insert(".".to_string(), curinode);
    retval.insert("..".to_string(), parentinode);
    retval
}

impl FilesystemMetadata {
    pub fn blank_fs_init() -> FilesystemMetadata {
        //remove open files?
        let retval = FilesystemMetadata {
            nextinode: interface::RustAtomicUsize::new(STREAMINODE + 1),
            dev_id: 20,
            inodetable: interface::RustHashMap::new(),
        };
        let time = interface::timestamp(); //We do a real timestamp now
        let dirinode = DirectoryInode {
            size: 0,
            uid: DEFAULT_UID,
            gid: DEFAULT_GID,
            //linkcount is how many entries the directory has (as per linux kernel), . and .. making 2 for the root directory initially,
            //plus one to make sure it can never be removed (can be thought of as mount point link)
            //refcount is how many open file descriptors pointing to the directory exist, 0 as no cages exist yet
            mode: S_IFDIR as u32 | S_IRWXA,
            linkcount: 3,
            refcount: 0,
            atime: time,
            ctime: time,
            mtime: time,
            filename_to_inode_dict: init_filename_to_inode_dict(
                ROOTDIRECTORYINODE,
                ROOTDIRECTORYINODE,
            ),
        };
        retval
            .inodetable
            .insert(ROOTDIRECTORYINODE, Inode::Dir(dirinode));

        retval
    }

    // Read file, and deserialize CBOR to FS METADATA
    pub fn init_fs_metadata() -> FilesystemMetadata {
        // Read CBOR from file
        if interface::pathexists(METADATAFILENAME.to_string()) {
            let metadata_fileobj = interface::openmetadata(METADATAFILENAME.to_string()).unwrap();
            let metadatabytes = metadata_fileobj.readfile_to_new_bytes().unwrap();
            metadata_fileobj.close().unwrap();

            // Restore metadata
            interface::serde_deserialize_from_bytes(&metadatabytes).unwrap()
        } else {
            FilesystemMetadata::blank_fs_init()
        }
    }
}

pub fn format_fs() {
    let newmetadata = FilesystemMetadata::blank_fs_init();
    //Because we keep the metadata as a synclazy, it is not possible to completely wipe it and
    //reinstate something over it in-place. Thus we create a new file system, wipe the old one, and
    //then persist our new one. In order to create the new one, because the FS_METADATA does not
    //point to the same metadata that we are trying to create, we need to manually insert these
    //rather than using system calls.

    let mut rootinode = newmetadata.inodetable.get_mut(&1).unwrap(); //get root to populate its dict
    if let Inode::Dir(ref mut rootdir) = *rootinode {
        rootdir.filename_to_inode_dict.insert("dev".to_string(), 2);
        rootdir.linkcount += 1;
    } else {
        unreachable!();
    }
    drop(rootinode);

    let devchildren = interface::RustHashMap::new();
    devchildren.insert("..".to_string(), 1);
    devchildren.insert(".".to_string(), 2);
    devchildren.insert("null".to_string(), 3);
    devchildren.insert("zero".to_string(), 4);
    devchildren.insert("urandom".to_string(), 5);
    devchildren.insert("random".to_string(), 6);

    let tmpchildren = interface::RustHashMap::new();
    tmpchildren.insert("..".to_string(), 1);
    tmpchildren.insert(".".to_string(), 2);

    let time = interface::timestamp(); //We do a real timestamp now
    let devdirinode = Inode::Dir(DirectoryInode {
        size: 0,
        uid: DEFAULT_UID,
        gid: DEFAULT_GID,
        mode: (S_IFDIR | 0o755) as u32,
        linkcount: 3 + 4, //3 for ., .., and the parent dir, 4 is one for each child we will create
        refcount: 0,
        atime: time,
        ctime: time,
        mtime: time,
        filename_to_inode_dict: devchildren,
    }); //inode 2
    let nullinode = Inode::CharDev(DeviceInode {
        size: 0,
        uid: DEFAULT_UID,
        gid: DEFAULT_UID,
        mode: (S_IFCHR | 0o666) as u32,
        linkcount: 1,
        refcount: 0,
        atime: time,
        ctime: time,
        mtime: time,
        dev: DevNo { major: 1, minor: 3 },
    }); //inode 3
    let zeroinode = Inode::CharDev(DeviceInode {
        size: 0,
        uid: DEFAULT_UID,
        gid: DEFAULT_UID,
        mode: (S_IFCHR | 0o666) as u32,
        linkcount: 1,
        refcount: 0,
        atime: time,
        ctime: time,
        mtime: time,
        dev: DevNo { major: 1, minor: 5 },
    }); //inode 4
    let urandominode = Inode::CharDev(DeviceInode {
        size: 0,
        uid: DEFAULT_UID,
        gid: DEFAULT_UID,
        mode: (S_IFCHR | 0o666) as u32,
        linkcount: 1,
        refcount: 0,
        atime: time,
        ctime: time,
        mtime: time,
        dev: DevNo { major: 1, minor: 9 },
    }); //inode 5
    let randominode = Inode::CharDev(DeviceInode {
        size: 0,
        uid: DEFAULT_UID,
        gid: DEFAULT_UID,
        mode: (S_IFCHR | 0o666) as u32,
        linkcount: 1,
        refcount: 0,
        atime: time,
        ctime: time,
        mtime: time,
        dev: DevNo { major: 1, minor: 8 },
    }); //inode 6
    let tmpdirinode = Inode::Dir(DirectoryInode {
        size: 0,
        uid: DEFAULT_UID,
        gid: DEFAULT_GID,
        mode: (S_IFDIR | 0o755) as u32,
        linkcount: 3 + 4,
        refcount: 0,
        atime: time,
        ctime: time,
        mtime: time,
        filename_to_inode_dict: tmpchildren,
    }); //inode 7
    newmetadata
        .nextinode
        .store(8, interface::RustAtomicOrdering::Relaxed);
    newmetadata.inodetable.insert(2, devdirinode);
    newmetadata.inodetable.insert(3, nullinode);
    newmetadata.inodetable.insert(4, zeroinode);
    newmetadata.inodetable.insert(5, urandominode);
    newmetadata.inodetable.insert(6, randominode);
    newmetadata.inodetable.insert(7, tmpdirinode);

    let _logremove = interface::removefile(LOGFILENAME.to_string());

    persist_metadata(&newmetadata);
}

pub fn load_fs() {
    // If the metadata file exists, just close the file for later restore
    // If it doesn't, lets create a new one, load special files, and persist it.
    if interface::pathexists(METADATAFILENAME.to_string()) {
        let metadata_fileobj = interface::openmetadata(METADATAFILENAME.to_string()).unwrap();
        metadata_fileobj.close().unwrap();

        // if we have a log file at this point, we need to sync it with the existing metadata
        if interface::pathexists(LOGFILENAME.to_string()) {
            let log_fileobj = interface::openmetadata(LOGFILENAME.to_string()).unwrap();
            // read log file and parse count
            let mut logread = log_fileobj.readfile_to_new_bytes().unwrap();
            let logsize = interface::convert_bytes_to_size(&logread[0..interface::COUNTMAPSIZE]);

            // create vec of log file bounded by indefinite encoding bytes (0x9F, 0xFF)
            let mut logbytes: Vec<u8> = Vec::new();
            logbytes.push(0x9F);
            logbytes.extend_from_slice(
                &mut logread[interface::COUNTMAPSIZE..(interface::COUNTMAPSIZE + logsize)],
            );
            logbytes.push(0xFF);
            let mut logvec: Vec<(usize, Option<Inode>)> =
                interface::serde_deserialize_from_bytes(&logbytes).unwrap();

            // drain the vector and deserialize into pairs of inodenum + inodes,
            // if the inode exists, add it, if not, remove it
            // keep track of the largest inodenum we see so we can update the nextinode counter
            let mut max_inodenum = FS_METADATA
                .nextinode
                .load(interface::RustAtomicOrdering::Relaxed);
            for serialpair in logvec.drain(..) {
                let (inodenum, inode) = serialpair;
                match inode {
                    Some(inode) => {
                        max_inodenum = interface::rust_max(max_inodenum, inodenum);
                        FS_METADATA.inodetable.insert(inodenum, inode);
                    }
                    None => {
                        FS_METADATA.inodetable.remove(&inodenum);
                    }
                }
            }

            // update the nextinode counter to avoid collisions
            FS_METADATA
                .nextinode
                .store(max_inodenum + 1, interface::RustAtomicOrdering::Relaxed);

            let _logclose = log_fileobj.close();
            let _logremove = interface::removefile(LOGFILENAME.to_string());

            // clean up broken links
            fsck();
        }
    } else {
        if interface::pathexists(LOGFILENAME.to_string()) {
            println!("Filesystem in very corrupted state: log existed but metadata did not!");
        }
        format_fs();
    }

    // then recreate the log
    create_log();
}

pub fn fsck() {
    FS_METADATA.inodetable.retain(|_inodenum, inode_obj| {
        match inode_obj {
            Inode::File(ref mut normalfile_inode) => normalfile_inode.linkcount != 0,
            Inode::Dir(ref mut dir_inode) => {
                //2 because . and .. always contribute to the linkcount of a directory
                dir_inode.linkcount > 2
            }
            Inode::CharDev(ref mut char_inodej) => char_inodej.linkcount != 0,
            Inode::Socket(_) => false,
        }
    });
}

pub fn create_log() {
    // reinstantiate the log file and assign it to the metadata struct
    let log_mapobj = interface::mapfilenew(LOGFILENAME.to_string()).unwrap();
    let mut logobj = LOGMAP.write();
    logobj.replace(log_mapobj);
}

// Serialize New Metadata to CBOR, write to logfile
pub fn log_metadata(metadata: &FilesystemMetadata, inodenum: usize) {
    let serialpair: (usize, Option<&Inode>);
    let entrybytes;

    // pack and serialize log entry
    if let Some(inode) = metadata.inodetable.get(&inodenum) {
        serialpair = (inodenum, Some(&*inode));
        entrybytes = interface::serde_serialize_to_bytes(&serialpair).unwrap();
    } else {
        serialpair = (inodenum, None);
        entrybytes = interface::serde_serialize_to_bytes(&serialpair).unwrap();
    }

    // write to file
    let mut mapopt = LOGMAP.write();
    let map = mapopt.as_mut().unwrap();
    map.write_to_map(&entrybytes).unwrap();
}

// Serialize Metadata Struct to CBOR, write to file
pub fn persist_metadata(metadata: &FilesystemMetadata) {
    // Serialize metadata to string
    let metadatabytes = interface::serde_serialize_to_bytes(&metadata).unwrap();

    // remove file if it exists, assigning it to nothing to avoid the compiler yelling about unused result
    let _ = interface::removefile(METADATAFILENAME.to_string());

    // write to file
    let mut metadata_fileobj = interface::openmetadata(METADATAFILENAME.to_string()).unwrap();
    metadata_fileobj
        .writefile_from_bytes(&metadatabytes)
        .unwrap();
    metadata_fileobj.close().unwrap();
}

pub fn convpath(cpath: &str) -> interface::RustPathBuf {
    interface::RustPathBuf::from(cpath)
}

/// This function resolves the absolute path of a directory from its inode number in a filesystem.
/// Here's how it operates:
///
/// - Starts from the given inode and fetches its associated metadata from the filesystem's inode table.
///
/// - Verifies that the inode represents a directory.
///
/// - Attempts to find the parent directory by looking for the ".." entry in the current directory's entries.
///
/// - Retrieves the directory name associated with the current inode using the `filenamefrominode` function and prepends it to the `path_string`.
///
/// - Continues this process recursively, updating the current inode to the parent inode, and accumulating directory names in the `path_string`.
///
/// - Stops when it reaches the root directory, where it prepends a "/" to the `path_string` and returns the complete path.
///
/// This function effectively constructs the absolute path by backtracking the parent directories. However, if any issues arise during this process, such as missing metadata or inability to find the parent directory, it returns `None`.
pub fn pathnamefrominodenum(inodenum: usize) -> Option<String> {
    let mut path_string = String::new();
    let mut first_iteration = true;
    let mut current_inodenum = inodenum;

    loop {
        let mut thisinode = match FS_METADATA.inodetable.get_mut(&current_inodenum) {
            Some(inode) => inode,
            None => {
                return None;
            }
        };

        match *thisinode {
            Inode::Dir(ref mut dir_inode) => {
                // We try to get the parent directory inode.
                if let Some(parent_dir_inode) = dir_inode.filename_to_inode_dict.get("..") {
                    // If the parent node is 1 (indicating the root directory) and this is not the first iteration, this indicates that we have arrived at the root directory. Here we add a '/' to the beginning of the path string and return it.
                    if *parent_dir_inode == (1 as usize) {
                        if !first_iteration {
                            path_string.insert(0, '/');
                            return Some(path_string);
                        }
                        first_iteration = false;
                    }

                    match filenamefrominode(*parent_dir_inode, current_inodenum) {
                        Some(filename) => {
                            path_string = filename + "/" + &path_string;
                            current_inodenum = *parent_dir_inode;
                        }
                        None => return None,
                    };
                } else {
                    return None;
                }
            }
            _ => {
                return None;
            }
        }
    }
}

// Find the file by the given inode number in the given directory
pub fn filenamefrominode(dir_inode_no: usize, target_inode: usize) -> Option<String> {
    let cur_node = Some(FS_METADATA.inodetable.get(&dir_inode_no).unwrap());

    match &*cur_node.unwrap() {
        Inode::Dir(d) => {
            let mut target_variable_name: Option<String> = None;

            for entry in d.filename_to_inode_dict.iter() {
                if entry.value() == &target_inode {
                    target_variable_name = Some(entry.key().to_owned());
                    break;
                }
            }
            return target_variable_name;
        }
        _ => return None,
    }
}

//returns tuple consisting of inode number of file (if it exists), and inode number of parent (if it exists)
pub fn metawalkandparent(path: &interface::RustPath) -> (Option<usize>, Option<usize>) {
    let mut curnode = Some(FS_METADATA.inodetable.get(&ROOTDIRECTORYINODE).unwrap());
    let mut inodeno = Some(ROOTDIRECTORYINODE);
    let mut previnodeno = None;

    //Iterate over the components of the pathbuf in order to walk the file tree
    for comp in path.components() {
        match comp {
            //We've already done what initialization needs to be done
            interface::RustPathComponent::RootDir => {}

            interface::RustPathComponent::Normal(f) => {
                //If we're trying to get the child of a nonexistent directory, exit out
                if inodeno.is_none() {
                    return (None, None);
                }
                match &*curnode.unwrap() {
                    Inode::Dir(d) => {
                        previnodeno = inodeno;

                        //populate child inode number from parent directory's inode dict
                        inodeno = match d
                            .filename_to_inode_dict
                            .get(&f.to_str().unwrap().to_string())
                        {
                            Some(num) => {
                                curnode = FS_METADATA.inodetable.get(&num);
                                Some(*num)
                            }

                            //if no such child exists, update curnode, inodeno accordingly so that
                            //we can check against none as we do at the beginning of the Normal match arm
                            None => {
                                curnode = None;
                                None
                            }
                        }
                    }
                    //if we're trying to get a child of a non-directory inode, exit out
                    _ => {
                        return (None, None);
                    }
                }
            }

            //If it's a component of the pathbuf that we don't expect given a normed path, exit out
            _ => {
                return (None, None);
            }
        }
    }
    //return inode number and it's parent's number
    (inodeno, previnodeno)
}
pub fn metawalk(path: &interface::RustPath) -> Option<usize> {
    metawalkandparent(path).0
}
pub fn normpath(origp: interface::RustPathBuf, cage: &Cage) -> interface::RustPathBuf {
    //If path is relative, prefix it with the current working directory, otherwise populate it with rootdir
    let mut newp = if origp.is_relative() {
        (**cage.cwd.read()).clone()
    } else {
        interface::RustPathBuf::from("/")
    };

    for comp in origp.components() {
        match comp {
            //if we have a normal path component, push it on to our normed path
            interface::RustPathComponent::Normal(_) => {
                newp.push(comp);
            }

            //if we have a .. path component, pop the last component off our normed path
            interface::RustPathComponent::ParentDir => {
                newp.pop();
            }

            //if we have a . path component (Or a root dir or a prefix(?)) do nothing
            _ => {}
        };
    }
    newp
}

pub fn remove_domain_sock(truepath: interface::RustPathBuf) {
    match metawalkandparent(truepath.as_path()) {
        //If the file does not exist
        (None, ..) => {
            panic!("path does not exist")
        }
        //If the file exists but has no parent, it's the root directory
        (Some(_), None) => {
            panic!("cannot unlink root directory")
        }

        //If both the file and the parent directory exists
        (Some(inodenum), Some(parentinodenum)) => {
            Cage::remove_from_parent_dir(parentinodenum, &truepath);

            FS_METADATA.inodetable.remove(&inodenum);
            NET_METADATA.domsock_paths.remove(&truepath);
        }
    }
}

pub fn incref_root() {
    if let Inode::Dir(ref mut rootdir_dirinode_obj) =
        *(FS_METADATA.inodetable.get_mut(&ROOTDIRECTORYINODE).unwrap())
    {
        rootdir_dirinode_obj.refcount += 1;
    } else {
        panic!("Root directory inode was not a directory");
    }
}

pub fn decref_dir(cwd_container: &interface::RustPathBuf) {
    if let Some(cwdinodenum) = metawalk(&cwd_container) {
        if let Inode::Dir(ref mut cwddir) = *(FS_METADATA.inodetable.get_mut(&cwdinodenum).unwrap())
        {
            cwddir.refcount -= 1;

            //if the directory has been removed but this cwd was the last open handle to it
            if cwddir.refcount == 0 && cwddir.linkcount == 0 {
                FS_METADATA.inodetable.remove(&cwdinodenum);
            }
        } else {
            panic!("Cage had a cwd that was not a directory!");
        }
    } else {
        panic!("Cage had a cwd which did not exist!");
    } //we probably want to handle this case, maybe cwd should be an inode number?? Not urgent
}
